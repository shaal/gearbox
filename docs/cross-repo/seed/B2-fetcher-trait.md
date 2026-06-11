# seed B2 — `Fetcher` trait (`gs://` + `https://`)

**Status**: Outline
**Target repo**: `cognitum-one/seed`
**Workstream**: Phase 1 / B2
**Depends on**: B1

## Goal

Abstract artifact fetching behind a trait keyed by the `artifact_base` scheme, so the
store base can be any supported scheme rather than a hardcoded GCS path.

## Changes

- A `Fetcher` trait — `fetch(&self, url) -> Result<Bytes>` (async to match the runtime).
- Resolve the implementation from the `artifact_base` scheme.
- **Implement `gs://`** (today's behavior) and **`https://`** (the cheap second scheme
  that proves config-driven, [plan §6](../../plans/phase-1-implementation.md#6-decisions-resolved-2026-06-10)).
- **Stub `s3://` / `oci://` / `file://`** to return a clear "unsupported in Phase 1" error
  — the trait shape lands now; the schemes arrive in Phase 2/3.
- A scheme-correct `join(artifact_base, relative_path)` helper.

## Acceptance

- Install works via both `gs://` and `https://`.
- An unsupported scheme yields a clear error, not a panic.
