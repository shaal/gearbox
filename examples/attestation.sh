#!/usr/bin/env bash
# End-to-end demo of provenance + SBOM attestation: a publisher signs an attestation.json that
# binds an artifact to WHERE it came from (SLSA-style provenance) and WHAT is inside it (an SBOM),
# and a consumer verifies BOTH the signature and the artifact-digest binding offline. Then a
# swapped artifact and a tampered provenance field are each refused.
#
#   attest create -> verify (sig + binding) -> swapped artifact refused -> tampered field refused
#
# Requires: the gearbox binary, python3.
#   cargo build --manifest-path crates/gearbox/Cargo.toml && examples/attestation.sh
set -euo pipefail
cd "$(dirname "$0")/.."
GB="${GEARBOX:-crates/gearbox/target/debug/gearbox}"
[ -x "$GB" ] || { echo "build first: cargo build --manifest-path crates/gearbox/Cargo.toml"; exit 1; }

SEED=000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f   # throwaway publisher key
KEYID=cogs-release-2026
PUB=$(python3 -c "import sys;sys.path.insert(0,'tools');from cogstore.signing import public_key_b64;print(public_key_b64(bytes.fromhex('$SEED')))")
ART=tools/testdata/artifacts/cogs/arm/cog-doom-arm
OTHER=tools/testdata/artifacts/cogs/arm/cog-adversarial-arm
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT

echo "==> 1. [publisher] sign an attestation binding the doom binary to its source + SBOM"
"$GB" attest create --artifact "$ART" --cog doom --version 0.1.0 \
  --artifact-path cogs/arm/cog-doom-arm \
  --builder cogs-ci --source-repo github.com/cognitum-one/cogs --source-commit abc1234def5678 \
  --built-at 2026-06-14T00:00:00Z \
  --package freedoom=0.13.0=BSD-3-Clause=7323bcc168c5a45ff10749b339960e98314740a734c30d4b9f3337001f9e703d \
  --sign-seed-hex "$SEED" --key-id "$KEYID" --out "$WORK/attestation.json"
sed 's/^/      /' "$WORK/attestation.json"

echo "==> 2. [consumer] verify the signature AND that the artifact is the one attested"
"$GB" attest verify "$WORK/attestation.json" --key-id "$KEYID" --pubkey-b64 "$PUB" --artifact "$ART"

echo "==> 3. point the same attestation at a DIFFERENT artifact -> digest binding must fail"
if "$GB" attest verify "$WORK/attestation.json" --key-id "$KEYID" --pubkey-b64 "$PUB" --artifact "$OTHER" 2>/dev/null; then
  echo "   UNEXPECTED: wrong artifact accepted"; exit 1
else
  echo "   OK: the wrong artifact was REJECTED (sha256 binding)"
fi

echo "==> 4. tamper a provenance field -> the signature must fail"
python3 -c "import json,pathlib;p=pathlib.Path('$WORK/attestation.json');d=json.load(open(p));d['provenance']['source_commit']='deadbeef';p.write_text(json.dumps(d))"
if "$GB" attest verify "$WORK/attestation.json" --key-id "$KEYID" --pubkey-b64 "$PUB" 2>/dev/null; then
  echo "   UNEXPECTED: tampered attestation verified"; exit 1
else
  echo "   OK: the tampered provenance was REJECTED (signature)"
fi

echo "ALL GOOD: signed provenance+SBOM -> verify sig + digest binding -> swap and forgery both refused"
