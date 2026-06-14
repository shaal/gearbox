# Contributing to Gearbox

Thanks for your interest. Gearbox is the store protocol + reference tooling that lets a
Cognitum Seed install cogs from a store other than the single official one (see the
[README](README.md)). It is **early and spec-driven**: the protocol and its test vectors are
the source of truth, and the code exists to implement and prove them.

## How the project is organized

| Path | What |
|---|---|
| [`docs/protocol/`](docs/protocol/) | The store protocol — the contract everything implements |
| [`docs/protocol/testvectors/`](docs/protocol/testvectors/) | **Frozen** signing test vectors (the byte-exact contract) |
| [`docs/adr/`](docs/adr/) | Architecture Decision Records (the *why*) |
| [`docs/plans/`](docs/plans/), [`docs/cross-repo/`](docs/cross-repo/) | Phased implementation plans + specs for `cognitum-one/seed` and `cognitum-one/cogs` |
| [`crates/gearbox/`](crates/gearbox/) | The **canonical** Rust implementation + CLI |
| [`tools/`](tools/) | The Python reference (an independent cross-check) |
| [`docs/strategy/`](docs/strategy/) | Capability status + roadmap |

## Build & test

- **Rust (canonical)** — MSRV 1.75:
  ```bash
  cargo test --manifest-path crates/gearbox/Cargo.toml      # all crate tests
  cargo build --manifest-path crates/gearbox/Cargo.toml     # build the `gearbox` CLI
  ```
- **Python tools** — Python ≥ 3.11:
  ```bash
  pip install -r tools/requirements.txt    # cryptography (Ed25519)
  tools/selftest.sh
  ```
- **End-to-end demo** (build the CLI first; needs `curl`, `python3`, `sha256sum`):
  ```bash
  examples/store-loop.sh
  ```

A change is "green" when **both** `cargo test` and `tools/selftest.sh` pass. CI
([`.github/workflows/ci.yml`](.github/workflows/ci.yml)) enforces this on every PR: `rust`,
`python`, cross-implementation `parity`, `lint` (rustfmt + clippy), and `supply-chain`
(`cargo deny`) all run as required gates. Run them locally with `cargo fmt --check`,
`cargo clippy --all-targets -- -D warnings`, and (optional) `cargo deny check`.

## The one rule: don't break the test vectors

Signing is the heart of this project, and `docs/protocol/testvectors/*.canonical.json` plus
the committed signatures are its **contract**. Any change that touches canonicalization (JCS)
or signing **must**:

1. keep `cargo test` and `tools/selftest.sh` green (they assert the Rust and Python output
   match the frozen vectors **byte-for-byte**), and
2. keep the **Rust ↔ Python parity** — the two implementations must produce identical
   canonical bytes and signatures for the same input.

If a protocol change genuinely requires new vectors, that is a **breaking change**: regenerate
the vectors deterministically, bump the relevant `schema_version`/`key_id`, and call it out in
the PR. Don't quietly edit a `*.canonical.json`.

Constraints that keep the implementations in agreement (protocol §7.1): **integer** numbers,
**ASCII** object keys, **UTF-8** string values. Floats and non-ASCII keys are rejected on
purpose — don't "relax" them without an ADR.

## Two implementations, on purpose

The **Rust crate is canonical** (it's what the Seed/publish path uses). The **Python tools are
an independent cross-check oracle** — two implementations catch canonicalization bugs that one
would miss (that's how the non-ASCII divergence was found). Keep them in sync, or have a very
good reason not to.

Dependencies are kept minimal (`serde_json`, `ed25519-dalek`, `base64`, `toml`, `sha2`, `hex`;
no `clap`; the dev store server is std-only). Adding a dependency needs a reason in the PR.

## Proposing changes

- **Protocol or trust-model changes** → open an **ADR** (`docs/adr/`, see the
  [index](docs/adr/README.md) and existing ADRs for the format) before/with the code. These
  are load-bearing decisions.
- **Tooling / CLI / fixes** → a PR with tests is enough. New behavior needs a test.
- **Work for the Seed runtime or cog manifests** lands in `cognitum-one/seed` /
  `cognitum-one/cogs`; the specs in [`docs/cross-repo/`](docs/cross-repo/) describe it.
- **Roadmap**: see the [enterprise-readiness matrix](docs/strategy/enterprise-readiness-matrix.md)
  for what's built vs. planned and where help is most useful.

Keep PRs focused; explain the *why*, not just the *what*. Match the surrounding style.

## Reporting security issues

Gearbox is a signing/trust system — please report vulnerabilities **privately**, not in a
public issue. See [SECURITY.md](SECURITY.md) for the policy, scope, and response targets; in
short, use GitHub's **private security advisory** ("Security" tab → "Report a
vulnerability"). Note the threat model already documented in
[ADR-0002](docs/adr/ADR-0002-store-info-and-tofu.md) (e.g. TOFU's unauthenticated first
fetch); reports that sharpen or extend it are welcome.

The keys/seeds in `docs/protocol/testvectors/` are **published test material** — not secrets.

## License

By contributing, you agree your contributions are licensed under the repository's
[MIT License](LICENSE).
