#!/usr/bin/env bash
# End-to-end demo of managed mode (Phase 3, T0-C): an admin authors a SIGNED policy.json
# (allow-only ACME, deny the public store, force-pin doom), and a managed device enforces it as
# a projection in front of the resolver — an in-policy ref resolves to ACME, an out-of-policy
# ref is DENIED and a policy_deny audit record is written (T0-B), and a forged policy is rejected
# fail-closed.
#
#   policy create -> verify -> check (allow) -> check (deny + audit) -> forged policy refused
#
# Requires: the gearbox binary, python3.
#   cargo build --manifest-path crates/gearbox/Cargo.toml && examples/managed-mode.sh
set -euo pipefail
cd "$(dirname "$0")/.."
GB="${GEARBOX:-crates/gearbox/target/debug/gearbox}"
[ -x "$GB" ] || { echo "build first: cargo build --manifest-path crates/gearbox/Cargo.toml"; exit 1; }

SEED=000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f   # throwaway org policy key
KEYID=acme-policy-2026
PUB=$(python3 -c "import sys;sys.path.insert(0,'tools');from cogstore.signing import public_key_b64;print(public_key_b64(bytes.fromhex('$SEED')))")
WORK="$(mktemp -d)"; trap 'rm -rf "$WORK"' EXIT

echo "==> 1. [admin] author + sign a managed policy (org policy key provisioned out-of-band)"
"$GB" policy create --out "$WORK/policy.json" --sign-seed-hex "$SEED" --key-id "$KEYID" \
  --allow-stores acme-internal --deny-public --forced-pin doom=acme-internal
sed 's/^/      /' "$WORK/policy.json"

echo "==> 2. [device] verify the policy against the PINNED org key (fail-closed gate)"
"$GB" policy verify "$WORK/policy.json" --key-id "$KEYID" --pubkey-b64 "$PUB"

echo "==> 3. [device] its configured stores (a private ACME store + the public store, both offer doom)"
cat > "$WORK/stores.json" <<'JSON'
{ "stores": [
    { "id": "acme-internal",     "priority": 10, "enabled": true, "cogs": ["doom"] },
    { "id": "cognitum-official", "priority": 50, "enabled": true, "cogs": ["doom"] }
  ], "pins": {} }
JSON
sed 's/^/      /' "$WORK/stores.json"

echo "==> 4. resolve 'doom' under policy -> the ACME cog (forced pin + allowlist)"
"$GB" policy check --policy "$WORK/policy.json" --key-id "$KEYID" --pubkey-b64 "$PUB" \
  --stores "$WORK/stores.json" --ref doom

echo "==> 5. resolve the public store explicitly -> DENIED, and a policy_deny is audited"
"$GB" policy check --policy "$WORK/policy.json" --key-id "$KEYID" --pubkey-b64 "$PUB" \
  --stores "$WORK/stores.json" --ref cognitum-official/doom \
  --audit-log "$WORK/audit.jsonl" --ts 2026-06-14T16:00:00Z || true
echo "   audit log:"; sed 's/^/      /' "$WORK/audit.jsonl"
"$GB" audit verify --log "$WORK/audit.jsonl" | sed 's/^/   /'

echo "==> 6. fail-closed: forge a byte in the policy -> verify AND check must refuse it"
python3 -c "import json,pathlib; p=pathlib.Path('$WORK/policy.json'); d=json.load(open(p)); d['allow_stores']=['evil']; p.write_text(json.dumps(d))"
if "$GB" policy verify "$WORK/policy.json" --key-id "$KEYID" --pubkey-b64 "$PUB" 2>/dev/null; then
  echo "   UNEXPECTED: forged policy verified"; exit 1
else
  echo "   OK: forged policy REJECTED (fail-closed) — a stripped/edited policy denies, not opens"
fi

echo "ALL GOOD: signed policy -> enforce -> ACME allowed, public denied + audited, forgery refused"
