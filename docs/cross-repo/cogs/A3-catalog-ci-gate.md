# cogs A3 — CI gate: catalog builds from manifests + hashes well-formed

**Status**: Outline
**Target repo**: `cognitum-one/cogs` (`.github/workflows/ci.yml`)
**Workstream**: Phase 1 / A3
**Depends on**: A1 (`path` field); **a `--manifests-only` mode in the Gearbox generator** (follow-up to #2)

## Goal

A CI job that proves every `cog.toml` produces a valid catalog entry and that asset hashes
are well-formed — catching catalog-breaking manifest errors at PR time, before publish.

## Why

Today asset validation is a brittle grep (`asset-sha256-validate`, ci.yml 181–221). The
catalog generator already parses and validates the full catalog structure; running it in
CI makes one tool the single source of truth for "is this manifest publishable."

## Approach

Add a `catalog-validate` job that runs the Gearbox catalog generator over `src/cogs` in
**manifests-only** mode (no `-arm` binaries exist at PR time — those are built later in the
seed publish pipeline). It:

- builds each cog's catalog entry from its manifest,
- validates structure: relative paths, sha256 = 64 lowercase hex, positive sizes, exactly
  one of `path`/`gcs_path` (subsumes A1's checks and the old `asset-sha256` grep),
- **skips binary hashing** (no artifact to hash yet).

Retire the `asset-sha256-validate` grep step; this supersedes it.

## Generator dependency (Gearbox follow-up to #2)

`catalog_gen.py` currently requires `--artifacts-dir` and hashes the binary. Add a
`--manifests-only` flag that:

- drops the `--artifacts-dir` requirement,
- emits each binary artifact with no hash + a `"pending": true` marker (or omits the
  binary block), while still validating manifest-derived fields and **asset** hashes,
- keeps `validate()` passing for the manifests-only shape.

Small change in `tools/cogstore/catalog.py` + the CLI. (Worth filing as its own Gearbox
issue alongside #2/#3.)

## Acceptance

- A PR that breaks a manifest (bad sha, both/neither `path`/`gcs_path`, absolute path,
  missing `size_bytes`) → the job fails, naming the offending cog and reason.
- Clean manifests → the job passes; the old grep-based asset gate is removed.
