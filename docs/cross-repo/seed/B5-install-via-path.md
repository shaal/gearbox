# seed B5 — Resolve `path`; sha256 vs the signed manifest

**Status**: Outline
**Target repo**: `cognitum-one/seed`
**Workstream**: Phase 1 / B5
**Depends on**: A1 (`path` field), B2 (fetcher), B3 (catalog), B4 (verified signature)

## Goal

Install using each artifact's relative `path` resolved against the active store's
`artifact_base`, sha256-verifying every artifact against the **signed** catalog. Behavior
is identical to today — now signature-gated.

## Changes

- For each artifact (binary + assets): `url = join(artifact_base, path)`; accept the
  `gcs_path` alias (A1).
- Fetch via B2; assert `sha256(bytes) == artifact.sha256` **from the verified catalog**
  (B4) — never a hash the transport alone supplied.
- Place under `COG_DATA_DIR` exactly as today; injection of `COGNITUM_COG_TOKEN` /
  `COGNITUM_COG_DATA_DIR` unchanged.

## Acceptance

- `path`-based and `gcs_path`-based cogs install identically.
- A hash mismatch → refuse to install.
- Hashes are checked against the signed catalog, closing the
  "[hashes ≠ authenticity across stores](../../adr/ADR-0001-pluggable-cog-stores.md)" gap.
