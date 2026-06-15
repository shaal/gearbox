# gearbox (Rust) — native cog-store reference

Native Rust implementation of the cog-store protocol (gearbox#3): catalog **generation**,
**signing**, **verification**, and the **store-info document** — JCS (RFC 8785) + Ed25519 —
matching the Python `tools/` reference and pinned to the frozen test vectors in
[`../../docs/protocol/testvectors/`](../../docs/protocol/testvectors/).

## Build / test / run

```bash
cargo test                                   # 33 tests (jcs/verify + catalog + store-info + server + resolve + bundle)

gearbox catalog --cogs-dir DIR (--artifacts-dir DIR | --manifests-only) \
                --store-id ID --generated-at TS --out FILE [--sign-seed-hex HEX --key-id ID]
gearbox sign    --in FILE --out FILE --sign-seed-hex HEX --key-id ID
gearbox verify  <catalog.json> --key-id ID --pubkey-b64 B64
gearbox store-info create --store-id ID --name NAME [--description D] --catalog-url URL \
                --key-id KID (--sign-seed-hex HEX | --pubkey-b64 B64) --out FILE
gearbox store-info verify <store.json>       # prints fingerprints + checks self-signature
gearbox export  --catalog app-registry.json --store-info store.json --artifacts-dir DIR \
                --out BUNDLE --generated-at TS [--sign-seed-hex HEX --key-id ID]  # air-gap bundle
gearbox import  <bundle-dir> [--expect-fingerprint HEX]      # verify + install via file:// (T0-A)
gearbox serve   --dir DIR [--port N] [--auth-token TOKEN]   # reference store server (dev)
```

## Layout

```
src/jcs.rs       RFC 8785 canonicalization (integer numbers, ASCII keys, UTF-8 values)
src/signing.rs   Ed25519 sign + verify over JCS bytes (protocol §7); generic over documents
src/catalog.rs   build + validate app-registry.json from a cog.toml tree (protocol §3)
src/store.rs     build / validate / fingerprint / self-verify store.json (protocol §8)
src/resolve.rs   multi-store resolution: namespacing / priority / pins (Phase 2 §6) — pure, no I/O
src/server.rs    minimal dependency-free HTTP store server (dev): store.json + catalog + artifacts
src/bundle.rs    air-gap bundle (Phase 3 T0-A): export/import a signed file://-installable store (§10)
src/main.rs      `gearbox` CLI: catalog / sign / verify / store-info / export / import / serve
tests/           vector.rs · catalog.rs · store.rs · server.rs · resolve.rs · bundle.rs
```

End-to-end demos:
- [`examples/store-loop.sh`](../../examples/store-loop.sh) builds a store, serves it, then runs
  the add-store loop a Seed would (TOFU fingerprint → verify catalog → fetch+hash artifact),
  including bearer auth.
- [`examples/bundle-airgap.sh`](../../examples/bundle-airgap.sh) exports a signed air-gap bundle,
  carries it across an "air gap" as a `tar`, imports it via `file://` with the same verification,
  and shows a single tampered artifact byte being refused.

## Conformance & cross-implementation parity

- `tests/{vector,store}.rs` assert the Rust JCS output equals
  `catalog.canonical.json` / `store.canonical.json` **byte-for-byte**.
- **Cross-impl parity**: the Rust and Python generators produce **byte-identical signatures**
  for the same inputs — including **non-ASCII** descriptions (e.g. `Wîdget — café`) — proving
  their JCS canonical bytes agree. Three implementations (Python signer/verifier in `tools/`
  and this Rust crate) reproduce the frozen vectors. A Rust verifier built to seed B4
  interoperates with the A4 signer by construction.

Why a hand-rolled JCS instead of a crate: documents are restricted to integer numbers + ASCII
keys (string values may be any UTF-8), so the in-tree canonicalizer is small, dependency-free,
and gated by the vectors + parity.

## Scope

Full parity with the Python tools (generate / sign / verify), plus `store-info` (Phase 2 TOFU
identity), `serve` (a dependency-free reference store server), `resolve` (multi-store
namespacing / priority / pins), and `export`/`import` (Phase 3 T0-A air-gap bundle, with a
`tools/cogstore/bundle.py` parity oracle). Next (Phase 2+): wiring these into the seed's
multi-store add flow + UX (specced in `docs/cross-repo/`). Dependencies: `serde_json`,
`ed25519-dalek`, `base64`, `toml`, `sha2`, `hex` — the server, resolver, and bundle add none
(std-only); no `clap`.
