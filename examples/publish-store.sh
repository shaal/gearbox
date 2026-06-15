#!/usr/bin/env bash
# Stage a self-hostable cog store for static HTTPS hosting (e.g. arcade.shaal.dev).
#
# Produces, under --out, exactly the tree a Seed fetches over `https://`:
#   <out>/store.json            store-info (self-signed; the key a user pins via TOFU)
#   <out>/app-registry.json     the signed catalog
#   <out>/cogs/<arch>/...        every binary + asset the catalog references, at its catalog path
# Each referenced artifact is re-hashed against the signed catalog before it is staged, so you
# can never upload a store whose bytes diverge from what you signed.
#
# Optional: --attest signs a provenance+SBOM attestation per binary; --audit appends a signed,
# hash-chained publish record (and --sign-head checkpoints it). All reuse the gearbox binary.
#
# The signing seed is read from --seed-file (32-byte ed25519 seed, hex) and is NEVER printed or
# committed. Use --gen-key once to create one.
#
#   examples/publish-store.sh \
#     --store-id shaal-arcade --name "Shaal Arcade — games for Cognitum" \
#     --base-url https://arcade.shaal.dev --key-id shaal-arcade-2026 \
#     --seed-file ./arcade-signing.key --gen-key \
#     --cogs-dir ./manifests --artifacts-dir ./staged \
#     --generated-at 2026-06-15T00:00:00Z --out ./public --attest
set -euo pipefail
cd "$(dirname "$0")/.."
GB="${GEARBOX:-crates/gearbox/target/debug/gearbox}"
[ -x "$GB" ] || { echo "build first: cargo build --manifest-path crates/gearbox/Cargo.toml"; exit 1; }

# ---- args ----
STORE_ID="" NAME="" BASE_URL="" KEY_ID="" SEED_FILE="" COGS_DIR="" ARTIFACTS_DIR=""
GENERATED_AT="" OUT="" GEN_KEY=0 ATTEST=0 AUDIT="" SIGN_HEAD=0
while [ $# -gt 0 ]; do
  case "$1" in
    --store-id) STORE_ID="$2"; shift 2;;
    --name) NAME="$2"; shift 2;;
    --base-url) BASE_URL="$2"; shift 2;;
    --key-id) KEY_ID="$2"; shift 2;;
    --seed-file) SEED_FILE="$2"; shift 2;;
    --cogs-dir) COGS_DIR="$2"; shift 2;;
    --artifacts-dir) ARTIFACTS_DIR="$2"; shift 2;;
    --generated-at) GENERATED_AT="$2"; shift 2;;
    --out) OUT="$2"; shift 2;;
    --gen-key) GEN_KEY=1; shift;;
    --attest) ATTEST=1; shift;;
    --audit) AUDIT="$2"; shift 2;;
    --sign-head) SIGN_HEAD=1; shift;;
    *) echo "unknown arg: $1"; exit 2;;
  esac
done
for v in STORE_ID NAME BASE_URL KEY_ID SEED_FILE COGS_DIR ARTIFACTS_DIR GENERATED_AT OUT; do
  [ -n "${!v}" ] || { echo "missing required --$(echo "$v" | tr 'A-Z_' 'a-z-')"; exit 2; }
done

# ---- signing key (read from file; never echoed) ----
if [ ! -f "$SEED_FILE" ]; then
  if [ "$GEN_KEY" = 1 ]; then
    ( umask 077; python3 -c "import secrets;print(secrets.token_hex(32))" > "$SEED_FILE" )
    echo "==> generated a new signing key at $SEED_FILE (chmod 600)"
    echo "    ⚠  BACK THIS UP and NEVER commit it — it is your store's whole trust anchor."
  else
    echo "no seed file at $SEED_FILE (pass --gen-key to create one)"; exit 1
  fi
fi
SEED="$(tr -d '[:space:]' < "$SEED_FILE")"
[ "${#SEED}" = 64 ] || { echo "seed in $SEED_FILE must be 64 hex chars (32 bytes)"; exit 1; }

rm -rf "$OUT"; mkdir -p "$OUT"

echo "==> 1. store.json (self-signed identity for $STORE_ID)"
"$GB" store-info create --store-id "$STORE_ID" --name "$NAME" \
  --catalog-url "$BASE_URL/app-registry.json" \
  --key-id "$KEY_ID" --sign-seed-hex "$SEED" --out "$OUT/store.json"

