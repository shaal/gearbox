# gearbox (Rust) — native cog-store reference

Native Rust implementation of the cog-store protocol (gearbox#3). **Phase-1 scope:
catalog signature verification** — JCS (RFC 8785) + Ed25519 — the same algorithm the
device-side verifier (`cognitum-one/seed` B4) needs, proven against the frozen test vector
in [`../../docs/protocol/testvectors/`](../../docs/protocol/testvectors/).

## Build / test / run

```bash
cargo test                                  # 5 conformance tests vs the test vector
cargo run -- verify <catalog.json> --key-id <ID> --pubkey-b64 <B64>
```

## Layout

```
src/jcs.rs       RFC 8785 canonicalization (ASCII+integer subset; matches the vector)
src/signing.rs   Ed25519 verify over JCS bytes (protocol §7)
src/main.rs      `gearbox verify` CLI
tests/vector.rs  byte-for-byte conformance + verify / tamper / untrusted-key / wrong-alg
```

## Conformance (the point of this crate)

`tests/vector.rs::jcs_reproduces_frozen_canonical_bytes` asserts the Rust JCS output equals
`catalog.canonical.json` **byte-for-byte** — the cross-language gate between this crate, the
Python signer (`tools/`), and the seed verifier (B4). If any of them drifts, it fails. This
is the concrete evidence that a Rust verifier built to seed B4 will interoperate with the
A4 signer.

Why a hand-rolled JCS instead of a crate: catalogs are restricted to the ASCII + integer
subset (protocol §7.1), so the in-tree canonicalizer is small, dependency-free, and
**gated by the vector** — the same approach as the Python reference.

## Scope / next

Verify only, for now. Next slices toward Python-tools parity: `gearbox catalog` (generate)
and `gearbox sign`. Dependencies are intentionally minimal — `serde_json`,
`ed25519-dalek`, `base64`; no JCS crate, no `clap` (hand-rolled arg parsing).
