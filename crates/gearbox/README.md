# gearbox (Rust) — native cog-store reference

Native Rust implementation of the cog-store protocol (gearbox#3): catalog **generation**,
**signing**, and **verification** — JCS (RFC 8785) + Ed25519 — matching the Python `tools/`
reference and pinned to the frozen test vector in
[`../../docs/protocol/testvectors/`](../../docs/protocol/testvectors/).

## Build / test / run

```bash
cargo test                                   # 7 tests (jcs/verify conformance + catalog gen)

gearbox catalog --cogs-dir DIR (--artifacts-dir DIR | --manifests-only) \
                --store-id ID --generated-at TS --out FILE [--sign-seed-hex HEX --key-id ID]
gearbox sign    --in FILE --out FILE --sign-seed-hex HEX --key-id ID
gearbox verify  <catalog.json> --key-id ID --pubkey-b64 B64
```

## Layout

```
src/jcs.rs       RFC 8785 canonicalization (ASCII+integer subset; matches the vector)
src/signing.rs   Ed25519 sign + verify over JCS bytes (protocol §7)
src/catalog.rs   build + validate app-registry.json from a cog.toml tree (protocol §3)
src/main.rs      `gearbox` CLI: catalog / sign / verify
tests/           vector.rs (verify conformance) · catalog.rs (generate / validate / sign)
```

## Conformance & cross-implementation parity

- `tests/vector.rs::jcs_reproduces_frozen_canonical_bytes` — the Rust JCS output equals
  `catalog.canonical.json` **byte-for-byte**.
- **Cross-impl parity**: the Rust and Python generators produce **byte-identical
  signatures** for the same inputs (checked in full + manifests-only modes), proving their
  JCS canonical bytes agree. Three implementations — the Python signer/verifier (`tools/`)
  and this Rust crate — all reproduce the frozen vector. A Rust verifier built to seed B4
  interoperates with the A4 signer by construction.

Why a hand-rolled JCS instead of a crate: catalogs are the ASCII + integer subset (protocol
§7.1), so the in-tree canonicalizer is small, dependency-free, and gated by the vector +
parity.

## Scope

Full parity with the Python tools (generate / sign / verify). Next (Phase 2+): a reference
store server; multi-store. Dependencies: `serde_json`, `ed25519-dalek`, `base64`, `toml`,
`sha2`, `hex` (no `clap` — hand-rolled arg parsing).
