# seed B3 — Load the catalog from `catalog_url`

**Status**: Drafted (ready to apply as a `cognitum-one/seed` PR)
**Target repo**: `cognitum-one/seed`
**Workstream**: Phase 1 / B3 — the loader between fetch (B2) and verify (B4)
**Depends on**: B1 (`catalog_url`), B2 (fetcher)
**Pins**: [protocol §3](../../protocol/cog-store-protocol.md#3-catalog-app-registryjson)

## Goal

Load `app-registry.json` from the store's `catalog_url` (via the B2 fetcher), parse it into
a raw `serde_json::Value`, and apply only **untrusted** structural sanity checks. Single
store, **no merge**. The raw value is what B4 verifies; typed extraction happens **after**
verification.

## Why

Today the catalog is effectively a bundled/companion file. B3 makes it a fetched artifact
like any other — the prerequisite for a catalog that can live in any store. The subtle part
is *ordering and fidelity*: parse must preserve the exact document so B4 can canonicalize
it, and nothing in the catalog may be trusted until B4 says so.

## Design rules

- **Buffer, don't stream.** Unlike artifacts (B5), the catalog is small metadata — fetch it
  into memory through a **size-capped** in-memory sink (a few MB), so a hostile/oversized
  catalog can't OOM the device.
- **Keep the raw `Value`.** Parse to `serde_json::Value` and hand *that* to B4. Do **not**
  deserialize straight into typed structs and re-serialize for verification — that would
  drop unknown fields and change the canonical bytes (see B4). The typed view is a separate,
  post-verification read.
- **schema_version gate.** Accept `schema_version == 1`; a newer version → a clear
  "catalog is newer than this Seed supports — update the Seed" error (forward-compat /
  min-version gating, enhancements §E). An older/missing one → malformed.
- **No merge, one store** (Phase 2 adds multi-catalog merge + namespacing).
- Anti-rollback (reject a `generated_at` older than what's installed) and catalog caching
  are **out of scope** for Phase 1 — noted, not silently added (enhancements §A/§F).

## Call order

```
B3 load_catalog()  →  raw Value
                      └─ B4 verify_catalog(raw, trust)        # gate
                         └─ B3 parse_typed(verified)  →  Catalog
                            └─ B5 install(...)
```

## Reference sketch (Rust)

```rust
use serde_json::Value;

const SUPPORTED_SCHEMA: u64 = 1;
const MAX_CATALOG_BYTES: usize = 8 * 1024 * 1024;   // metadata, not an artifact

#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("fetch: {0}")] Fetch(#[from] super::fetch::FetchError),
    #[error("catalog exceeds {0} bytes")] TooLarge(usize),
    #[error("json: {0}")] Json(#[from] serde_json::Error),
    #[error("catalog is not a JSON object")] NotObject,
    #[error("missing/invalid `{0}`")] Missing(&'static str),
    #[error("catalog schema_version {found} is newer than supported ({SUPPORTED_SCHEMA}); update the Seed")]
    UnsupportedSchema { found: u64 },
}

/// Fetch + parse the catalog as a RAW value (for B4). Applies untrusted sanity only.
pub async fn load_catalog(store: &StoreDescriptor, fetcher: &dyn Fetcher)
    -> Result<Value, CatalogError>
{
    let mut buf = CappedBuf::new(MAX_CATALOG_BYTES);     // AsyncWrite that errors past the cap
    fetcher.fetch_to(&store.catalog_url, &mut buf).await?;
    let value: Value = serde_json::from_slice(&buf.into_inner())?;

    let obj = value.as_object().ok_or(CatalogError::NotObject)?;
    match obj.get("schema_version").and_then(Value::as_u64) {
        Some(v) if v == SUPPORTED_SCHEMA => {}
        Some(found) => return Err(CatalogError::UnsupportedSchema { found }),
        None => return Err(CatalogError::Missing("schema_version")),
    }
    if !obj.get("store_id").is_some_and(Value::is_string) { return Err(CatalogError::Missing("store_id")); }
    if !obj.get("cogs").is_some_and(Value::is_array)      { return Err(CatalogError::Missing("cogs")); }
    Ok(value)   // -> B4 verify -> parse_typed
}

// Typed read view. Unknown fields are ignored on purpose — the security boundary is the
// raw Value (B4); this struct is only ever built from an already-verified value.
#[derive(serde::Deserialize)]
pub struct Catalog {
    pub schema_version: u64,
    pub store_id: String,
    pub generated_at: String,
    pub cogs: Vec<CatalogCog>,
}
#[derive(serde::Deserialize)]
pub struct CatalogCog { pub id: String, pub versions: Vec<CatalogVersion> }
#[derive(serde::Deserialize)]
pub struct CatalogVersion { pub version: String, pub manifest: Value, pub artifacts: Artifacts }
#[derive(serde::Deserialize)]
pub struct Artifacts { pub binary: Artifact, #[serde(default)] pub assets: Vec<Artifact> }
#[derive(serde::Deserialize)]
pub struct Artifact {
    #[serde(default)] pub id: Option<String>,
    pub path: String, pub sha256: String, pub size: u64,
}

/// Typed view — call ONLY after B4 has verified `verified`.
pub fn parse_typed(verified: &Value) -> Result<Catalog, CatalogError> {
    Ok(serde_json::from_value(verified.clone())?)
}
```

## Acceptance criteria

- The Seed loads the catalog from `store.catalog_url` (works for `gs://` and `https://`);
  the bundled/in-repo catalog path is removed.
- A catalog over the size cap → `TooLarge`, not an OOM.
- `schema_version` newer than supported → a clear "update the Seed" error; missing/older or
  a non-object / missing `store_id` / missing `cogs` → a clear malformed error.
- The committed test vector
  ([`catalog.signed.json`](../../protocol/testvectors/catalog.signed.json)) parses, and the
  typed view yields cog `doom`, version `0.1.0`, binary path `cogs/arm/cog-doom-arm`.
- The raw `Value` handed to B4 is byte-faithful to the fetched document (no field dropping).
- Exactly one store is consulted; no catalog merge.
