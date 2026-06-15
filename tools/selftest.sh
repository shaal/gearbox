#!/usr/bin/env bash
# Self-test for the catalog generator (gearbox#2) and its Phase-1 follow-ups.
#   1. conformance  — jcs/signing reproduce the FROZEN #1 test vector byte-for-byte
#   2. end-to-end   — generate (full) -> validate -> verify a signed catalog from testdata
#   3. manifests-only — generate without a built binary (A3 gate): binary pending, assets enriched
#   4. asset_entry  — filename + required_when flow into asset entries (B5)
#   5. verify_catalog.py — the A4 verify-before-upload helper (positive + tamper)
#   6. air-gap bundle — build/sign/verify the bundle manifest (T0-A); conformance vs the
#                       frozen vector + a tampered-artifact import must fail
#   7. audit log — append/verify the hash-chained log (T0-B); conformance vs the frozen vector
#                  + an edited record must fail at the right seq
#   8. managed policy — build/sign/verify the policy doc (T0-C); conformance vs the frozen
#                       vector + a forged policy must be rejected (fail-closed)
#   9. attestation — build/sign/verify the provenance+SBOM doc; conformance vs the frozen
#                    vector + a swapped artifact and a tampered field must both fail
set -euo pipefail
cd "$(dirname "$0")"

TVDIR=../docs/protocol/testvectors
SEED=000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f   # published throwaway
KEYID=gearbox-testvector-2026
PUB=$(python3 -c "import sys; sys.path.insert(0,'.'); from cogstore.signing import public_key_b64; print(public_key_b64(bytes.fromhex('$SEED')))")
OUT="$(mktemp /tmp/app-registry.XXXXXX.json)"
MOUT="$(mktemp /tmp/app-registry-mo.XXXXXX.json)"
trap 'rm -f "$OUT" "$MOUT"' EXIT

echo "== 1/9 conformance vs frozen #1 test vector =="
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

echo "== 2/9 end-to-end (full) generate -> validate -> verify =="
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

echo "== 3/9 manifests-only (A3 gate): no built binary =="
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

echo "== 4/9 asset_entry: filename + required_when flow =="
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

echo "== 5/9 verify_catalog.py (A4 verify-before-upload) =="
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

echo "== 6/9 air-gap bundle (T0-A): conformance + export roundtrip + tamper =="
python3 - "$TVDIR/bundle" "$SEED" "$KEYID" <<'PY'
import sys, json, pathlib, shutil, tempfile
sys.path.insert(0, ".")
from cogstore import bundle, jcs, signing
tv, seed_hex, kid = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
seed = bytes.fromhex(seed_hex)

# (a) conformance: build_manifest + sign reproduce the frozen bundle vector byte-for-byte
m = bundle.build_manifest(tv, "2026-06-10T00:00:00Z")
assert jcs.canonical(m) == (tv / "manifest.canonical.json").read_bytes(), \
    "bundle manifest JCS differs from the frozen vector"
assert signing.sign_catalog(m, seed=seed, key_id=kid)["signature"]["sig"] == \
    json.loads((tv / "manifest.signed.json").read_text())["signature"]["sig"], \
    "bundle manifest signature differs from the frozen vector"
print("   OK: build_manifest + signing reproduce the frozen bundle vector byte-for-byte")

# (b) export a fresh bundle from the vector inputs, then import it (full produce -> consume)
work = pathlib.Path(tempfile.mkdtemp())
out = work / "bundle"
bundle.export(tv / "app-registry.json", tv / "store.json", tv / "artifacts",
              out, "2026-06-10T00:00:00Z", seed=seed, key_id=kid)
body = {k: v for k, v in json.loads((out / "manifest.json").read_text()).items() if k != "signature"}
assert jcs.canonical(body) == (tv / "manifest.canonical.json").read_bytes(), \
    "exported manifest diverged from the vector"
