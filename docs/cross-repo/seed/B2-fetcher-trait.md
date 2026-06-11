# seed B2 — `Fetcher` trait (`gs://` + `https://`)

**Status**: Drafted (ready to apply as a `cognitum-one/seed` PR)
**Target repo**: `cognitum-one/seed`
**Workstream**: Phase 1 / B2
**Depends on**: B1 (`StoreDescriptor.artifact_base`)
**Pins**: [protocol §2](../../protocol/cog-store-protocol.md#2-store-descriptor-seed-side)

## Goal

Abstract artifact fetching behind a trait keyed by the `artifact_base` **scheme**, so the
store base can be `gs://` (today) or `https://` (a mirror/CDN) with the same install path.
The trait shape lands now; `s3://` / `oci://` / `file://` are stubbed for Phase 2/3.

## Why

B1 made the base a config value; B2 makes its *scheme* pluggable — which is what actually
proves "config-driven" (point `artifact_base` at an HTTPS mirror of the same bytes and
installs still work) and what later unlocks private/enterprise backends.

## Design: stream, don't buffer

Artifacts include a ~28 MB IWAD and ~100 MB GGUF models, on a Pi Zero 2 W (512 MB RAM).
The fetcher therefore **streams into a sink** rather than returning a `Vec<u8>` — B5 wraps
that sink with a hashing writer so bytes are verified on the fly and never fully held in
memory. This is the single most important shape decision in B2.

## Changes

- A `Fetcher` trait: stream the object at `url` into a writer, return bytes written.
- `fetcher_for(artifact_base)` → the implementation for the scheme.
- Implement **`gs://`** (today's GCS access — anonymous/public read as now) and **`https://`**
  (the Seed's existing HTTP client; stream the response body).
- **Stub `s3://` / `oci://` / `file://`** → `UnsupportedScheme` error (clear, not a panic).
- A scheme-correct `join(artifact_base, relative_path)`.
- Minimal resilience: a couple of retries on transient 5xx / IO; **resumable/range requests
  are deferred** (enhancements §H) — note the cap, don't silently add it.
- `auth` is `None` for the public official store in Phase 1; the fetcher accepts the
  descriptor's auth so Phase 2 can add bearer/mTLS/cloud-IAM without reshaping the trait.

## Reference sketch (Rust)

```rust
use tokio::io::{AsyncWrite, AsyncWriteExt};

#[async_trait::async_trait]
pub trait Fetcher: Send + Sync {
    /// Stream `url` into `sink`; return bytes written. Streams so a 100 MB model
    /// never sits fully in RAM (B5 tees `sink` through a sha256 hasher).
    async fn fetch_to(
        &self, url: &str, sink: &mut (dyn AsyncWrite + Unpin + Send),
    ) -> Result<u64, FetchError>;
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("unsupported scheme {0:?} (Phase 1: gs|https)")] UnsupportedScheme(String),
    #[error("not found: {0}")] NotFound(String),
    #[error("http status {0}")] Http(u16),
    #[error("io: {0}")] Io(#[from] std::io::Error),
    #[error("transport: {0}")] Transport(String),
}

/// `artifact_base` + relative `path` -> full URL. `path` is validated relative upstream
/// (cogs A1 + the catalog validator), so this only normalizes the slash.
pub fn join(artifact_base: &str, rel: &str) -> String {
    format!("{}/{}", artifact_base.trim_end_matches('/'), rel.trim_start_matches('/'))
}

pub fn fetcher_for(artifact_base: &str) -> Result<Box<dyn Fetcher>, FetchError> {
    match artifact_base.split("://").next().unwrap_or("") {
        "gs"    => Ok(Box::new(GcsFetcher::new())),
        "https" => Ok(Box::new(HttpsFetcher::new())),
        other   => Err(FetchError::UnsupportedScheme(other.into())),
    }
}
```

- `GcsFetcher` — preserve today's GCS read path (whatever access the install handler uses
  now); stream the object body.
- `HttpsFetcher` — the Seed's HTTP client (hyper/reqwest); GET, follow the body as a stream,
  map non-2xx to `Http(status)` / 404 to `NotFound`.

## Acceptance criteria

- Install works via both `gs://` and `https://` bases for the same relative paths.
- A large artifact is fetched with **bounded memory** (streamed, not buffered).
- An unsupported scheme (`s3`/`oci`/`file`/other) → a clear `UnsupportedScheme` error.
- A 404 / non-2xx surfaces as `NotFound` / `Http(status)`, not a panic.
- `join("gs://b/cogs", "cogs/arm/x")` and trailing/leading-slash variants produce the
  expected URL.
