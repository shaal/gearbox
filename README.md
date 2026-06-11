# Gearbox

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
  adr/        ADR-0001-pluggable-cog-stores.md  # the decision record
  plans/      pluggable-cog-stores.md           # long-form design, rollout, prior art
              phase-1-implementation.md         # Phase 1 cross-repo task plan + issues
  protocol/   cog-store-protocol.md             # the store + catalog + signing contract
              testvectors/                      # executable signing contract (gearbox#1)
  research/   cog-store-enhancements.md         # forward-looking feature research / roadmap
tools/                                          # Python reference catalog generator + signer (gearbox#2)
  catalog_gen.py · verify_catalog.py · cogstore/ · testdata/ · selftest.sh
crates/gearbox/                                 # native Rust reference (gearbox#3): generate / sign / verify
  src/ (jcs · signing · catalog · CLI) · tests/ — byte-for-byte vs the vector + Python parity
```

Planned: a reference store server; multi-store (Phase 2). The device-side verifier lives in
`cognitum-one/seed` (B4, specced in `docs/cross-repo/`).

## Relationship to the rest of Cognitum

| Repo | Role |
|---|---|
| [`cognitum-one/cogs`](https://github.com/cognitum-one/cogs) | The cogs + their `cog.toml` manifests + ADRs (source of truth for cog shape) |
| [`cognitum-one/seed`](https://github.com/cognitum-one/seed) | The device runtime that installs/runs cogs (consumes stores) |
| **gearbox** (this repo) | The store protocol + tooling that makes stores pluggable |

## License

MIT — see [LICENSE](LICENSE).