rep = bundle.verify_bundle(out)
assert rep["n_artifacts"] == 1 and rep["key_id"] == kid, rep
print(f"   OK: export -> import verifies via file:// ({rep['n_cogs']} cog, {rep['n_artifacts']} artifact)")

# (c) flip a single artifact byte -> import MUST fail (the air-gap acceptance criterion)
art = out / "artifacts/cogs/arm/cog-adversarial-arm"
art.write_bytes(art.read_bytes() + b"X")
try:
    bundle.verify_bundle(out)
    print("   UNEXPECTED: tampered bundle verified"); sys.exit(1)
except Exception as e:
    print(f"   OK: tampered artifact correctly REJECTED ({e})")
shutil.rmtree(work)
PY

echo "== 7/9 audit log (T0-B): conformance + append roundtrip + tamper =="
python3 - "$TVDIR/audit" <<'PY'
import sys, pathlib, tempfile
sys.path.insert(0, ".")
from cogstore import audit
tv = pathlib.Path(sys.argv[1])
frozen = (tv / "log.jsonl").read_bytes()

# (a) conformance: verify the frozen chain + recompute every self hash
recs = audit.read_log(tv / "log.jsonl")
rep = audit.verify(recs)
assert rep["head_self"] == "65a00c0ac86fd4ad8b16919bc9b5022939481ce87bcb783818ae8d78ae8ea2d3", rep
for r in recs:
    assert audit.record_self(r) == r["self"], r["seq"]
print(f"   OK: verify passes; {rep['n']} self hashes recomputed (head {rep['head_self'][:12]}…)")

# (b) append the same fields to a fresh log -> reproduce the frozen bytes exactly
work = pathlib.Path(tempfile.mkdtemp()) / "log.jsonl"
audit.append(work, "2026-06-14T15:00:00Z", "add_store", "acme-internal",
             {"key_id": "acme-signing-2026",
              "fingerprint": "56475aa75463474c0285df5dbf2bcab73da651358839e9b77481b2eab107708c",
              "result": "ok"})
audit.append(work, "2026-06-14T15:01:00Z", "verify_catalog", "acme-internal",
             {"key_id": "acme-signing-2026", "result": "ok"})
audit.append(work, "2026-06-14T15:02:00Z", "install", "acme-internal/doom@0.1.0",
             {"sha256": "238a6e038d11d2b9851396b8ec167ad2f5c8724525100473c2a3f06c9ea43561", "result": "ok"})
audit.append(work, "2026-06-14T15:03:00Z", "policy_deny", "cognitum-official/doom",
             {"reason": "store not allowed — managed policy", "result": "deny"})
assert work.read_bytes() == frozen, "appended log differs from the frozen vector"
print("   OK: append reproduces the frozen log byte-for-byte")

# (c) edit a record -> verify MUST fail at the right seq (the audit acceptance criterion)
recs[1]["detail"]["result"] = "EVIL"
try:
    audit.verify(recs)
    print("   UNEXPECTED: tampered log verified"); sys.exit(1)
except audit.ChainBreak as e:
    assert e.seq == 1, e
    print(f"   OK: edited record correctly REJECTED at {e}")
work.unlink()
PY

echo "== 8/9 managed policy (T0-C): conformance + verify + forged reject =="
python3 - "$TVDIR/policy" "$SEED" "$KEYID" <<'PY'
import sys, json, pathlib
sys.path.insert(0, ".")
from cogstore import policy, jcs, signing
tv, seed_hex, kid = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
seed = bytes.fromhex(seed_hex)
pub = signing.public_key_b64(seed)

# (a) conformance: build_policy + sign reproduce the frozen policy vector byte-for-byte
p = policy.build_policy(allow_stores=["acme-internal"], deny_public=True,
                        forced_pins={"doom": "acme-internal"}, allow_user_add_store=False)
assert jcs.canonical(p) == (tv / "policy.canonical.json").read_bytes(), \
    "policy JCS differs from the frozen vector"