echo "==> 2. app-registry.json (signed catalog over the staged artifacts)"
"$GB" catalog --cogs-dir "$COGS_DIR" --artifacts-dir "$ARTIFACTS_DIR" \
  --store-id "$STORE_ID" --generated-at "$GENERATED_AT" \
  --out "$OUT/app-registry.json" --sign-seed-hex "$SEED" --key-id "$KEY_ID"

echo "==> 3. stage every catalog artifact at its catalog path (sha256-verified)"
python3 - "$OUT/app-registry.json" "$ARTIFACTS_DIR" "$OUT" <<'PY'
import hashlib, json, pathlib, shutil, sys
catalog, src_dir, out = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), pathlib.Path(sys.argv[3])
c = json.loads(catalog.read_text())
refs = []
for cog in c["cogs"]:
    for v in cog["versions"]:
        a = v["artifacts"]
        refs.append((a["binary"]["path"], a["binary"]["sha256"]))
        for asset in a.get("assets", []):
            refs.append((asset["path"], asset["sha256"]))
missing = []
for path, want in refs:
    src = src_dir / path
    if not src.is_file():
        missing.append(path); continue
    got = hashlib.sha256(src.read_bytes()).hexdigest()
    if got != want:
        print(f"   sha256 MISMATCH for {path}: staged {got} != catalog {want}", file=sys.stderr)
        sys.exit(1)
    dst = out / path
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(src, dst)
    print(f"   staged {path} ({src.stat().st_size} bytes, sha256 ok)")
if missing:
    print("\n   MISSING artifacts — stage these under --artifacts-dir and re-run:", file=sys.stderr)
    for m in missing:
        print(f"     {m}", file=sys.stderr)
    sys.exit(1)
PY

if [ "$ATTEST" = 1 ]; then
  echo "==> 4. sign a provenance + SBOM attestation per binary"
  python3 - "$OUT/app-registry.json" <<'PY' > /tmp/_bins.txt
import json, pathlib, sys
c = json.loads(pathlib.Path(sys.argv[1]).read_text())
for cog in c["cogs"]:
    cid = cog["id"]
    for v in cog["versions"]:
        b = v["artifacts"]["binary"]
        print(f'{cid}\t{v["version"]}\t{b["path"]}')
PY
  mkdir -p "$OUT/attestations"
  while IFS=$'\t' read -r cid ver bpath; do
    "$GB" attest create --artifact "$OUT/$bpath" --cog "$cid" --version "$ver" \
      --artifact-path "$bpath" --builder "$STORE_ID-publish" \
      --source-repo github.com/cognitum-one/cogs --source-commit "$(date +%Y%m%d)" \
      --built-at "$GENERATED_AT" --sign-seed-hex "$SEED" --key-id "$KEY_ID" \
      --out "$OUT/attestations/$cid.json"
  done < /tmp/_bins.txt
  rm -f /tmp/_bins.txt
fi

if [ -n "$AUDIT" ]; then
  echo "==> 5. append a signed publish record to the audit log"
  "$GB" audit append --log "$AUDIT" --ts "$GENERATED_AT" --event verify_catalog \
    --subject "$STORE_ID" --detail key_id="$KEY_ID" --detail result=ok
  if [ "$SIGN_HEAD" = 1 ]; then
    "$GB" audit sign-head --log "$AUDIT" --log-id "$STORE_ID" --ts "$GENERATED_AT" \
      --sign-seed-hex "$SEED" --key-id "$KEY_ID" --out "${AUDIT%.jsonl}.head.json"
  fi
fi

FP=$(python3 -c "import json,base64,hashlib;k=json.load(open('$OUT/store.json'))['keys'][0]['pubkey_b64'];print(hashlib.sha256(base64.b64decode(k)).hexdigest())")
PUB=$(python3 -c "import json;print(json.load(open('$OUT/store.json'))['keys'][0]['pubkey_b64'])")
echo
echo "DONE — upload the contents of $OUT/ to $BASE_URL/ (static HTTPS host)."
echo "  store_id          : $STORE_ID   (cogs install as $STORE_ID/<cog>)"
echo "  catalog_url       : $BASE_URL/app-registry.json"
echo "  pinned public key : $PUB"
echo "  key fingerprint   : $FP"
echo "  (a user adding your store confirms this fingerprint on first use — publish it out-of-band.)"
echo
echo "Seed store descriptor (B1):"
echo "  { \"id\": \"$STORE_ID\", \"name\": \"$NAME\","
echo "    \"catalog_url\": \"$BASE_URL/app-registry.json\","
echo "    \"artifact_base\": \"$BASE_URL\","
echo "    \"trust\": [\"$KEY_ID\"], \"priority\": 100, \"enabled\": true }"
