# seed B4 — Verify the catalog signature; fail closed

**Status**: Drafted (ready to apply as a `cognitum-one/seed` PR)
**Target repo**: `cognitum-one/seed`
**Workstream**: Phase 1 / B4 — **critical path**
**Depends on**: #1 signing format (done), A4 (signed catalog), B3 (loaded catalog)
**Pins**: [protocol §7](../../protocol/cog-store-protocol.md#7-signing) + [`testvectors/`](../../protocol/testvectors/)

## Goal

Before a fetched catalog is used, verify its Ed25519 signature over the **RFC 8785 (JCS)**
canonicalization against an embedded trusted key. Reject unsigned / tampered / untrusted
catalogs (enforcement gated by B6 during the transition). The Rust verifier **MUST
reproduce the frozen test vector byte-for-byte** — that is the cross-language conformance
gate between this verifier, the signer (A4), and the generator (#2).

## Why this is the load-bearing piece

sha256 protects the *download*, not the *decision* to download ([ADR-0001](../../adr/ADR-0001-pluggable-cog-stores.md)).
Authenticity comes **only** from this signature check; B5's hash checks are meaningful
solely because the hashes came from a catalog this step verified. Get the canonicalization
wrong by one byte and either everything fails to verify, or — worse — the security property
is silently void.

## Algorithm (protocol §7.1)

1. Parse the fetched catalog bytes into a JSON tree (`serde_json::Value`). **Keep this raw
   tree.**
2. Read the top-level `signature` object: `{ key_id, alg, sig }`.
3. Resolve `key_id` against the trust store (`StoreDescriptor.trust`). Reject if unknown.
4. Reject if `alg != "ed25519"`.
5. Build the **signing input**: clone the tree, remove the top-level `signature` member,
   JCS-canonicalize the remainder → bytes.
6. base64-decode `sig` (64 bytes); `verify_strict(pubkey, signing_input, sig)`.
7. On success, hand the (now verified) catalog to typed extraction (B3) → install (B5).

## Critical correctness rules

- **Canonicalize the raw JSON tree, not a round-tripped typed struct.** If you deserialize
  into Rust structs that drop unknown fields and then re-serialize, you canonicalize
  *different* bytes than the signer — verification fails, or (worse) appears to pass over a
  subset. Always JCS the full parsed document minus `signature`. Do typed extraction
  (cogs/versions/artifacts) *after* verification.
- **Use a real RFC 8785 implementation** (e.g. `serde_jcs` or `json-canon`) — do not
  hand-roll. Catalogs are restricted to ASCII strings + integers (protocol §7.1), but a
  conforming crate is the safe choice. The test vector is the acceptance gate.
- **Use `verify_strict`** (ed25519-dalek): it rejects non-canonical / malleable signatures
  and small-order points that plain `verify` accepts.
- **Reject duplicate top-level keys** in the catalog JSON (defense in depth).
- Verification operates on public data — no constant-time needed (unlike the per-cog
  bearer-token compare, which stays `subtle`).

## Trust anchor (Phase 1)

- A key registry: `key_id -> VerifyingKey` (raw 32-byte Ed25519). The official release
  public key is embedded in the Seed binary (compile-time constant or shipped config),
  keyed by its date-scoped `key_id` (e.g. `cognitum-release-2026`). `StoreDescriptor.trust`
  lists which `key_id`s are acceptable for that store; Phase 1 = exactly one.
- Rotation-ready: the registry is a map, so an overlap window can trust two `key_id`s at
  once (revocation is Phase 2/3, protocol §8).
- **Per-version signatures** (the protocol §3 alternative) are out of scope for Phase 1 —
  whole-catalog signature only.

## Dependencies (crates)

- `ed25519-dalek` (2.x) — already a workspace dep (`VerifyingKey`, `Signature`,
  `verify_strict`).
- `serde_json` — catalog parsing. **Do not enable `arbitrary_precision`** (it stores
  numbers as strings and can change JCS number handling).
- an RFC 8785 crate — `serde_jcs` (serde_json-based) or `json-canon`.
- `base64`.

## Reference sketch (Rust)

```rust
use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::Value;
use std::collections::HashMap;

pub struct TrustStore(pub HashMap<String, VerifyingKey>); // key_id -> public key

#[derive(Debug, thiserror::Error)]
pub enum VerifyError {
    #[error("catalog has no signature")]      MissingSignature,
    #[error("unsupported alg {0:?}")]          UnsupportedAlg(String),
    #[error("untrusted key_id {0:?}")]         UntrustedKeyId(String),
    #[error("malformed signature envelope")]   Malformed,
    #[error("bad base64: {0}")]                Base64(#[from] base64::DecodeError),
    #[error("signature did not verify")]       Invalid,
    #[error("canonicalization: {0}")]          Canon(String),
}

/// Verify a parsed catalog; on success returns the key_id that signed it.
pub fn verify_catalog(catalog: &Value, trust: &TrustStore) -> Result<String, VerifyError> {
    let obj = catalog.as_object().ok_or(VerifyError::Malformed)?;
    let sig = obj.get("signature").ok_or(VerifyError::MissingSignature)?;
    let key_id  = sig.get("key_id").and_then(Value::as_str).ok_or(VerifyError::Malformed)?;
    let alg     = sig.get("alg").and_then(Value::as_str).ok_or(VerifyError::Malformed)?;
    let sig_b64 = sig.get("sig").and_then(Value::as_str).ok_or(VerifyError::Malformed)?;

    if alg != "ed25519" { return Err(VerifyError::UnsupportedAlg(alg.into())); }
    let vk = trust.0.get(key_id).ok_or_else(|| VerifyError::UntrustedKeyId(key_id.into()))?;

    // signing input = catalog MINUS its `signature` member, JCS-canonicalized
    let mut body = catalog.clone();
    body.as_object_mut().unwrap().remove("signature");
    let signing_input = serde_jcs::to_vec(&body).map_err(|e| VerifyError::Canon(e.to_string()))?;

    let raw = STANDARD.decode(sig_b64)?;
    let sig = Signature::from_slice(&raw).map_err(|_| VerifyError::Malformed)?;
    vk.verify_strict(&signing_input, &sig).map_err(|_| VerifyError::Invalid)?;
    Ok(key_id.to_string())
}
```

*(Exact JCS fn name per the chosen crate. Call order: B3 fetch/parse → **B4 verify** →
B5 install, gated by B6.)*

## Conformance test (the heart of B4)

Vendor the two frozen files from Gearbox into the seed test fixtures (tiny, stable), with a
pointer to the source of truth (`gearbox:docs/protocol/testvectors/`):
`catalog.signed.json`, `catalog.canonical.json`. Then:

1. **JCS byte-equality** — `serde_jcs::to_vec(signed_minus_signature)` equals the bytes of
   `catalog.canonical.json`. This is the cross-language gate; if it fails, the Rust JCS
   differs from the signer's and nothing else matters.
2. **verify ok** — `verify_catalog(signed, trust{test_key})` returns `gearbox-testvector-2026`.
3. **tamper rejected** — flip one byte in a value → `Invalid`.
4. **untrusted key** — empty trust store → `UntrustedKeyId`.
5. **bad alg** — `alg = "rsa"` → `UnsupportedAlg`.

Test key (published throwaway; identical to the Gearbox vector):

| | |
|---|---|
| `key_id` | `gearbox-testvector-2026` |
| public key (base64, raw 32 B) | `A6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=` |
| expected canonical SHA-256 | `f2b6f83a31f5c03860169ae76445a55619c8f2ac4bc3ddb22d131cf6d4fe4687` |

Optional CI: assert the vendored fixtures byte-match the Gearbox source so they can't drift.

## Integration

- Called by B3 after the catalog is fetched/parsed, **before** any typed use / install (B5).
- Gated by **B6**: when `require_signed_catalog = false` (one transition release), a
  missing/invalid signature logs a loud warning but does not block; when `true`, it rejects
  (**fail closed**).
- The verified catalog's artifact hashes are what B5 enforces — never transport-supplied
  hashes.

## Acceptance criteria

- The Rust JCS output equals `catalog.canonical.json` **byte-for-byte** (test #1).
- The committed test vector verifies; a one-byte tamper, an untrusted `key_id`, and a
  non-`ed25519` `alg` are each rejected (tests #2–5).
- Verification runs on the **raw parsed JSON** — unknown future fields are included, not
  dropped.
- With enforcement on (B6), unsigned/untrusted catalogs are refused; with it off, they warn.
- No catalog is used for install before `verify_catalog` succeeds (when enforced).
