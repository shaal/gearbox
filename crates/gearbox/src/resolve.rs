//! Multi-store resolution (Phase 2 §6): map a cog reference to a concrete `(store, cog)`,
//! honoring explicit namespacing, pins, and store priority. **Pure logic, no I/O** — a Seed
//! projects its `StoreDescriptor`s (protocol §2) + verified catalogs + pins into a `Resolver`
//! and asks it *which store* serves a reference. Catalog verification + trust live elsewhere
//! (seed B4); this module never fetches or verifies anything.
//!
//! Rules (Phase 2 §6):
//! - `store/cog` (namespaced) → exactly that store's `cog`.
//! - `cog` (bare), pinned    → the pinned store, always (until unpinned).
//! - `cog` (bare), unpinned  → the **enabled** store with the lowest `priority` that offers it
//!   (ties broken by store id, for determinism).

use std::collections::{BTreeSet, HashMap};

/// The minimal store projection resolution needs (a subset of the seed `StoreDescriptor`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreRef {
    pub id: String,
    pub priority: u32,
    pub enabled: bool,
}

/// A parsed cog reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CogRef {
    Namespaced { store_id: String, cog_id: String },
    Bare { cog_id: String },
}

/// Parse `store/cog` (namespaced) or `cog` (bare) — exactly zero or one `/`.
pub fn parse_ref(s: &str) -> Result<CogRef, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty reference".into());
    }
    let parts: Vec<&str> = s.split('/').collect();
    match parts.as_slice() {
        [cog] => {
            check_token(cog, "cog id")?;
            Ok(CogRef::Bare { cog_id: (*cog).to_string() })
        }
        [store, cog] => {
            check_token(store, "store id")?;
            check_token(cog, "cog id")?;
            Ok(CogRef::Namespaced {
                store_id: (*store).to_string(),
                cog_id: (*cog).to_string(),
            })
        }
        _ => Err(format!("reference {s:?} has too many '/' (use store/cog or cog)")),
    }
}

fn check_token(t: &str, what: &str) -> Result<(), String> {
    if t.is_empty() {
        return Err(format!("empty {what}"));
    }
    if t.chars().any(char::is_whitespace) {
        return Err(format!("{what} {t:?} contains whitespace"));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveReason {
    Explicit, // namespaced store/cog
    Pinned,   // bare cog with a pin
    Priority, // bare cog, lowest-priority enabled store that offers it
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub store_id: String,
    pub cog_id: String,
    pub reason: ResolveReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    BadRef(String),
    UnknownStore(String),
    StoreDisabled(String),
    NotOffered { store_id: String, cog_id: String },
    NotFound(String),
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::BadRef(m) => write!(f, "bad reference: {m}"),
            ResolveError::UnknownStore(s) => write!(f, "unknown store {s:?}"),
            ResolveError::StoreDisabled(s) => write!(f, "store {s:?} is disabled"),
            ResolveError::NotOffered { store_id, cog_id } => {
                write!(f, "store {store_id:?} does not offer cog {cog_id:?}")
            }
            ResolveError::NotFound(c) => write!(f, "no enabled store offers cog {c:?}"),
        }
    }
}
impl std::error::Error for ResolveError {}

/// Resolves cog references against a fixed set of stores, offerings, and pins.
pub struct Resolver {
    stores: HashMap<String, StoreRef>,
    offerings: HashMap<String, BTreeSet<String>>, // store_id -> cog ids it offers
    pins: HashMap<String, String>,                // cog_id -> store_id
}

impl Resolver {
    pub fn new(
        stores: Vec<StoreRef>,
        offerings: HashMap<String, BTreeSet<String>>,
        pins: HashMap<String, String>,
    ) -> Result<Self, String> {
        let mut map: HashMap<String, StoreRef> = HashMap::new();
        for s in stores {
            if map.contains_key(&s.id) {
                return Err(format!("duplicate store id {:?}", s.id));
            }
            map.insert(s.id.clone(), s);
        }
        for k in offerings.keys() {
            if !map.contains_key(k) {
                return Err(format!("offerings reference unknown store {k:?}"));
            }
        }
        for (cog, store) in &pins {
            if !map.contains_key(store) {
                return Err(format!("pin for {cog:?} references unknown store {store:?}"));
            }
        }
        Ok(Self { stores: map, offerings, pins })
    }

    fn offers(&self, store_id: &str, cog_id: &str) -> bool {
        self.offerings.get(store_id).is_some_and(|s| s.contains(cog_id))
    }

    /// Resolve a reference to a concrete `(store, cog)` or a typed error.
    pub fn resolve(&self, query: &str) -> Result<Resolution, ResolveError> {
        match parse_ref(query).map_err(ResolveError::BadRef)? {
            CogRef::Namespaced { store_id, cog_id } => {
                self.check_store_serves(&store_id, &cog_id)?;
                Ok(Resolution { store_id, cog_id, reason: ResolveReason::Explicit })
            }
            CogRef::Bare { cog_id } => {
                if let Some(pin_store) = self.pins.get(&cog_id) {
                    let pin_store = pin_store.clone();
                    self.check_store_serves(&pin_store, &cog_id)?;
                    return Ok(Resolution { store_id: pin_store, cog_id, reason: ResolveReason::Pinned });
                }
                match self.candidates(&cog_id).first() {
                    Some(store) => Ok(Resolution {
                        store_id: store.id.clone(),
                        cog_id,
                        reason: ResolveReason::Priority,
                    }),
                    None => Err(ResolveError::NotFound(cog_id)),
                }
            }
        }
    }

    fn check_store_serves(&self, store_id: &str, cog_id: &str) -> Result<(), ResolveError> {
        let store = self.stores.get(store_id).ok_or_else(|| ResolveError::UnknownStore(store_id.to_string()))?;
        if !store.enabled {
            return Err(ResolveError::StoreDisabled(store_id.to_string()));
        }
        if !self.offers(store_id, cog_id) {
            return Err(ResolveError::NotOffered {
                store_id: store_id.to_string(),
                cog_id: cog_id.to_string(),
            });
        }
        Ok(())
    }

    /// Enabled stores that offer `cog_id`, in resolution order: ascending priority, then id.
    pub fn candidates(&self, cog_id: &str) -> Vec<&StoreRef> {
        let mut v: Vec<&StoreRef> = self
            .stores
            .values()
            .filter(|s| s.enabled && self.offers(&s.id, cog_id))
            .collect();
        v.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));
        v
    }

    /// Cog ids offered by more than one **enabled** store (for dashboard badging). Sorted,
    /// each with its offering store ids sorted.
    pub fn collisions(&self) -> Vec<(String, Vec<String>)> {
        let mut by_cog: HashMap<String, Vec<String>> = HashMap::new();
        for (store_id, cogs) in &self.offerings {
            if self.stores.get(store_id).is_some_and(|s| s.enabled) {
                for c in cogs {
                    by_cog.entry(c.clone()).or_default().push(store_id.clone());
                }
            }
        }
        let mut out: Vec<(String, Vec<String>)> = by_cog
            .into_iter()
            .filter(|(_, s)| s.len() > 1)
            .map(|(c, mut s)| {
                s.sort();
                (c, s)
            })
            .collect();
        out.sort();
        out
    }
}
