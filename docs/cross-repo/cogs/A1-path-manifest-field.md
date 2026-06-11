# cogs A1 — scheme-agnostic `path` field in `[[assets]]`

**Status**: Drafted (ready to apply as a `cognitum-one/cogs` PR)
**Target repo**: `cognitum-one/cogs`
**Workstream**: Phase 1 / A1 ([plan §5.2](../../plans/phase-1-implementation.md), [ADR-0001](../../adr/ADR-0001-pluggable-cog-stores.md) §4.2)
**Depends on**: nothing — independent; unblocks A3 and seed B5

## Goal

Add a scheme-agnostic relative **`path`** to `[[assets]]`; keep **`gcs_path`** as a
back-compat alias. Exactly one of the two per asset. **Zero behavior change** for existing
cogs — both resolve identically; the active store's `artifact_base` selects the base (for
the official store that base *is* the GCS bucket).

## Why

The cog manifest is the one place the store origin leaks in by name (`gcs_path`). `path`
makes assets store-agnostic so the same cog can be served by any store. `gcs_path` stays
valid, so the only two asset-using cogs today need no change.

## Current state (verbatim, for the implementer)

- **Asset fields today**: `id`, `filename`, `size_bytes`, `sha256`, `gcs_path`, optional
  `required_when` / `source_url` / `license`. Examples:
  `src/cogs/doom/cog.toml:55–62`, `src/cogs/cognitive-pipeline/cog.toml` (4 assets).
- **All manifest validation is inline bash** in `.github/workflows/ci.yml` — no Rust
  deserializes `cog.toml`; the seed agent parses it at install time. So the cogs-side
  change is **docs + CI only**.
  - `manifest-validate` (lines 109–149): required files, `id` == dirname, binary name
    `cog-<dirname>`.
  - `asset-sha256-validate` (lines 181–221): greps `sha256 = "TODO-` and fails unless a
    `.allow-unpublished-assets` marker is present.
- **Migration surface**: only `doom` (1 asset) and `cognitive-pipeline` (4 assets) use
  `[[assets]]`; all 5 hashes are real.

## Changes

### 1. Field semantics (document in ADR-001 + authoring notes)

- **`path`** (string, relative) — the artifact's location relative to the active store's
  `artifact_base`. **Preferred** for new cogs.
- **`gcs_path`** (string, relative) — deprecated alias; identical meaning, implies the
  official GCS base. Still accepted.
- **Exactly one** of `path` / `gcs_path` per `[[assets]]` (error on both or neither).
- `path` must be **relative**: no scheme (`://`), no leading `/`.

### 2. CI validation — replace the brittle grep with a `tomllib` check

The current asset gate is grep-only and can't express "exactly one of two fields." GitHub
runners have Python ≥ 3.11 (`tomllib` is stdlib), so add a small validator (Appendix) and
call it from the asset-validation job over each changed `cog.toml`. It enforces the A1
rules **and** subsumes the existing `sha256 = "TODO-"` check, so the old grep step is
retired.

### 3. Docs

- **ADR-001** contract summary: add `path` to the `[[assets]]` example; note `gcs_path` is
  a deprecated alias and state the exactly-one + relative rules.
- One-line note in the `doom` / `cognitive-pipeline` manifests is optional; their existing
  `gcs_path` stays valid.

### 4. Migration

No forced migration. `doom` + `cognitive-pipeline` keep `gcs_path`. Optional cosmetic
follow-up: rename their `gcs_path` → `path` (identical bytes/sha).

### 5. Forward-compatibility (already in place)

The Gearbox catalog generator already reads `a.get("path") or a["gcs_path"]`
(`tools/cogstore/catalog.py`), so it accepts both the moment A1 lands. The seed verifier
(B5) resolves `path` against `artifact_base`.

## Acceptance criteria

- A cog with `path = "..."` passes `manifest-validate` + the asset gate and builds.
- An asset with **both** `path` and `gcs_path` → CI fails with a clear message.
- An asset with **neither** → CI fails.
- A `path` containing `://` or a leading `/` → CI fails.
- Existing `gcs_path`-only cogs still pass unchanged.
- ADR-001 documents `path` and marks `gcs_path` deprecated.

## Appendix — drop-in CI validator (`scripts/validate_assets.py`)

```python
#!/usr/bin/env python3
"""Validate [[assets]] blocks in the given cog.toml files (cogs CI, A1)."""
import sys, re, tomllib, pathlib
HEX = re.compile(r"^[0-9a-f]{64}$")
def err(p, msg): print(f"::error file={p}::{msg}"); return 1
def main(paths):
    rc = 0
    for p in paths:
        allow = (pathlib.Path(p).parent / ".allow-unpublished-assets").exists()
        data = tomllib.loads(pathlib.Path(p).read_text())
        for i, a in enumerate(data.get("assets", [])):
            where = f"[[assets]] #{i} (id={a.get('id', '?')})"
            has_path, has_gcs = "path" in a, "gcs_path" in a
            if has_path == has_gcs:                      # both or neither
                rc |= err(p, f"{where}: set exactly one of `path` or `gcs_path`"); continue
            rel = a.get("path") or a.get("gcs_path")
            if "://" in rel or rel.startswith("/"):
                rc |= err(p, f"{where}: artifact path must be relative, got {rel!r}")
            for req in ("id", "filename", "size_bytes", "sha256"):
                if req not in a: rc |= err(p, f"{where}: missing `{req}`")
            sha = str(a.get("sha256", ""))
            if not allow and (sha.startswith("TODO-") or not HEX.match(sha)):
                rc |= err(p, f"{where}: sha256 must be 64 lowercase hex "
                             f"(or add .allow-unpublished-assets)")
            if not isinstance(a.get("size_bytes"), int) or a.get("size_bytes", 0) <= 0:
                rc |= err(p, f"{where}: size_bytes must be a positive integer")
    return rc
if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
```

Wire-up in `.github/workflows/ci.yml` (replacing the `asset-sha256-validate` grep step):

```yaml
      - name: Validate cog assets
        run: python3 scripts/validate_assets.py $(git ls-files 'src/cogs/*/cog.toml')
```