assert policy.sign(p, seed=seed, key_id=kid)["signature"]["sig"] == \
    json.loads((tv / "policy.signed.json").read_text())["signature"]["sig"], \
    "policy signature differs from the frozen vector"
print("   OK: build_policy + signing reproduce the frozen policy vector byte-for-byte")

# (b) verify the frozen signed policy against the pinned key
frozen = json.loads((tv / "policy.signed.json").read_text())
assert policy.verify_signed(frozen, {kid: pub}) == kid
print(f"   OK: signed policy verifies under the org key ({kid})")

# (c) forge a signed field -> verify MUST fail (fail-closed acceptance criterion)
forged = json.loads(json.dumps(frozen)); forged["allow_stores"] = ["evil"]
try:
    policy.verify_signed(forged, {kid: pub})
    print("   UNEXPECTED: forged policy verified"); sys.exit(1)
except Exception as e:
    print(f"   OK: forged policy correctly REJECTED (fail-closed: {type(e).__name__})")
PY

echo "== 9/9 attestation: conformance + verify + swapped-artifact / tampered-field reject =="
python3 - "$TVDIR/attestation" "$SEED" "$KEYID" <<'PY'
import sys, json, pathlib
sys.path.insert(0, ".")
from cogstore import attest, jcs, signing
tv, seed_hex, kid = pathlib.Path(sys.argv[1]), sys.argv[2], sys.argv[3]
seed = bytes.fromhex(seed_hex); pub = signing.public_key_b64(seed)

# (a) conformance: build + sign reproduce the frozen attestation vector byte-for-byte
subject = {"cog": "doom", "version": "0.1.0", "artifact": "cogs/arm/cog-doom-arm",
           "sha256": "238a6e038d11d2b9851396b8ec167ad2f5c8724525100473c2a3f06c9ea43561"}
prov = {"builder": "cogs-ci", "source_repo": "github.com/cognitum-one/cogs",
        "source_commit": "abc1234def5678", "built_at": "2026-06-10T00:00:00Z"}
pkgs = [{"name": "freedoom", "version": "0.13.0", "license": "BSD-3-Clause",
         "sha256": "7323bcc168c5a45ff10749b339960e98314740a734c30d4b9f3337001f9e703d"}]
a = attest.build_attestation(subject, prov, pkgs)
assert jcs.canonical(a) == (tv / "attestation.canonical.json").read_bytes(), \
    "attestation JCS differs from the frozen vector"
assert attest.sign(a, seed=seed, key_id=kid)["signature"]["sig"] == \
    json.loads((tv / "attestation.signed.json").read_text())["signature"]["sig"], \
    "attestation signature differs from the frozen vector"
print("   OK: build + signing reproduce the frozen attestation vector byte-for-byte")

# (b) verify the signed attestation + its artifact-digest binding
frozen = json.loads((tv / "attestation.signed.json").read_text())
assert attest.verify(frozen, {kid: pub}) == kid
art = pathlib.Path("testdata/artifacts/cogs/arm/cog-doom-arm").read_bytes()
attest.check_artifact(frozen, art)
print("   OK: signature + artifact digest binding verify")

# (c) a swapped artifact breaks the binding; a tampered field breaks the signature
try:
    attest.check_artifact(frozen, art + b"X")
    print("   UNEXPECTED: swapped artifact accepted"); sys.exit(1)
except ValueError:
    print("   OK: swapped artifact REJECTED (digest binding)")
bad = json.loads(json.dumps(frozen)); bad["provenance"]["builder"] = "evil"
try:
    attest.verify(bad, {kid: pub})
    print("   UNEXPECTED: tampered provenance verified"); sys.exit(1)
except Exception as e:
    print(f"   OK: tampered provenance REJECTED (signature: {type(e).__name__})")
PY

echo "ALL CHECKS PASSED"
