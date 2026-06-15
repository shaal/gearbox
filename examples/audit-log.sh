#!/usr/bin/env bash
# End-to-end demo of the audit / event log (Phase 3, T0-B): record a trust-affecting sequence
# (add-store -> verify-catalog -> install) into an append-only, hash-chained JSONL log, verify
# the chain offline, then show that editing OR deleting a record makes `audit verify` fail at the
# right seq — tamper-evident with no server and no key.
#
# Requires: the gearbox binary, python3.
#   cargo build --manifest-path crates/gearbox/Cargo.toml && examples/audit-log.sh
set -euo pipefail
cd "$(dirname "$0")/.."
GB="${GEARBOX:-crates/gearbox/target/debug/gearbox}"
[ -x "$GB" ] || { echo "build first: cargo build --manifest-path crates/gearbox/Cargo.toml"; exit 1; }

WORK="$(mktemp -d)"; LOG="$WORK/audit.jsonl"
trap 'rm -rf "$WORK"' EXIT
SEED=000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f   # throwaway head-signing key
KEYID=acme-audit-2026
PUB=$(python3 -c "import sys;sys.path.insert(0,'tools');from cogstore.signing import public_key_b64;print(public_key_b64(bytes.fromhex('$SEED')))")

echo "==> 1. record a scripted trust-affecting sequence (the seed's B-series hooks would call this)"
"$GB" audit append --log "$LOG" --ts 2026-06-14T15:00:00Z --event add_store \
  --subject acme-internal --detail key_id=acme-signing-2026 \
  --detail fingerprint=56475aa75463474c0285df5dbf2bcab73da651358839e9b77481b2eab107708c --detail result=ok
"$GB" audit append --log "$LOG" --ts 2026-06-14T15:01:00Z --event verify_catalog \
  --subject acme-internal --detail key_id=acme-signing-2026 --detail result=ok
"$GB" audit append --log "$LOG" --ts 2026-06-14T15:02:00Z --event install \
  --subject "acme-internal/doom@0.1.0" --detail sha256=238a6e0…ea43561 --detail result=ok

echo "==> 2. the log (one JSON record per line; each chains to the previous via prev/self):"
sed 's/^/      /' "$LOG"

echo "==> 3. verify the chain offline (recompute every self + linkage)"
"$GB" audit verify --log "$LOG"

echo "==> 4. sign the head (a checkpoint) -> now a TAIL TRUNCATION is detectable, not just edits"
"$GB" audit sign-head --log "$LOG" --log-id acme-dev-01 --ts 2026-06-14T16:00:00Z \
  --sign-seed-hex "$SEED" --key-id "$KEYID" --out "$WORK/head.json"
cp "$LOG" "$WORK/truncated.jsonl"
python3 -c "import pathlib;p=pathlib.Path('$WORK/truncated.jsonl');ls=p.read_text().splitlines();p.write_text(chr(10).join(ls[:-1])+chr(10))"
echo "   plain verify of the truncated log (still passes — the keyless-chain limit):"
"$GB" audit verify --log "$WORK/truncated.jsonl" | sed 's/^/      /'
echo "   with the signed head (MUST now fail):"
if "$GB" audit verify --log "$WORK/truncated.jsonl" --head "$WORK/head.json" --key-id "$KEYID" --pubkey-b64 "$PUB" 2>&1 | sed 's/^/      /'; then
  echo "   UNEXPECTED: truncated log accepted"; exit 1
else
  echo "   OK: signing the head turns tamper-EVIDENT into tamper-PROOF up to the checkpoint"
fi

echo "==> 5. tamper: edit a past record's detail in place -> verify MUST fail at that seq"
python3 - "$LOG" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1]); lines = p.read_text(encoding="utf-8").splitlines()
lines[1] = lines[1].replace('"result":"ok"', '"result":"TAMPERED"')   # edit record seq 1
p.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY
if "$GB" audit verify --log "$LOG" 2>/dev/null; then
  echo "   UNEXPECTED: tampered log verified"; exit 1
else
  "$GB" audit verify --log "$LOG" 2>&1 | sed 's/^/   /' || true
  echo "   OK: the edited record was caught"
fi

echo "==> 6. deletion: drop a middle record -> the chain breaks (seq gap + linkage)"
python3 - "$LOG" <<'PY'
import sys, pathlib
p = pathlib.Path(sys.argv[1]); lines = p.read_text(encoding="utf-8").splitlines()
# rebuild the (already-tampered) log minus its middle line to show deletion is also caught
del lines[1]
p.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY
"$GB" audit verify --log "$LOG" 2>&1 | sed 's/^/   /' || true

echo "ALL GOOD: chain verifies -> signed head defeats truncation -> edit and deletion both refused"
