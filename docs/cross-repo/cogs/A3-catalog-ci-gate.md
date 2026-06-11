# cogs A3 — CI gate: catalog builds from manifests + hashes well-formed

**Status**: Drafted (ready to apply, after the generator follow-up below)
**Target repo**: `cognitum-one/cogs` (`.github/workflows/ci.yml`)
**Workstream**: Phase 1 / A3
**Depends on**: A1 (`path` field); **a `--manifests-only` mode in the Gearbox generator** (gearbox-native, see below)

## Goal

A CI job that proves every `cog.toml` produces a valid catalog entry and that asset hashes
are well-formed — catching catalog-breaking manifest errors at PR time, before publish.

## Why

Today asset validation is a brittle grep (`asset-sha256-validate`, ci.yml 181–221). The
catalog generator already parses and validates the full catalog structure; running it in CI
makes one tool the single source of truth for "is this manifest publishable," and it
strictly supersedes the grep.

## The generator dependency (Gearbox-native, follow-up to #2)

At PR time on cogs there are **no `-arm` binaries** (those are built later in the seed
publish pipeline), so the generator can't hash binaries. Add a **`--manifests-only`** mode
to `tools/catalog_gen.py` + `cogstore/catalog.py`:

- drops the `--artifacts-dir` requirement,
- for each cog, emits the catalog entry from the manifest, with the binary artifact marked
  `{"path": "...", "pending": true}` (no `sha256`/`size`) instead of hashing a file,
- still validates manifest-derived fields and **asset** hashes (which come from the
  manifest, the CI-gated source of truth),
- `validate()` accepts a `pending` binary (relaxes only the binary `sha256`/`size`
  requirement when `pending == true`; everything else unchanged).

This is small and lives in Gearbox `tools/`. It is **implemented in Gearbox**, then consumed
by this cogs CI job.

## The cogs CI job

Replace the `asset-sha256-validate` grep step with a `catalog-validate` job:

```yaml
      - uses: actions/checkout@v4
        with: { repository: <org>/gearbox, ref: <pinned-tag>, path: gearbox }
      - run: pip install cryptography   # tomllib is stdlib on 3.11+
      - name: Validate catalog builds from manifests
        run: |
          python3 gearbox/tools/catalog_gen.py \
            --cogs-dir src/cogs --manifests-only \
            --store-id cognitum-official \
            --generated-at "$(git show -s --format=%cI HEAD)" \
            --out /tmp/app-registry.json
```

A non-zero exit (bad sha256, both/neither `path`/`gcs_path`, absolute/`scheme://` path,
missing `size_bytes`, malformed structure) fails the PR with the offending cog + reason.

## Relationship to A1's validator

A1 ships a lightweight, dependency-free `validate_assets.py` as the **immediate** gate (it
lands first, no Gearbox dependency). A3's generator-based gate is **broader** (full catalog
structure, not just assets) and becomes the single source of truth — at which point A1's
standalone validator can be retired in favor of the generator. Order: A1 now → A3 once
`--manifests-only` exists.

## Acceptance criteria

- The generator gains a tested `--manifests-only` mode (Gearbox `tools/` + self-test).
- A cogs PR that breaks a manifest (bad sha, both/neither `path`/`gcs_path`, absolute path,
  missing `size_bytes`, malformed) → the job fails, naming the cog and reason.
- Clean manifests → the job passes; the old grep-based `asset-sha256` gate is removed.
- The job runs without any built binary present (manifests-only).
