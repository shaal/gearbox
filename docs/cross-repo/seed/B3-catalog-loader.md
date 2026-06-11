# seed B3 — Load the catalog from `catalog_url`

**Status**: Outline
**Target repo**: `cognitum-one/seed`
**Workstream**: Phase 1 / B3
**Depends on**: B1 (descriptor), B2 (fetcher)

## Goal

Load `app-registry.json` from the store's `catalog_url` instead of a bundled in-repo file.
Single store, **no merge** (multi-catalog merge is Phase 2).

## Changes

- Catalog loader fetches `catalog_url` (via B2 / https) and parses it per
  [protocol §3](../../protocol/cog-store-protocol.md#3-catalog-app-registryjson):
  `schema_version`, `store_id`, `generated_at`, `cogs[].versions[].{manifest, artifacts}`.
- Remove the bundled/in-repo catalog path.
- Data model matches the generator output (`tools/` / gearbox#2); the test vector
  (`docs/protocol/testvectors/catalog.signed.json`) is a parse fixture.

## Acceptance

- The Seed reads the catalog from config; the bundled-file path is gone.
- Exactly one store is consulted.
- Parses the committed test-vector catalog without error.
