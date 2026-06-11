# seed B1 — `StoreDescriptor` config; remove the hardcoded base

**Status**: Drafted (ready to apply as a `cognitum-one/seed` PR)
**Target repo**: `cognitum-one/seed`
**Workstream**: Phase 1 / B1 — the foundation B2–B5 build on
**Depends on**: nothing
**Pins**: [protocol §2](../../protocol/cog-store-protocol.md#2-store-descriptor-seed-side)

## Goal

Replace the compiled-in `gs://cognitum-apps` base with a config-driven **store
descriptor**. When no store is configured, a built-in official default reproduces today's
behavior **exactly** — no user-visible change. This is the single seam every later phase
plugs into: Phase 2 adds more descriptors to the same list; Phase 3 adds auth/policy to
each.

## Why

The hardcoded base is the entire reason alternative stores don't exist
([ADR-0001](../../adr/ADR-0001-pluggable-cog-stores.md)). Until the Seed reads the store
from config, nothing downstream (multi-store, private auth, managed policy) is possible.
B1 changes *where the base comes from*, not *what it is*.

## Current state

The Seed has the bucket `gs://cognitum-apps` baked into the install path (the install
handler resolves binaries + assets under `gs://cognitum-apps/cogs/<arch>/…`). Exact symbol
/ file lives in the seed repo (not in this workspace) — step 1 of the PR is to grep for the
literal and route every use through the descriptor.

## Changes

### 1. The `StoreDescriptor` type (mirror protocol §2)

Fields: `id`, `name`, `catalog_url`, `artifact_base`, `trust` (key_ids), optional `auth`,
`priority`, `enabled`. `auth` is modeled now but is `None` for the public official store;
real auth backends are Phase 2/3.

### 2. Config surface

Add an optional `[[store]]` array to the Seed's device config. **Phase 1 holds exactly one
enabled store.** If the array is omitted, the Seed synthesizes the official default — so
existing devices upgrade with no config change (this is what makes B1 invisible).

```toml
# optional — omit entirely to use the built-in official store
[[store]]
id            = "cognitum-official"
catalog_url   = "gs://cognitum-apps/cogs/app-registry.json"
artifact_base = "gs://cognitum-apps/cogs"
trust         = ["cognitum-release-2026"]
# priority = 0, enabled = true, auth = none  (defaults)
```

### 3. Defaults that reproduce today

`official_default()`: `id = "cognitum-official"`, `artifact_base = "gs://cognitum-apps/cogs"`,
`catalog_url = <official app-registry.json>` (a `gs://` object or an `https://` mirror —
B3 loads it), `trust = ["cognitum-release-<year>"]`, `priority = 0`, `enabled = true`,
`auth = None`.

### 4. Remove the constant

Delete the compiled-in bucket literal; route all resolution through
`active_store().artifact_base`. Add a grep-guard test asserting the literal no longer
appears in non-default code paths.

### 5. Single-store invariant (Phase 1)

Model `stores` as a list (the Phase 2 shape) but require **exactly one enabled** store;
error on zero or many. Phase 2 lifts this to priority-ordered resolution + namespacing — no
type changes needed then.

## Hand-off to the rest of Phase 1

- `artifact_base` → **B2** fetcher (scheme selects the implementation).
- `catalog_url` → **B3** catalog loader.
- `trust` → **B4** trust store (`key_id`s the catalog must be signed by).
- `priority` / `enabled` → inert in Phase 1; **Phase 2** resolution consumes them.

## Reference sketch (Rust)

```rust
#[derive(Debug, Clone, serde::Deserialize)]
pub struct StoreDescriptor {
    pub id: String,
    #[serde(default)] pub name: String,
    pub catalog_url: String,
    pub artifact_base: String,
    pub trust: Vec<String>,                              // key_ids -> B4 TrustStore
    #[serde(default)] pub auth: Option<StoreAuth>,       // None for the public official store
    #[serde(default = "default_priority")] pub priority: u32,
    #[serde(default = "default_true")]     pub enabled: bool,
}
fn default_priority() -> u32 { 100 }
fn default_true() -> bool { true }

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StoreAuth { Bearer { token_ref: String } }      // mtls/gcp/aws: Phase 2/3

const OFFICIAL_KEY_ID: &str = "cognitum-release-2026";

impl StoreDescriptor {
    /// Built-in official store — used when no `[[store]]` is configured, so existing
    /// devices behave exactly as before B1.
    pub fn official_default() -> Self {
        Self {
            id: "cognitum-official".into(),
            name: "Cognitum Official".into(),
            catalog_url: "gs://cognitum-apps/cogs/app-registry.json".into(),
            artifact_base: "gs://cognitum-apps/cogs".into(),
            trust: vec![OFFICIAL_KEY_ID.into()],
            auth: None,
            priority: 0,
            enabled: true,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreConfigError {
    #[error("no enabled store configured")] NoStore,
    #[error("more than one enabled store — multi-store is Phase 2")] MultiStore,
    #[error("store {0:?}: {1}")] Invalid(String, &'static str),
}

/// Resolve the single active store for Phase 1 (defaulting to the official store).
pub fn active_store(configured: &[StoreDescriptor]) -> Result<StoreDescriptor, StoreConfigError> {
    let mut stores = configured.to_vec();
    if stores.is_empty() { stores.push(StoreDescriptor::official_default()); }
    let mut enabled = stores.into_iter().filter(|s| s.enabled);
    let store = enabled.next().ok_or(StoreConfigError::NoStore)?;
    if enabled.next().is_some() { return Err(StoreConfigError::MultiStore); }
    validate(&store)?;
    Ok(store)
}

fn validate(s: &StoreDescriptor) -> Result<(), StoreConfigError> {
    let bad = |m| StoreConfigError::Invalid(s.id.clone(), m);
    if s.id.is_empty() { return Err(bad("empty id")); }
    if s.trust.is_empty() { return Err(bad("trust must list >=1 key_id")); }
    let scheme = s.artifact_base.split("://").next().unwrap_or("");
    if !matches!(scheme, "gs" | "https") {            // Phase 1 fetchers (B2)
        return Err(bad("artifact_base scheme unsupported in Phase 1 (gs|https)"));
    }
    Ok(())
}
```

## Acceptance criteria

- A Seed with **no** `[[store]]` config installs all cogs identically to before B1.
- A Seed with the explicit official `[[store]]` block behaves identically to the default.
- No hardcoded `gs://cognitum-apps` literal remains in resolution paths (grep-guard test).
- Setting `artifact_base` to an `https://` mirror of the same bytes resolves end-to-end
  (proves config-driven) — exercised once **B2** lands.
- Zero or two-plus enabled stores → a clear config error (multi-store deferred to Phase 2).
- `trust` empty or unsupported `artifact_base` scheme → a clear config error.
