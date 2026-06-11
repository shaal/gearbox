# seed B5 — Resolve `path`; sha256 vs the signed manifest

**Status**: Drafted (ready to apply as a `cognitum-one/seed` PR)
**Target repo**: `cognitum-one/seed`
**Workstream**: Phase 1 / B5 — closes the install loop
**Depends on**: A1 (`path` field), B2 (fetcher), B3 (catalog), B4 (verified signature)
**Pins**: [protocol §4](../../protocol/cog-store-protocol.md#4-install-algorithm-seed-side)

## Goal

Install a cog by resolving each artifact's relative `path` against the active store's
`artifact_base`, streaming it via B2, **sha256-verifying against the B4-verified catalog**,
and atomically placing it under `COG_DATA_DIR`. Behavior is identical to today — now
authenticity-gated.

## Why

This is where authenticity becomes integrity: B4 proved the catalog is genuine, so the
sha256 values *in that catalog* are trustworthy, and checking artifacts against them is what
actually protects the device. The hashes come from the **verified catalog**, never from the
transport and never from a locally re-read manifest.

## Algorithm (protocol §4)

For the resolved cog version (from the verified catalog):

1. Determine the artifacts to install: the **binary** plus each **asset whose
   `required_when` holds** for this cog's config (see *required_when* below).
2. For each artifact:
   - `url = join(store.artifact_base, artifact.path)` (accept the `gcs_path` alias).
   - Stream-fetch (B2) into a temp file under `COG_DATA_DIR`, hashing on the fly.
   - Assert `sha256(bytes) == artifact.sha256` from the **verified** catalog; on mismatch,
     delete the temp file and **abort the install**.
   - Atomically `rename` the temp file to its destination.
3. Mark the binary executable. Install is all-or-nothing.

## Destination layout

- **Binary** → `COG_DATA_DIR/<binary>` (e.g. `cog-doom-arm`), `chmod +x`.
- **Asset** → `COG_DATA_DIR/<filename>` — the local `filename` is the manifest's `filename`
  field (e.g. `freedoom1.wad`, or `smollm2-135m/model.gguf` which implies a subdir), **not**
  the store `path`.

> **Refinement surfaced here (Gearbox follow-up to #2).** The catalog splits asset data:
> `artifacts.assets[]` carries `{id, path, sha256, size}`, while the local `filename` and
> `required_when` live in the embedded `manifest.assets[]`. So B5 must **join by `id`**
> across the two. Cleaner: have the generator copy `filename` (and `required_when`) into the
> `artifacts.assets[]` entries, making the install record self-contained. B5 below works
> with the join today; recommend enriching the generator so it doesn't have to. Both are
> covered by the verified signature either way.

## required_when (Phase 1 parity)

`cognitive-pipeline` declares conditional assets, e.g.
`required_when = "config.model_id == 'smollm2-135m'"`. The Seed already evaluates this when
deciding which assets to download; B5 **must preserve that behavior** so model selection
keeps working. Evaluate against the cog's effective config; skip assets whose condition is
false.

## Atomicity & idempotency

- Download → temp (`*.part`) → verify → atomic `rename` into place: never leaves a
  partially written or unverified artifact.
- Re-install is idempotent (same bytes, same destinations).
- On any failure (fetch error, hash mismatch), clean up temp files and report which artifact
  failed and why.

## Reference sketch (Rust)

```rust
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("fetch: {0}")] Fetch(#[from] super::fetch::FetchError),
    #[error("io: {0}")] Io(#[from] std::io::Error),
    #[error("sha256 mismatch for {rel}: expected {expected}, got {got}")]
    HashMismatch { rel: String, expected: String, got: String },
}

/// Fetch one artifact, verify its sha256 against the verified catalog, place it atomically.
async fn install_artifact(
    fetcher: &dyn Fetcher, base: &str, rel_path: &str, expected_sha256: &str, dest: &Path,
) -> Result<(), InstallError> {
    if let Some(parent) = dest.parent() { tokio::fs::create_dir_all(parent).await?; }
    let tmp = dest.with_extension("part");
    let mut hasher = Sha256::new();
    {
        let file = tokio::fs::File::create(&tmp).await?;
        let mut sink = HashingWriter::new(file, &mut hasher);   // tees bytes -> file + hasher
        fetcher.fetch_to(&join(base, rel_path), &mut sink).await?;
        sink.shutdown().await?;
    }
    let got = hex::encode(hasher.finalize());
    if got != expected_sha256 {
        tokio::fs::remove_file(&tmp).await.ok();
        return Err(InstallError::HashMismatch {
            rel: rel_path.into(), expected: expected_sha256.into(), got });
    }
    tokio::fs::rename(&tmp, dest).await?;   // atomic within COG_DATA_DIR
    Ok(())
}
```

`HashingWriter` is a thin `AsyncWrite` adapter that forwards bytes to the file and feeds the
same bytes to the `Sha256` — so verification adds no extra pass and no extra memory.

## Acceptance criteria

- `path`-based and `gcs_path`-based cogs install identically.
- Every artifact is verified against the **signed** catalog's sha256; a mismatch aborts the
  install and leaves nothing partial behind.
- `required_when` conditional assets behave exactly as today (model selection works).
- Large artifacts install with bounded memory (streamed + hashed, no full buffering).
- Re-install is idempotent; a failed install cleans up its temp files.
- Hashes are never taken from the transport or a locally re-read manifest — only from the
  B4-verified catalog.
