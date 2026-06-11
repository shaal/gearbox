# seed B1 — `StoreDescriptor` config; remove the hardcoded base

**Status**: Outline
**Target repo**: `cognitum-one/seed`
**Workstream**: Phase 1 / B1
**Depends on**: nothing

## Goal

Replace the compiled-in `gs://cognitum-apps` base with a config-driven **store
descriptor**. The default config reproduces today's behavior exactly — no user-visible
change.

## Changes

- Introduce a `StoreDescriptor` type mirroring [protocol §2](../../protocol/cog-store-protocol.md#2-store-descriptor-seed-side):
  `id`, `name`, `catalog_url`, `artifact_base`, `trust` (key ids), optional `auth`,
  `priority`, `enabled`.
- Build the single official store from config with defaults that reproduce today:
  `id = "cognitum-official"`, `artifact_base = "gs://cognitum-apps/cogs"`,
  `catalog_url = <official>`, `trust = ["cognitum-release-<year>"]`, `priority = 0`,
  `enabled = true`.
- Remove every compiled-in reference to the bucket constant; route all resolution through
  the descriptor.
- **Phase 1 holds exactly one descriptor** (multi-store is Phase 2).

## Acceptance

- Default config installs all cogs identically to today.
- No hardcoded bucket constant remains (grep clean).
- Setting `artifact_base` to an `https://` mirror of the same bytes works end-to-end
  (proves the base is config-driven) — exercised once B2 lands.
