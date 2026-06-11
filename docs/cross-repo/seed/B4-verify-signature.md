# seed B4 — Verify the catalog signature; fail closed

**Status**: Outline
**Target repo**: `cognitum-one/seed`
**Workstream**: Phase 1 / B4 — **critical path**
**Depends on**: #1 signing format (done), A4 (signed catalog), B3 (loaded catalog)

## Goal

Verify the catalog's Ed25519 signature over its RFC 8785 (JCS) canonicalization against an
embedded trusted key, before the catalog is used. Reject unsigned/tampered/untrusted
(enforcement gated by B6 during the transition).

## Changes

- Implement **RFC 8785 (JCS)** in Rust. It MUST reproduce
  [`docs/protocol/testvectors/catalog.canonical.json`](../../protocol/testvectors/catalog.canonical.json)
  **byte-for-byte** — the test vector is the cross-language conformance check. Use a JCS
  crate; do not hand-roll.
- Embed the official public key(s) as `key_id -> raw 32-byte key`; this is the sole Phase 1
  trust anchor (`StoreDescriptor.trust`).
- Verify: drop the `signature` member → JCS → `ed25519_verify(trust[key_id], bytes, sig)`.
  On failure, refuse to install from the catalog.

## Acceptance

- The Rust JCS output equals `catalog.canonical.json` exactly (run as a unit test).
- The committed test vector verifies; a one-byte tamper is rejected.
- With `require_signed_catalog` on (B6), an unsigned/untrusted catalog is rejected.
