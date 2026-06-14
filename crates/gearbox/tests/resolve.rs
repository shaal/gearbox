//! Multi-store resolution tests (Phase 2 §6): parsing, validation, and the
//! namespaced / pinned / priority rules, plus candidates + collisions.

use std::collections::{BTreeSet, HashMap};

use gearbox::resolve::{self, CogRef, ResolveError, ResolveReason, Resolver, StoreRef};

fn store(id: &str, priority: u32, enabled: bool) -> StoreRef {
    StoreRef {
        id: id.into(),
        priority,
        enabled,
    }
}

fn offerings(pairs: &[(&str, &[&str])]) -> HashMap<String, BTreeSet<String>> {
    pairs
        .iter()
        .map(|(s, cogs)| ((*s).into(), cogs.iter().map(|c| (*c).to_string()).collect()))
        .collect()
}

fn pins(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(c, s)| ((*c).into(), (*s).into()))
        .collect()
}

// ---- parse_ref --------------------------------------------------------------------------

#[test]
fn parse_ref_variants() {
    assert_eq!(
        resolve::parse_ref("doom").unwrap(),
        CogRef::Bare {
            cog_id: "doom".into()
        }
    );
    assert_eq!(
        resolve::parse_ref("acme/doom").unwrap(),
        CogRef::Namespaced {
            store_id: "acme".into(),
            cog_id: "doom".into()
        }
    );
    assert!(resolve::parse_ref("a/b/c").is_err()); // too many slashes
    assert!(resolve::parse_ref("").is_err());
    assert!(resolve::parse_ref("/doom").is_err()); // empty store id
    assert!(resolve::parse_ref("acme/").is_err()); // empty cog id
    assert!(resolve::parse_ref("ac me/doom").is_err()); // whitespace
}

// ---- Resolver::new validation -----------------------------------------------------------

#[test]
fn new_rejects_bad_inputs() {
    assert!(Resolver::new(
        vec![store("a", 0, true), store("a", 1, true)],
        HashMap::new(),
        HashMap::new()
    )
    .is_err());
    assert!(Resolver::new(
        vec![store("a", 0, true)],
        offerings(&[("ghost", &["x"])]),
        HashMap::new()
    )
    .is_err());
    assert!(Resolver::new(
        vec![store("a", 0, true)],
        HashMap::new(),
        pins(&[("x", "ghost")])
    )
    .is_err());
    assert!(Resolver::new(
        vec![store("a", 0, true)],
        offerings(&[("a", &["x"])]),
        pins(&[("x", "a")])
    )
    .is_ok());
}

// ---- namespaced -------------------------------------------------------------------------

#[test]
fn namespaced_resolution() {
    let r = Resolver::new(
        vec![
            store("official", 0, true),
            store("acme", 10, true),
            store("off", 5, false),
        ],
        offerings(&[
            ("official", &["doom"]),
            ("acme", &["doom"]),
            ("off", &["doom"]),
        ]),
        HashMap::new(),
    )
    .unwrap();

    let res = r.resolve("acme/doom").unwrap();
    assert_eq!(
        (res.store_id.as_str(), res.cog_id.as_str(), res.reason),
        ("acme", "doom", ResolveReason::Explicit)
    );

    assert_eq!(
        r.resolve("nope/doom"),
        Err(ResolveError::UnknownStore("nope".into()))
    );
    assert_eq!(
        r.resolve("off/doom"),
        Err(ResolveError::StoreDisabled("off".into()))
    );
    assert_eq!(
        r.resolve("official/ghost"),
        Err(ResolveError::NotOffered {
            store_id: "official".into(),
            cog_id: "ghost".into()
        })
    );
}

// ---- bare: priority + tie-break ---------------------------------------------------------

#[test]
fn bare_resolves_by_priority_then_id() {
    let r = Resolver::new(
        vec![
            store("official", 0, true),
            store("acme", 10, true),
            store("beta", 0, true),
        ],
        offerings(&[
            ("official", &["doom"]),
            ("acme", &["doom"]),
            ("beta", &["doom"]),
        ]),
        HashMap::new(),
    )
    .unwrap();

    // official (prio 0) and beta (prio 0) tie -> id breaks it: "beta" < "official"
    let res = r.resolve("doom").unwrap();
    assert_eq!(
        (res.store_id.as_str(), res.reason),
        ("beta", ResolveReason::Priority)
    );

    let cands: Vec<&str> = r.candidates("doom").iter().map(|s| s.id.as_str()).collect();
    assert_eq!(cands, ["beta", "official", "acme"]); // (0,beta),(0,official),(10,acme)
}

#[test]
fn bare_skips_disabled_and_non_offering() {
    let r = Resolver::new(
        vec![
            store("a", 0, false),
            store("b", 5, true),
            store("c", 1, true),
        ],
        offerings(&[("a", &["doom"]), ("b", &["doom"]), ("c", &["other"])]),
        HashMap::new(),
    )
    .unwrap();
    // a disabled, c doesn't offer doom -> b wins despite higher priority number
    assert_eq!(r.resolve("doom").unwrap().store_id, "b");
    assert_eq!(
        r.resolve("missing"),
        Err(ResolveError::NotFound("missing".into()))
    );
}

// ---- pins -------------------------------------------------------------------------------

#[test]
fn pin_overrides_priority() {
    let r = Resolver::new(
        vec![store("official", 0, true), store("acme", 10, true)],
        offerings(&[("official", &["doom"]), ("acme", &["doom"])]),
        pins(&[("doom", "acme")]),
    )
    .unwrap();
    let res = r.resolve("doom").unwrap();
    assert_eq!(
        (res.store_id.as_str(), res.reason),
        ("acme", ResolveReason::Pinned)
    );
    // explicit namespacing still works regardless of the pin
    assert_eq!(r.resolve("official/doom").unwrap().store_id, "official");
}

#[test]
fn pin_to_unusable_store_errors() {
    let disabled = Resolver::new(
        vec![store("acme", 0, false)],
        offerings(&[("acme", &["doom"])]),
        pins(&[("doom", "acme")]),
    )
    .unwrap();
    assert_eq!(
        disabled.resolve("doom"),
        Err(ResolveError::StoreDisabled("acme".into()))
    );

    let not_offered = Resolver::new(
        vec![store("acme", 0, true)],
        offerings(&[("acme", &["other"])]),
        pins(&[("doom", "acme")]),
    )
    .unwrap();
    assert_eq!(
        not_offered.resolve("doom"),
        Err(ResolveError::NotOffered {
            store_id: "acme".into(),
            cog_id: "doom".into()
        })
    );
}

// ---- collisions + cog_ids ---------------------------------------------------------------

#[test]
fn collisions_lists_shared_cogs_among_enabled_stores() {
    let r = Resolver::new(
        vec![
            store("a", 0, true),
            store("b", 1, true),
            store("c", 2, false),
        ],
        offerings(&[("a", &["doom", "solo"]), ("b", &["doom"]), ("c", &["doom"])]),
        HashMap::new(),
    )
    .unwrap();
    // c is disabled, so doom collides only across a + b; solo is unique
    assert_eq!(
        r.collisions(),
        vec![("doom".to_string(), vec!["a".to_string(), "b".to_string()])]
    );
}

#[test]
fn cog_ids_extracts_from_catalog() {
    let catalog = serde_json::json!({
        "schema_version": 1, "store_id": "s", "generated_at": "t",
        "cogs": [ { "id": "doom", "versions": [] }, { "id": "adversarial", "versions": [] } ]
    });
    assert_eq!(
        gearbox::catalog::cog_ids(&catalog),
        vec!["doom", "adversarial"]
    );
}
