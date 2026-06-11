"""Reference implementation of the cog-store protocol (publish side).

Modules:
- `jcs`      — RFC 8785 canonicalization (the subset cog-store catalogs use)
- `signing`  — Ed25519 sign/verify over JCS bytes (protocol §7)
- `catalog`  — build + validate an app-registry.json from a cog.toml tree (protocol §3)

This is the *generator* side (gearbox#2), invoked from cognitum-one/cogs CI. The
device-side verifier lives in cognitum-one/seed; the native `gearbox` CLI is gearbox#3.
All three implement the same protocol and are pinned by the test vector in
docs/protocol/testvectors/.
"""
