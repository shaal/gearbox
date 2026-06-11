# cogs A3 — CI gate: catalog builds from manifests + hashes well-formed

**Status**: Drafted
**Target repo**: `cognitum-one/cogs` (`.github/workflows/ci.yml`)
**Workstream**: Phase 1 / A3
**Depends on**: A1 (`path` field); `gearbox catalog --manifests-only` — **done** (Rust `crates/gearbox` and Python `tools/`)

## Goal

A CI job that proves every `cog.toml` produces a valid catalog entry and that asset hashes
are well-formed — catching catalog-breaking manifest errors at PR time, before publish.

## Why

Today asset validation is a brittle grep (`asset-sha256-validate`, ci.yml 181–221). The
catalog generator already parses and validates the full catalog structure; running it in CI
makes one tool the single source of truth for "is this manifest publishable," and it
strictly supersedes the grep.

## The generator mode (done)

At PR time on cogs there are **no `-arm` binaries** (those are built later in the seed
publish pipeline), so the generator can't hash binaries — it runs in **`--manifests-only`**
mode (binary entry `{path, pending: true}`; manifest-derived fields and asset hashes still
validated). This mode is implemented in **both** `crates/gearbox` (Rust, canonical) and
`tools/` (Python, cross-check). CI uses the **Rust binary** — cogs is already a Rust
workspace, so `cargo` is present and no Python/`pip` is needed
([Phase 2 plan §11](../../plans/phase-2-implementation.md)).

## The cogs CI job

Replace the `asset-sha256-validate` grep step with a `catalog-validate` job:

```yaml
      - uses: actions/checkout@v4
        with: { repository: <org>/gearbox, ref: <pinned-tag>, path: gearbox }
      - run: cargo build --release --manifest-path gearbox/crates/gearbox/Cargo.toml
      - name: Validate catalog builds from manifests
        run: |
          gearbox/crates/gearbox/target/release/gearbox catalog \
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
standalone validator can be retired in favor of the single Rust `gearbox` gate (Rust is
canonical per the Phase 2 plan §11). Order: A1 now → A3 once the gate lands.

## Acceptance criteria

- The `gearbox catalog --manifests-only` mode exists and is tested (done: `crates/gearbox`
  tests + `tools/selftest.sh`).
- A cogs PR that breaks a manifest (bad sha, both/neither `path`/`gcs_path`, absolute path,
  missing `size_bytes`, malformed) → the job fails, naming the cog and reason.
- Clean manifests → the job passes; the old grep-based `asset-sha256` gate is removed.
- The job runs without any built binary present (manifests-only).
