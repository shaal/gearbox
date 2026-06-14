# Security Policy

Gearbox is the signing and distribution layer for Cognitum Seed cogs. A vulnerability here
can affect the integrity of software shipped to devices, so we treat security reports as the
highest priority.

## Reporting a vulnerability

**Please do not open a public issue for security problems.**

Report privately through GitHub's [private security advisories][advisory] for this
repository (Security → Advisories → *Report a vulnerability*). If you cannot use that
channel, contact the maintainers listed in [`.github/CODEOWNERS`](.github/CODEOWNERS).

Please include, where possible:

- the affected component (protocol spec, Rust `crates/gearbox`, Python `tools/`, or a doc),
- the version/commit,
- a description of the impact (e.g. signature forgery, canonicalization divergence,
  catalog tampering that verification fails to catch),
- and a minimal reproduction or proof of concept.

[advisory]: https://github.com/shaal/gearbox/security/advisories/new

## Response targets

This is an early, community-maintained project; these are goals, not contractual SLAs:

| Stage | Target |
|---|---|
| Acknowledge receipt | within 3 business days |
| Initial assessment / severity | within 7 business days |
| Fix or mitigation plan | depends on severity, communicated in the assessment |

We will keep you updated through the advisory thread and credit you in the release notes
unless you prefer to remain anonymous. We support coordinated disclosure and ask for
reasonable time to ship a fix before public details are published.

## Scope

In scope — anything that undermines the trust model:

- **Signature / verification flaws** — forging a valid signature, or verification accepting
  a tampered catalog or `store.json`.
- **Canonicalization (JCS) divergence** — inputs where the Rust and Python implementations,
  or the spec, disagree on the bytes that get signed.
- **Trust-bootstrap (TOFU) weaknesses** — store-identity confusion or impersonation
  (see [ADR-0002](docs/adr/ADR-0002-store-info-and-tofu.md)).
- **Namespacing / resolution bypass** — a store shadowing or impersonating another store's
  cog (`crates/gearbox/src/resolve.rs`).
- **Reference store server** issues (`gearbox serve`): auth bypass, path traversal.
- **Supply-chain** issues in the gearbox tooling's own dependencies.

Out of scope:

- Vulnerabilities in the consuming runtime (report those to
  [`cognitum-one/seed`](https://github.com/cognitum-one/seed)) or in individual cogs
  ([`cognitum-one/cogs`](https://github.com/cognitum-one/cogs)).
- Theoretical weaknesses in Ed25519 / SHA-256 themselves.
- Findings against a deployer's own infrastructure (their store host, auth, TLS config).

## The contract

The frozen test vectors in [`docs/protocol/testvectors/`](docs/protocol/testvectors/) are the
authoritative definition of correct signing behaviour. Any change to signing or
canonicalization must keep those vectors byte-for-byte and keep the Rust and Python
implementations in agreement — CI enforces this. A report that breaks this invariant is a
security issue, not just a bug.

## Supported versions

Gearbox is pre-1.0 (spec-stage). Security fixes are applied to the `main` branch. There is no
back-port guarantee for older tags until a stable release line exists.
