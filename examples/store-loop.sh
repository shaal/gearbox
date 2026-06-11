#!/usr/bin/env bash
# End-to-end demo of the cog-store add-store loop against a LIVE HTTP store:
#   build a store -> serve it -> [Seed] fetch+verify store.json (TOFU fingerprint)
#   -> fetch+verify the signed catalog -> fetch an artifact and check its sha256.
# Also demonstrates bearer auth (private store): 401 without token, 200 with.
#
# Requires: the gearbox binary, curl, python3, sha256sum.
#   cargo build --manifest-path crates/gearbox/Cargo.toml && examples/store-loop.sh
set -euo pipefail
cd "$(dirname "$0")/.."
GB="${GEARBOX:-crates/gearbox/target/debug/gearbox}"
[ -x "$GB" ] || { echo "build first: cargo build --manifest-path crates/gearbox/Cargo.toml"; exit 1; }

SEED=000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f   # throwaway test seed
KEYID=demo-store-2026
PORT="${PORT:-8099}"; APORT=$((PORT + 1))
WORK="$(mktemp -d)"; SRV=""; ASRV=""
cleanup() { [ -n "$SRV" ] && kill "$SRV" 2>/dev/null || true
            [ -n "$ASRV" ] && kill "$ASRV" 2>/dev/null || true
            rm -rf "$WORK"; }
trap cleanup EXIT

STORE="$WORK/store"; mkdir -p "$STORE/cogs/arm" "$WORK/cogs/demo"

echo "==> 1. author a demo cog (no assets; binary is hashed by the generator)"
printf 'stub demo binary\n' > "$STORE/cogs/arm/cog-demo-arm"
cat > "$WORK/cogs/demo/cog.toml" <<'TOML'
[cog]
id = "demo"
name = "Demo Cog — café"
version = "0.1.0"
category = "demo"
description = "Reference store demo cog."
binary = "cog-demo-arm"
hardware_requirement = "pi-zero-2w"
TOML

echo "==> 2. build store.json (self-signed) and the signed catalog"
"$GB" store-info create --store-id demo-store --name "Demo Store" \
  --description "A reference store served over HTTP." \
  --catalog-url "http://127.0.0.1:$PORT/app-registry.json" \
  --key-id "$KEYID" --sign-seed-hex "$SEED" --out "$STORE/store.json"
"$GB" catalog --cogs-dir "$WORK/cogs" --artifacts-dir "$STORE" \
  --store-id demo-store --generated-at 2026-06-11T00:00:00Z \
  --out "$STORE/app-registry.json" --sign-seed-hex "$SEED" --key-id "$KEYID"

echo "==> 3. serve the store"
"$GB" serve --dir "$STORE" --port "$PORT" & SRV=$!

echo "==> 4. [Seed] add store: fetch store.json, show fingerprint, verify self-signature (TOFU)"
curl -fsS --retry 20 --retry-connrefused --retry-delay 1 \
  "http://127.0.0.1:$PORT/store.json" -o "$WORK/store.json"
"$GB" store-info verify "$WORK/store.json"
PUB=$(python3 -c "import json; print(json.load(open('$WORK/store.json'))['keys'][0]['pubkey_b64'])")

echo "==> 5. [Seed] fetch the catalog and verify it against the pinned key"
curl -fsS "http://127.0.0.1:$PORT/app-registry.json" -o "$WORK/catalog.json"
"$GB" verify "$WORK/catalog.json" --key-id "$KEYID" --pubkey-b64 "$PUB"

echo "==> 6. [Seed] fetch the binary artifact and check its sha256 against the verified catalog"
read -r BPATH BSHA < <(python3 -c "import json; a=json.load(open('$WORK/catalog.json'))['cogs'][0]['versions'][0]['artifacts']['binary']; print(a['path'], a['sha256'])")
curl -fsS "http://127.0.0.1:$PORT/$BPATH" -o "$WORK/artifact.bin"
GOT=$(sha256sum "$WORK/artifact.bin" | cut -d' ' -f1)
[ "$GOT" = "$BSHA" ] && echo "   artifact sha256 OK ($BPATH)" \
  || { echo "   sha256 MISMATCH: $GOT != $BSHA"; exit 1; }

echo "==> 7. bearer auth (private store): 401 without token, 200 with"
"$GB" serve --dir "$STORE" --port "$APORT" --auth-token s3cr3t & ASRV=$!
C1=$(curl -s -o /dev/null -w '%{http_code}' --retry 20 --retry-connrefused --retry-delay 1 \
  "http://127.0.0.1:$APORT/store.json")
C2=$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer s3cr3t" \
  "http://127.0.0.1:$APORT/store.json")
echo "   no token -> $C1 ; with token -> $C2"
[ "$C1" = "401" ] && [ "$C2" = "200" ] || { echo "   auth demo FAILED"; exit 1; }

echo "ALL GOOD: serve -> TOFU -> verify catalog -> fetch+hash artifact -> bearer auth"
