//! Managed-mode policy (Phase 3, T0-C) — a **signed** document an admin distributes to managed
//! devices (MDM / image bake / bundle) that the resolver enforces (ADR-0003, protocol §12).
//!
//! ```jsonc
//! {
//!   "schema_version": 1,
//!   "managed": true,
//!   "allow_stores": ["acme-internal"],          // only these store ids may stay enabled
//!   "deny_public": true,                          // force-disable the built-in official store
//!   "forced_pins": { "doom": "acme-internal" },  // admin pins; the user layer cannot override
//!   "allow_user_add_store": false,                // may a user TOFU-add their own store?
//!   "signature": { "key_id": "…", "alg": "ed25519", "sig": "…" }   // org policy key (§7.2)
//! }
//! ```
//!
//! Enforcement is a **pre-resolution projection** — `Policy::project(stores, pins)` flips
//! `enabled` flags and overlays forced pins, then hands the result to the *unchanged*
//! `resolve::Resolver`. Policy only ever **restricts** (drop stores, force pins); it grants no
//! new authority (Phase 2's per-store-key namespace rule stands). An out-of-policy reference
//! fails through the resolver's existing typed errors, which the CLI relabels a policy denial.
//!
//! **Fail-closed**: authority is the org policy key, so `verify` reuses `signing::verify_document`
//! against the pinned key. An unsigned / forged / wrong-key policy fails verification and is
//! rejected — there is deliberately no "missing policy → open default" path (that absence is the
//! fail-closed property; see ADR-0003 / §8).

use std::collections::{BTreeSet, HashMap};

use serde_json::{json, Map, Value};

use crate::resolve::StoreRef;
use crate::signing::{self, TrustStore};

/// The built-in official / public store id (ADR-0001). `deny_public` force-disables it.
pub const OFFICIAL_STORE_ID: &str = "cognitum-official";

/// A parsed managed-mode policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    pub managed: bool,
    pub allow_stores: Vec<String>,
    pub deny_public: bool,
    pub forced_pins: HashMap<String, String>,
    pub allow_user_add_store: bool,
}

