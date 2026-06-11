# Cross-repo change specs (staged in Gearbox)

Phase 1 work that targets **`cognitum-one/cogs`** or **`cognitum-one/seed`** is drafted
here first — under `docs/cross-repo/` — instead of being filed as issues/PRs in those
repos. Each doc is a ready-to-apply blueprint; when we're ready it becomes a PR or issue
in its target repo. This keeps the whole initiative reviewable in one place.

The two **Gearbox-native** pieces are real here, not staged: the signing contract
(`docs/protocol/` + test vector, issue #1) and the catalog generator (`tools/`, issue #2).

## Status

| Item | Target | Title | Status |
|---|---|---|---|
| A1 | cogs | `path` manifest field (alias `gcs_path`) + CI gate | **Drafted** — [cogs/A1](cogs/A1-path-manifest-field.md) |
| A3 | cogs | CI: catalog builds from manifests + hash gate | **Drafted** — [cogs/A3](cogs/A3-catalog-ci-gate.md) |
| A4 | seed ¹ | Sign the official catalog in the publish pipeline | **Drafted** — [seed/A4](seed/A4-sign-catalog-in-publish.md) |
| B1 | seed | `StoreDescriptor` config; remove the hardcoded base | **Drafted** — [seed/B1](seed/B1-store-descriptor-config.md) |
| B2 | seed | `Fetcher` trait (`gs://`, `https://`) | **Drafted** — [seed/B2](seed/B2-fetcher-trait.md) |
| B3 | seed | Load the catalog from `catalog_url` | **Drafted** — [seed/B3](seed/B3-catalog-loader.md) |
| B4 | seed | Verify the catalog signature; fail closed | **Drafted** — [seed/B4](seed/B4-verify-signature.md) |
| B5 | seed | Resolve `path`; sha256 vs the signed manifest | **Drafted** — [seed/B5](seed/B5-install-via-path.md) |
| B6 | seed | `require_signed_catalog` transition flag | **Drafted** — [seed/B6](seed/B6-transition-flag.md) |

¹ Plan §5.2 listed A4 under `cogs`, but the publish/upload workflow is **not** in the
`cogs` repo — it lives in `cognitum-one/seed`. The signing step attaches there, so A4 is
filed under `seed/`.

**Gearbox-native** (real issues in this repo, not staged here): **C1** signing format
([#1](https://github.com/shaal/gearbox/issues/1), done), **A2** catalog generator
([#2](https://github.com/shaal/gearbox/issues/2), done), **C2** native Rust CLI
([#3](https://github.com/shaal/gearbox/issues/3)).

## Sequencing

See the [Phase 1 plan](../plans/phase-1-implementation.md) and
[epic #4](https://github.com/shaal/gearbox/issues/4). Critical path:
signing format (#1) → sign (A4) + verify (B4). A1 is independent and unblocks A3 + B5.

> **Gearbox follow-ups surfaced while drafting** (both small additions to `tools/`, after #2):
> 1. **`--manifests-only` mode** — for the cogs A3 PR-time gate, which builds the catalog
>    before any `-arm` binary exists (skip binary hashing). See [cogs/A3](cogs/A3-catalog-ci-gate.md).
> 2. **Enrich `artifacts.assets[]`** with `filename` (and `required_when`) so the install
>    record is self-contained, instead of making the Seed join by `id` against the embedded
>    manifest. See [seed/B5](seed/B5-install-via-path.md).
