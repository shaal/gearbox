#!/usr/bin/env bash
# End-to-end demo of the air-gap bundle (Phase 3, T0-A): export a self-contained, signed
# bundle, carry it across an "air gap" as a .tar (no network), and import it on the far side
# with the SAME verification an online store gets — verify_catalog + per-artifact sha256, only
# the transport (file://) differs. Finally, flip one artifact byte and show import REFUSES it.
#
#   build store+catalog -> gearbox export -> tar -> [air gap] -> untar -> gearbox import
#
# Requires: the gearbox binary, python3, sha256sum, tar.
#   cargo build --manifest-path crates/gearbox/Cargo.toml && examples/bundle-airgap.sh
set -euo pipefail
cd "$(dirname "$0")/.."
GB="${GEARBOX:-crates/gearbox/target/debug/gearbox}"
[ -x "$GB" ] || { echo "build first: cargo build --manifest-path crates/gearbox/Cargo.toml"; exit 1; }

SEED=000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f   # throwaway test seed
KEYID=acme-signing-2026
TS=2026-06-14T00:00:00Z
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

STAGE="$WORK/stage"; mkdir -p "$STAGE/cogs/arm/data" "$WORK/cogs/sensor"

echo "==> 1. author a cog WITH an asset (binary + a data file), staged for hashing"
printf 'stub sensor binary\n' > "$STAGE/cogs/arm/cog-sensor-arm"
printf 'calibration table v1\n' > "$STAGE/cogs/arm/data/calib.bin"
ASHA=$(sha256sum "$STAGE/cogs/arm/data/calib.bin" | cut -d' ' -f1)
ASIZE=$(wc -c < "$STAGE/cogs/arm/data/calib.bin" | tr -d ' ')
cat > "$WORK/cogs/sensor/cog.toml" <<TOML
[cog]
id = "sensor"
name = "Sensor Cog"
version = "1.0.0"
category = "demo"
description = "Air-gap bundle demo cog with a downloadable asset."
binary = "cog-sensor-arm"
hardware_requirement = "pi-zero-2w"

[[assets]]
id = "calib"
filename = "calib.bin"
size_bytes = ${ASIZE}
sha256 = "${ASHA}"
gcs_path = "data/calib.bin"
TOML

echo "==> 2. build store.json (self-signed) and the signed catalog over the staged artifacts"
"$GB" store-info create --store-id acme-internal --name "ACME Internal Store" \
  --description "Private store, distributed air-gapped." \
  --catalog-url "file://./app-registry.json" \
  --key-id "$KEYID" --sign-seed-hex "$SEED" --out "$WORK/store.json"
"$GB" catalog --cogs-dir "$WORK/cogs" --artifacts-dir "$STAGE" \
  --store-id acme-internal --generated-at "$TS" \
  --out "$WORK/app-registry.json" --sign-seed-hex "$SEED" --key-id "$KEYID"

echo "==> 3. export the air-gap bundle (copies + re-hashes every artifact, signs the manifest)"
"$GB" export --catalog "$WORK/app-registry.json" --store-info "$WORK/store.json" \
  --artifacts-dir "$STAGE" --out "$WORK/acme-bundle" \
  --generated-at "$TS" --sign-seed-hex "$SEED" --key-id "$KEYID"
echo "    bundle contents:"; (cd "$WORK/acme-bundle" && find . -type f | sort | sed 's/^/      /')

echo "==> 4. cross the air gap: tar it up, 'transfer', and extract on a disconnected device"
tar -cf "$WORK/acme-bundle.tar" -C "$WORK/acme-bundle" .
DEVICE="$WORK/device/acme-bundle"; mkdir -p "$DEVICE"
tar -xf "$WORK/acme-bundle.tar" -C "$DEVICE"

echo "==> 5. [disconnected device] import: TOFU the store key, verify catalog + every artifact"
"$GB" import "$DEVICE"

echo "==> 6. returning device with a PINNED key: assert the fingerprint instead of TOFU"
FP=$(python3 -c "import sys,json,base64,hashlib; \
k=json.load(open('$DEVICE/store.json'))['keys'][0]['pubkey_b64']; \
print(hashlib.sha256(base64.b64decode(k)).hexdigest())")
"$GB" import "$DEVICE" --expect-fingerprint "$FP"

echo "==> 7. tamper one artifact byte -> import MUST fail (air-gap is no weaker than online)"
printf 'X' >> "$DEVICE/artifacts/cogs/arm/data/calib.bin"
if "$GB" import "$DEVICE" 2>/dev/null; then
  echo "    UNEXPECTED: tampered bundle verified"; exit 1
else
  echo "    OK: a single flipped byte was REJECTED"
fi

echo "ALL GOOD: export -> tar -> air gap -> import (TOFU + pinned) -> tamper refused"
