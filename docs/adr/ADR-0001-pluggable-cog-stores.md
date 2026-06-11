# ADR-0001: Pluggable cog stores (private & alternative)

**Status**: Proposed
**Date**: 2026-06-10
**Related**: [Plan: Pluggable cog stores](../plans/pluggable-cog-stores.md),
[Cog Store Protocol](../protocol/cog-store-protocol.md),
[cognitum-one/cogs ADR-001 (Cogs as plugins)](https://github.com/cognitum-one/cogs/blob/main/docs/adrs/ADR-001-cogs-as-plugins-architecture.md),
[cognitum-one/cogs ADR-019 (DOOM — foreshadows the "separate repo" case)](https://github.com/cognitum-one/cogs/blob/main/docs/adrs/ADR-019-doom.md),
[cognitum-one/seed ADR-095 (Cogs as API providers)](https://github.com/cognitum-one/seed/blob/main/docs/seed/ADR-095-cogs-as-api-providers.md)

## Context

Today a Seed installs cogs from exactly **one** store: the catalog
(`app-registry.json`, in the seed repo) plus a single hardcoded artifact bucket,
`gs://cognitum-apps`. The cog manifest is already store-agnostic — `cog.toml
[[assets]]` declares only a **relative** `gcs_path` (e.g. `wads/freedoom1.wad`),
which the Seed resolves under `gs://cognitum-apps/cogs/<arch>/`. The single store
origin is baked into the **Seed runtime**, not into any cog.

Three concrete demands push past one central store:

1. **Enterprises** want a **private store** — internal cogs and a vetted subset of
   public ones, on infrastructure they control, behind their own auth, with the
   option to forbid the public store on managed devices.
2. **Other communities** want **alternative public stores** (the F-Droid / Open VSX
   model alongside the official one).
3. **Single-cog self-hosters** want to ship one cog from their own URL. ADR-019
   already named this: a GPLv2 cog the maintainers may not want in the MIT cogs repo
   could live in "a separate repository referenced from the registry." Alternative
   stores are that mechanism.

The forces: the cog format barely needs to change, but **`sha256` protects the
download, not the decision to download** — whoever controls a catalog chooses the
hashes. Crossing into a second, less-trusted origin therefore requires
**signatures and a per-store trust anchor**, plus auth, namespacing, and policy —
all of which live in the Seed.

## Decision

Adopt a **pluggable, signed, multi-store model**, sequenced in phases (full design
in the [plan](../plans/pluggable-cog-stores.md)):

1. **Store descriptor** — the Seed holds a *list* of stores (`id`, `catalog_url`,
   `artifact_base`, `priority`, `auth`, `trust` keys, `enabled`) instead of one
   hardcoded base. (The APT `sources.list` / F-Droid "repos" pattern.)
2. **Scheme-agnostic artifact paths** — add a relative `path` to `cog.toml
   [[assets]]`, resolved against the active store's `artifact_base`; keep `gcs_path`
   as a back-compat alias so **every existing manifest is unchanged**. The Seed
   gains a fetcher abstraction for `gs://`, `https://`, `s3://`, `oci://`, `file://`.
3. **Portable, signed catalog** — specify `app-registry.json` as a versioned,
   ed25519-signed artifact any operator can generate and host.
4. **Trust store** — the Seed verifies the catalog signature against keys trusted
   *for that store*, then sha256-verifies each artifact against the signed manifest.
   Official store → Cognitum release key; private store → admin-provisioned key;
   public alt store → trust-on-first-use with a shown fingerprint.
5. **Namespacing + policy** — namespaced ids (`store-id/cog-id`) with priority and
   pinning to resolve collisions; managed-mode allow/deny lists so enterprises can
   restrict or disable stores.
6. **Configurable publish pipeline** — parameterize the cogs repo's catalog generator +
   CI (`STORE_ARTIFACT_BASE`, `STORE_CATALOG_URL`, `STORE_SIGNING_KEY`) so a fork can
   run an alternative store **without forking the Seed**.

**Most of the implementation is Seed-side** (multi-catalog resolution, trust, auth,
policy, UX). The rest splits across `cognitum-one/cogs` (the additive `path`
manifest field, a configurable catalog generator, CI) and **gearbox** (this repo:
the store/catalog/signing spec + reference tooling). **Phase 1** is a pure,
no-behavior-change refactor — the Seed reads the official store from config instead
of a constant — and lands first.

## Consequences

- **Positive**: enterprises get private/air-gapped stores; communities can run
  alternative public stores; the GPL-`doom`/single-cog case gets a clean home
  outside the MIT cogs repo; the cog format stays stable (`path` is additive).
- **Negative**: introduces real cryptographic trust machinery (signing, key
  custody, rotation, revocation) and a policy/UX surface in the Seed that didn't
  exist; cross-repo coordination (gearbox spec ↔ seed runtime ↔ cogs) for every phase.
- **Neutral**: the store becomes a **distribution** boundary only — runtime
  containment (loopback bind, bearer tokens, `[mesh] permissions`, capability
  grants, resource budgets) still gates every cog regardless of origin. Adding a
  store is explicitly **not** a sandbox bypass.

## Alternatives considered

- **Keep one central store** — rejected: blocks every motivating case (enterprise
  privacy, air-gap, alternative communities, GPL sidecar repos).
- **Just let `artifact_base` be overridden, reuse `sha256` for trust** — rejected:
  hashes come from the catalog, so a second origin's hashes prove nothing about
  authenticity. Signing is mandatory across a trust boundary.
- **Hardcode each cog's source in its `cog.toml`** — rejected: couples a cog to one
  store, breaks mirroring/forking, and bloats every manifest. Source is a Seed/store
  concern, not a cog concern.
- **Flat (non-namespaced) ids with last-writer-wins** — rejected: silent collisions
  and store-shadowing attacks. Namespacing + priority + pinning instead.
- **Put the store spec + tooling inside `cogs` or `seed`** — rejected: it's
  cross-cutting (consumed by the seed runtime, produced by store operators, and it
  references cog shape from cogs). It lives in its own repo (gearbox) so any operator
  can implement a store without cloning the device runtime or the cog monorepo.
