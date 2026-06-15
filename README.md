# Gearbox

[![CI](https://github.com/shaal/gearbox/actions/workflows/ci.yml/badge.svg)](https://github.com/shaal/gearbox/actions/workflows/ci.yml)

**A cog store for Cognitum Seed — private, alternative, and self-hostable.**

> A gearbox is the housing that holds and meshes a set of cogs. This project is the
> housing for *software* cogs: the catalog, signing, and distribution layer that lets
> a Seed install cogs from a store **other than** the single official one — your
> company's private store, a community's alternative public store, or a one-cog repo
> you host yourself.

Status: **early / spec-stage.** This repo defines the store protocol and will hold the
reference tooling (catalog generator, signer, reference store server). The runtime that
*consumes* stores is [`cognitum-one/seed`](https://github.com/cognitum-one/seed); the
cogs themselves live in [`cognitum-one/cogs`](https://github.com/cognitum-one/cogs).

---

## Why

Today a Seed installs cogs from exactly one place: a hardcoded bucket
(`gs://cognitum-apps`) plus a single catalog. That blocks three real needs:

- **Enterprises** want a **private store** — internal cogs on their own infra, behind
  their own auth, optionally air-gapped, with the public store disabled on managed devices.
- **Communities** want **alternative public stores** (the F-Droid / Open VSX model).
- **Self-hosters** want to ship **one cog** from their own URL (e.g. a GPLv2 cog that
  shouldn't live in the MIT `cogs` repo).

Gearbox is the mechanism for all three.

Gearbox isn't a generic artifact registry — hosting and signing files is already solved
(Harbor, Artifactory, Cosign/Sigstore). The point is the **Seed-specific** contract: the
`cog.toml` manifest (the capabilities, permissions, and resource budgets a Seed enforces)
plus the device-side install → verify → run path. A plain registry stores bytes; Gearbox
distributes *cogs a Seed can trust and run*.

## The model

A **store** is a named source the Seed trusts: a catalog URL + an artifact base + a
trust anchor. Crossing from the official store to any other origin is gated by
**signatures**, not just hashes — because whoever controls a catalog controls the
hashes.

```toml
# Seed-side store list (illustrative)
[[store]]
id            = "acme-private"
name          = "ACME Internal"
catalog_url   = "https://cogs.acme.internal/app-registry.json"
artifact_base = "https://cogs.acme.internal/artifacts"   # gs:// | https:// | s3:// | oci:// | file://
auth          = { type = "bearer", token_ref = "secret://acme-store-token" }
trust         = ["acme-signing-2026"]                    # ed25519 keys this store must sign with
priority      = 10
enabled       = true
```

Cog manifests stay store-agnostic: assets already declare a **relative** path, so the
same cog can be served by any store. Ids are namespaced (`store-id/cog-id`) to resolve
collisions, with priority + pinning + managed-mode allow/deny on top.

Full design and rationale:
- Decision record: [`docs/adr/ADR-0001-pluggable-cog-stores.md`](docs/adr/ADR-0001-pluggable-cog-stores.md)
- Backing plan: [`docs/plans/pluggable-cog-stores.md`](docs/plans/pluggable-cog-stores.md)
- Protocol spec: [`docs/protocol/cog-store-protocol.md`](docs/protocol/cog-store-protocol.md)

## Beyond multi-store: where the store is headed

Multi-store is the foundation. On top of it, [`docs/research/cog-store-enhancements.md`](docs/research/cog-store-enhancements.md)
researches the broader roadmap — lifecycle (updates, rollback, channels, delta updates),
discovery (icons, search, ratings), trust & supply chain (provenance, SBOM, consent UX),
edge-specific wins (LAN peer caching, air-gap bundles, WASM portability), and developer
experience (a `gearbox` CLI, local dev stores).

## Repo layout

```
docs/
  adr/        ADR-0001-pluggable-cog-stores.md  # the foundational decision
              ADR-0002-store-info-and-tofu.md   # store.json identity + trust-on-first-use
  plans/      pluggable-cog-stores.md           # long-form design, rollout, prior art
              phase-1-implementation.md         # Phase 1: config-driven single store (done/specced)
              phase-2-implementation.md         # Phase 2: multi-store, namespacing, TOFU, auth
              phase-3-implementation.md         # Phase 3 (Tier 0): air-gap export, audit log, managed mode
  protocol/   cog-store-protocol.md             # the store + catalog + signing contract
              testvectors/                      # executable signing contract (gearbox#1)
  research/   cog-store-enhancements.md         # forward-looking feature research / roadmap
  strategy/   enterprise-readiness-matrix.md    # honest capability status + Tier 0/1/2 roadmap
tools/                                          # Python reference catalog generator + signer (gearbox#2)
  catalog_gen.py · verify_catalog.py · cogstore/ · testdata/ · selftest.sh
crates/gearbox/                                 # native Rust reference (gearbox#3): catalog + store-info + serve + air-gap bundle + audit log + managed policy
  src/ (jcs · signing · catalog · store · resolve · server · bundle · audit · policy · CLI) · tests/ — byte-for-byte vs the vectors + Python parity
examples/store-loop.sh                          # e2e demo: serve -> TOFU -> verify catalog -> fetch+hash artifact
examples/bundle-airgap.sh                       # e2e demo: export -> tar -> air-gap import via file:// -> tamper refused (T0-A)
examples/audit-log.sh                           # e2e demo: append -> verify hash-chained log -> edit/delete refused (T0-B)
examples/managed-mode.sh                        # e2e demo: sign policy -> enforce (allow/deny + audit) -> forgery refused (T0-C)
```

Planned: a reference store server; multi-store (Phase 2). The device-side verifier lives in
`cognitum-one/seed` (B4, specced in `docs/cross-repo/`).

## Relationship to the rest of Cognitum

| Repo | Role |
|---|---|
| [`cognitum-one/cogs`](https://github.com/cognitum-one/cogs) | The cogs + their `cog.toml` manifests + ADRs (source of truth for cog shape) |
| [`cognitum-one/seed`](https://github.com/cognitum-one/seed) | The device runtime that installs/runs cogs (consumes stores) |
| **gearbox** (this repo) | The store protocol + tooling that makes stores pluggable |

## Contributing

Early and spec-driven — see [CONTRIBUTING.md](CONTRIBUTING.md). The short version: the test
vectors in `docs/protocol/testvectors/` are the contract (changes must keep them byte-for-byte
and the Rust ↔ Python implementations in agreement), and protocol changes go through an ADR.
Build + test with `cargo test --manifest-path crates/gearbox/Cargo.toml` and `tools/selftest.sh`.
Security issues: see [SECURITY.md](SECURITY.md) — please use a private GitHub security
advisory, not a public issue.

## License

MIT — see [LICENSE](LICENSE).