impl Policy {
    /// Parse + validate a (possibly signed) policy document. The `signature` member is ignored
    /// here — verify it separately with [`verify_signed`].
    pub fn from_json(doc: &Value) -> Result<Policy, String> {
        validate(doc)?;
        let o = doc.as_object().unwrap();
        let allow_stores = o
            .get("allow_stores")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let mut forced_pins = HashMap::new();
        if let Some(p) = o.get("forced_pins").and_then(Value::as_object) {
            for (cog, store) in p {
                forced_pins.insert(cog.clone(), store.as_str().unwrap_or_default().to_string());
            }
        }
        Ok(Policy {
            managed: o.get("managed").and_then(Value::as_bool).unwrap_or(false),
            allow_stores,
            deny_public: o
                .get("deny_public")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            forced_pins,
            allow_user_add_store: o
                .get("allow_user_add_store")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    /// Project a device's stores + user pins through the policy, **in front of** the resolver:
    /// 1. if `allow_stores` is non-empty, force-disable any store not in it;
    /// 2. if `deny_public`, force-disable the built-in official store ([`OFFICIAL_STORE_ID`]);
    /// 3. overlay `forced_pins` onto the user pins (the admin pin wins).
    ///
    /// The resolution rules themselves are unchanged — an out-of-policy reference then resolves
    /// to the resolver's existing `StoreDisabled` / `NotFound` error.
    pub fn project(
        &self,
        stores: Vec<StoreRef>,
        pins: HashMap<String, String>,
    ) -> (Vec<StoreRef>, HashMap<String, String>) {
        let allow: Option<BTreeSet<&str>> = if self.allow_stores.is_empty() {
            None
        } else {
            Some(self.allow_stores.iter().map(String::as_str).collect())
        };
        let stores = stores
            .into_iter()
            .map(|mut s| {
                if let Some(allow) = &allow {
                    if !allow.contains(s.id.as_str()) {
                        s.enabled = false;
                    }
                }
                if self.deny_public && s.id == OFFICIAL_STORE_ID {
                    s.enabled = false;
                }
                s
            })
            .collect();

        let mut pins = pins;
        for (cog, store) in &self.forced_pins {
            pins.insert(cog.clone(), store.clone()); // forced pins override user pins
        }
        (stores, pins)
    }
}

/// Build an (unsigned) managed policy document. `managed` is always true — this is the managed
/// policy artifact; an absent/false `managed` would not be enforced (see §8 fail-closed).
pub fn build_policy(
    allow_stores: &[String],
    deny_public: bool,
    forced_pins: &Map<String, Value>,
    allow_user_add_store: bool,
) -> Value {
    json!({
        "schema_version": 1,
        "managed": true,
        "allow_stores": allow_stores,
        "deny_public": deny_public,
        "forced_pins": Value::Object(forced_pins.clone()),
        "allow_user_add_store": allow_user_add_store,
    })
}

/// Verify a policy's signature against a trusted org-policy-key trust store; return the key id.
/// Fail-closed: an unsigned / wrong-key / malformed-schema policy is an error.
pub fn verify_signed(doc: &Value, trust: &TrustStore) -> Result<String, String> {
    validate(doc)?;
    signing::verify_document(doc, trust)
}

pub fn validate(doc: &Value) -> Result<(), String> {
    let o = doc.as_object().ok_or("invalid policy: not an object")?;
    if o.get("schema_version").and_then(Value::as_i64) != Some(1) {
        return Err("invalid policy: schema_version must be 1".into());
    }
    if o.get("managed").and_then(Value::as_bool).is_none() {
        return Err("invalid policy: managed must be a boolean".into());
    }
    if o.get("deny_public").and_then(Value::as_bool).is_none() {
        return Err("invalid policy: deny_public must be a boolean".into());
    }
    if o.get("allow_user_add_store")
        .and_then(Value::as_bool)
        .is_none()
    {
        return Err("invalid policy: allow_user_add_store must be a boolean".into());
    }
    let allow = o
        .get("allow_stores")
        .and_then(Value::as_array)
        .ok_or("invalid policy: allow_stores must be a list")?;
    for s in allow {
        if !s.is_string() {
            return Err("invalid policy: allow_stores entries must be strings".into());
        }
    }
    let pins = o
        .get("forced_pins")
        .and_then(Value::as_object)
        .ok_or("invalid policy: forced_pins must be an object")?;
    for (cog, store) in pins {
        if !store.is_string() {
            return Err(format!(
                "invalid policy: forced_pin {cog:?} must map to a store id string"
            ));
        }
    }
    Ok(())
}

/// The device's configured stores for a `policy check` dry-run: which stores exist, their
/// priority/enabled state, the cog ids each offers, and any user pins. This is a device-state
/// projection (not the published `store.json` identity doc).
///
/// ```jsonc
/// {
///   "stores": [
///     { "id": "acme-internal",    "priority": 10, "enabled": true, "cogs": ["doom"] },
///     { "id": "cognitum-official", "priority": 50, "enabled": true, "cogs": ["doom"] }
///   ],
///   "pins": { "tetris": "cognitum-official" }
/// }
/// ```
pub type DeviceStores = (
    Vec<StoreRef>,
    HashMap<String, BTreeSet<String>>,
    HashMap<String, String>,
);

/// Parse a device-stores document into `(stores, offerings, pins)` for a `resolve::Resolver`.
pub fn parse_device_stores(doc: &Value) -> Result<DeviceStores, String> {
    let arr = doc
        .get("stores")
        .and_then(Value::as_array)
        .ok_or("invalid stores file: `stores` must be a list")?;
    let mut stores = Vec::new();
    let mut offerings: HashMap<String, BTreeSet<String>> = HashMap::new();
    for s in arr {
        let id = s
            .get("id")
            .and_then(Value::as_str)
            .ok_or("invalid stores file: store missing `id`")?;
        let priority = s.get("priority").and_then(Value::as_u64).ok_or_else(|| {
            format!("invalid stores file: store {id:?} missing integer `priority`")
        })? as u32;
        let enabled = s.get("enabled").and_then(Value::as_bool).unwrap_or(true);
        let cogs: BTreeSet<String> = s
            .get("cogs")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|c| c.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        stores.push(StoreRef {
            id: id.to_string(),
            priority,
            enabled,
        });
        offerings.insert(id.to_string(), cogs);
    }
    let mut pins = HashMap::new();
    if let Some(p) = doc.get("pins").and_then(Value::as_object) {
        for (cog, store) in p {
            pins.insert(cog.clone(), store.as_str().unwrap_or_default().to_string());
        }
    }
    Ok((stores, offerings, pins))
}

/// Parse repeated `cog=store` tokens into a `forced_pins` object (split on the first `=`).
pub fn parse_forced_pins(pairs: &[String]) -> Result<Map<String, Value>, String> {
    let mut m = Map::new();
    for p in pairs {
        let (cog, store) = p
            .split_once('=')
            .ok_or_else(|| format!("--forced-pin {p:?} must be cog=store"))?;
        if cog.is_empty() || store.is_empty() {
            return Err(format!(
                "--forced-pin {p:?}: cog and store must both be non-empty"
            ));
        }
        m.insert(cog.to_string(), Value::from(store));
    }
    Ok(m)
}
