#!/usr/bin/env bash
# Self-test for the catalog generator (gearbox#2) and its Phase-1 follow-ups.
#   1. conformance  — jcs/signing reproduce the FROZEN #1 test vector byte-for-byte
#   2. end-to-end   — generate (full) -> validate -> verify a signed catalog from testdata
#   3. manifests-only — generate without a built binary (A3 gate): binary pending, assets enriched
#   4. asset_entry  — filename + required_when flow into asset entries (B5)
#   5. verify_catalog.py — the A4 verify-before-upload helper (positive + tamper)
set -euo pipefail
cd "$(dirname "$0")"

TVDIR=../docs/protocol/testvectors
SEED=000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f   # published throwaway
KEYID=gearbox-testvector-2026
PUB=$(python3 -c "import sys; sys.path.insert(0,'.'); from cogstore.signing import public_key_b64; print(public_key_b64(bytes.fromhex('$SEED')))")
OUT="$(mktemp /tmp/app-registry.XXXXXX.json)"
MOUT="$(mktemp /tmp/app-registry-mo.XXXXXX.json)"
trap 'rm -f "$OUT" "$MOUT"' EXIT

echo "== 1/5 conformance vs frozen #1 test vector =="
python3 - "$TVDIR" "$SEED" "$KEYID" <<'PY'
import sys, json, pathlib
sys.path.insert(0, ".")
from cogstore import jcs, signing
tv, seed_hex, kid = sys.argv[1], sys.argv[2], sys.argv[3]
signed = json.loads(pathlib.Path(tv, "catalog.signed.json").read_text())
body = {k: v for k, v in signed.items() if k != "signature"}
assert jcs.canonical(body) == pathlib.Path(tv, "catalog.canonical.json").read_bytes(), \
    "JCS output differs from the frozen test vector"
re = signing.sign_catalog(body, seed=bytes.fromhex(seed_hex), key_id=kid)
assert re["signature"]["sig"] == signed["signature"]["sig"], \
    "signature differs from the frozen test vector"
print("   OK: jcs + signing reproduce the frozen vector byte-for-byte")
PY

echo "== 2/5 end-to-end (full) generate -> validate -> verify =="
python3 catalog_gen.py \
    --cogs-dir testdata/cogs --artifacts-dir testdata/artifacts \
    --store-id cognitum-official --generated-at 2026-06-10T00:00:00Z \
    --out "$OUT" --sign-seed-hex "$SEED" --key-id "$KEYID"
python3 - "$OUT" "$SEED" "$KEYID" <<'PY'
import sys, json, pathlib
sys.path.insert(0, ".")
from cogstore import catalog, signing
out, seed_hex, kid = sys.argv[1], sys.argv[2], sys.argv[3]
c = json.loads(pathlib.Path(out).read_text())
catalog.validate(c)
used = signing.verify_catalog(c, {kid: signing.public_key_b64(bytes.fromhex(seed_hex))})
doom = next(x for x in c["cogs"] if x["id"] == "doom")["versions"][0]["artifacts"]
assert doom["binary"]["sha256"], "binary should be hashed in full mode"
assert doom["assets"][0]["filename"] == "freedoom1.wad", doom["assets"][0]
print(f"   OK: valid + verified ({used}); asset carries filename")
PY

echo "== 3/5 manifests-only (A3 gate): no built binary =="
python3 catalog_gen.py \
    --cogs-dir testdata/cogs --manifests-only \
    --store-id cognitum-official --generated-at 2026-06-10T00:00:00Z --out "$MOUT"
python3 - "$MOUT" <<'PY'
import sys, json, pathlib
sys.path.insert(0, ".")
from cogstore import catalog
c = json.loads(pathlib.Path(sys.argv[1]).read_text())
catalog.validate(c)                                       # must pass with pending binary
b = next(x for x in c["cogs"] if x["id"] == "doom")["versions"][0]["artifacts"]["binary"]
assert b.get("pending") is True and "sha256" not in b, b
print("   OK: validates with binary pending (no artifact needed)")
PY

echo "== 4/5 asset_entry: filename + required_when flow =="
python3 - <<'PY'
import sys
sys.path.insert(0, ".")
from cogstore.catalog import asset_entry
e = asset_entry({"id": "m", "filename": "m/model.gguf", "sha256": "a"*64,
                 "size_bytes": 10, "gcs_path": "models/m.gguf",
                 "required_when": "config.model_id == 'm'"}, "arm")
assert e == {"id": "m", "path": "cogs/arm/models/m.gguf", "filename": "m/model.gguf",
             "sha256": "a"*64, "size": 10, "required_when": "config.model_id == 'm'"}, e
e2 = asset_entry({"id": "n", "filename": "f", "sha256": "b"*64, "size_bytes": 5,
                  "path": "x/y"}, "arm")                  # `path` preferred over gcs_path
assert e2["path"] == "cogs/arm/x/y" and "required_when" not in e2, e2
print("   OK: filename + required_when carried; path preferred; required_when omitted when absent")
PY

echo "== 5/5 verify_catalog.py (A4 verify-before-upload) =="
python3 verify_catalog.py "$TVDIR/catalog.signed.json" --key-id "$KEYID" --pubkey-b64 "$PUB"
python3 - "$TVDIR" > "$MOUT.tamper" <<'PY'
import sys, json, pathlib
d = json.loads(pathlib.Path(sys.argv[1], "catalog.signed.json").read_text())
d["store_id"] = "evil"                                    # tamper a signed field
print(json.dumps(d))
PY
if python3 verify_catalog.py "$MOUT.tamper" --key-id "$KEYID" --pubkey-b64 "$PUB" 2>/dev/null; then
    echo "   UNEXPECTED: tampered catalog verified"; exit 1
else
    echo "   OK: tampered catalog correctly REJECTED"
fi
rm -f "$MOUT.tamper"

echo "ALL CHECKS PASSED"
