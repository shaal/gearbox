#!/usr/bin/env bash
# Self-test for the catalog generator (gearbox#2). Two checks:
#   1. Conformance — the shared jcs/signing lib reproduces the FROZEN #1 test vector
#      byte-for-byte (canonical bytes + signature). Ties the generator to the signing
#      contract; a JCS/signing regression fails here.
#   2. End-to-end — generate a signed catalog from testdata/, then validate it against
#      the protocol and verify its signature.
set -euo pipefail
cd "$(dirname "$0")"

TVDIR=../docs/protocol/testvectors
SEED=000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f   # published throwaway
KEYID=gearbox-testvector-2026
OUT="$(mktemp /tmp/app-registry.XXXXXX.json)"
trap 'rm -f "$OUT"' EXIT

echo "== 1/2 conformance vs frozen #1 test vector =="
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

echo "== 2/2 end-to-end generate -> validate -> verify =="
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
catalog.validate(c)                                   # spec-structural validation
trusted = {kid: signing.public_key_b64(bytes.fromhex(seed_hex))}
used = signing.verify_catalog(c, trusted)             # signature check
ids = [x["id"] for x in c["cogs"]]
bins = {x["id"]: x["versions"][0]["artifacts"]["binary"]["path"] for x in c["cogs"]}
print(f"   OK: catalog valid + signature verified (key {used})")
print(f"   cogs={ids}")
for cid, p in bins.items():
    print(f"   {cid}: binary -> {p}")
PY

echo "ALL CHECKS PASSED"
