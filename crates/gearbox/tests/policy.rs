//! Managed-mode policy conformance (Phase 3, T0-C). The Rust producer/consumer reproduces the
//! frozen policy vector (docs/protocol/testvectors/policy/) byte-for-byte, the projection
//! restricts resolution as specified, and a forged policy is rejected fail-closed — the same
//! signing contract the Python oracle (`tools/cogstore/policy.py`) holds.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;

use gearbox::resolve::{ResolveError, Resolver, StoreRef};
use gearbox::{jcs, policy, signing};

const TEST_KEY_ID: &str = "gearbox-testvector-2026";
const TEST_PUBKEY_B64: &str = "A6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=";

fn tvdir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/protocol/testvectors/policy")
}

fn seed() -> [u8; 32] {
    std::array::from_fn(|i| i as u8) // 00 01 02 … 1f — the published throwaway seed
}

fn trust() -> HashMap<String, [u8; 32]> {
    let pk: [u8; 32] = STANDARD
        .decode(TEST_PUBKEY_B64)
        .unwrap()
        .try_into()
        .unwrap();
    HashMap::from([(TEST_KEY_ID.to_string(), pk)])
}

fn signed_policy() -> Value {
    serde_json::from_slice(&std::fs::read(tvdir().join("policy.signed.json")).unwrap()).unwrap()
}

/// Two stores — a private ACME store and the built-in public store — both offering `doom`.
fn device_stores() -> policy::DeviceStores {
    let stores = vec![
        StoreRef {
            id: "acme-internal".into(),
            priority: 10,
            enabled: true,
        },
        StoreRef {
            id: "cognitum-official".into(),
            priority: 50,
            enabled: true,
        },
    ];
    let offerings = HashMap::from([
        (
            "acme-internal".to_string(),
            BTreeSet::from(["doom".to_string()]),
        ),
        (
            "cognitum-official".to_string(),
            BTreeSet::from(["doom".to_string()]),
        ),
    ]);
    (stores, offerings, HashMap::new())
}

#[test]
fn build_policy_reproduces_frozen_canonical_bytes() {
    let forced = policy::parse_forced_pins(&["doom=acme-internal".to_string()]).unwrap();
    let doc = policy::build_policy(&["acme-internal".to_string()], true, &forced, false);
    let canon = jcs::canonical(&doc).unwrap();
    let expected = std::fs::read(tvdir().join("policy.canonical.json")).unwrap();
    assert_eq!(
        canon, expected,
        "Rust policy differs from the frozen vector"
    );
}

#[test]
fn signing_reproduces_frozen_signature() {
    let forced = policy::parse_forced_pins(&["doom=acme-internal".to_string()]).unwrap();
    let doc = policy::build_policy(&["acme-internal".to_string()], true, &forced, false);
    let signed = signing::sign_document(&doc, &seed(), TEST_KEY_ID).unwrap();
    assert_eq!(
        signed["signature"]["sig"],
        signed_policy()["signature"]["sig"],
        "Rust policy signature differs from the frozen vector"
    );
}

#[test]
fn frozen_policy_verifies_and_parses() {
    let kid = policy::verify_signed(&signed_policy(), &trust()).unwrap();
    assert_eq!(kid, TEST_KEY_ID);
    let p = policy::Policy::from_json(&signed_policy()).unwrap();
    assert!(p.managed);
    assert_eq!(p.allow_stores, vec!["acme-internal".to_string()]);
    assert!(p.deny_public);
    assert_eq!(p.forced_pins.get("doom").unwrap(), "acme-internal");
    assert!(!p.allow_user_add_store);
}

#[test]
fn forged_policy_is_rejected_fail_closed() {
    let mut p = signed_policy();
    p["allow_stores"] = serde_json::json!(["evil"]); // tamper a signed field
    assert!(policy::verify_signed(&p, &trust()).is_err());

    let mut unsigned = signed_policy();
    unsigned.as_object_mut().unwrap().remove("signature"); // strip the signature entirely
    assert!(policy::verify_signed(&unsigned, &trust()).is_err());

    // Right document, wrong (untrusted) key.
    let empty: HashMap<String, [u8; 32]> = HashMap::new();
    assert!(policy::verify_signed(&signed_policy(), &empty).is_err());
}

#[test]
fn projection_resolves_to_acme_and_denies_public() {
    let p = policy::Policy::from_json(&signed_policy()).unwrap();
    let (stores, offerings, pins) = device_stores();
    let (stores, pins) = p.project(stores, pins);
    let resolver = Resolver::new(stores, offerings, pins).unwrap();

    // bare `doom` -> ACME, because the forced pin (and the allowlist) point there.
    let res = resolver.resolve("doom").unwrap();
    assert_eq!(res.store_id, "acme-internal");

    // the explicitly-named public store is force-disabled -> the existing typed error.
    assert_eq!(
        resolver.resolve("cognitum-official/doom"),
        Err(ResolveError::StoreDisabled("cognitum-official".into()))
    );
}

#[test]
fn allowlist_disables_stores_not_listed() {
    let p = policy::Policy::from_json(&signed_policy()).unwrap();
    // A third store, not in allow_stores, must be force-disabled by the projection.
    let mut stores = device_stores().0;
    stores.push(StoreRef {
        id: "rogue".into(),
        priority: 1,
        enabled: true,
    });
    let (projected, _) = p.project(stores, HashMap::new());
    let rogue = projected.iter().find(|s| s.id == "rogue").unwrap();
    assert!(
        !rogue.enabled,
        "a store not in allow_stores must be disabled"
    );
    let official = projected
        .iter()
        .find(|s| s.id == "cognitum-official")
        .unwrap();
    assert!(
        !official.enabled,
        "deny_public must disable the official store"
    );
    let acme = projected.iter().find(|s| s.id == "acme-internal").unwrap();
    assert!(acme.enabled, "an allow-listed store stays enabled");
}

#[test]
fn deny_public_works_independently_of_the_allowlist() {
    // A policy with an EMPTY allow_stores but deny_public must still disable the official store
    // (and leave others enabled) — deny_public is a separate lever, not a consequence of the list.
    let doc = policy::build_policy(&[], true, &serde_json::Map::new(), false);
    let p = policy::Policy::from_json(&doc).unwrap();
    let (projected, _) = p.project(device_stores().0, HashMap::new());
    assert!(
        !projected
            .iter()
            .find(|s| s.id == "cognitum-official")
            .unwrap()
            .enabled,
        "deny_public disables the official store even with an empty allowlist"
    );
    assert!(
        projected
            .iter()
            .find(|s| s.id == "acme-internal")
            .unwrap()
            .enabled,
        "with no allowlist and deny_public, non-public stores stay enabled"
    );
}

#[test]
fn bare_ref_to_a_denied_only_store_is_not_found() {
    // `tetris` is offered ONLY by the public store; under deny_public a bare resolve finds no
    // enabled store -> NotFound (the un-namespaced half of "resolving a public store is denied").
    // (acme-internal is present so the policy's forced `doom` pin still references a known store.)
    let p = policy::Policy::from_json(&signed_policy()).unwrap();
    let stores = vec![
        StoreRef {
            id: "acme-internal".into(),
            priority: 10,
            enabled: true,
        },
        StoreRef {
            id: "cognitum-official".into(),
            priority: 50,
            enabled: true,
        },
    ];
    let offerings = HashMap::from([
        (
            "acme-internal".to_string(),
            BTreeSet::from(["doom".to_string()]),
        ),
        (
            "cognitum-official".to_string(),
            BTreeSet::from(["tetris".to_string()]),
        ),
    ]);
    let (stores, pins) = p.project(stores, HashMap::new());
    let resolver = Resolver::new(stores, offerings, pins).unwrap();
    assert_eq!(
        resolver.resolve("tetris"),
        Err(ResolveError::NotFound("tetris".into()))
    );
}

#[test]
fn forced_pin_overrides_a_user_pin() {
    let p = policy::Policy::from_json(&signed_policy()).unwrap();
    let user_pins = HashMap::from([("doom".to_string(), "cognitum-official".to_string())]);
    let (_, pins) = p.project(device_stores().0, user_pins);
    assert_eq!(
        pins.get("doom").unwrap(),
        "acme-internal",
        "the admin forced pin must win over the user pin"
    );
}

#[test]
fn parse_forced_pins_guards_empty_sides() {
    assert!(policy::parse_forced_pins(&["doom=acme".to_string()]).is_ok());
    assert!(policy::parse_forced_pins(&["noeq".to_string()]).is_err());
    assert!(policy::parse_forced_pins(&["=acme".to_string()]).is_err());
    assert!(policy::parse_forced_pins(&["doom=".to_string()]).is_err());
}

#[test]
fn parse_device_stores_reads_stores_offerings_and_pins() {
    let doc = serde_json::json!({
        "stores": [
            { "id": "acme-internal", "priority": 10, "enabled": true, "cogs": ["doom", "tetris"] },
            { "id": "cognitum-official", "priority": 50, "cogs": ["doom"] }
        ],
        "pins": { "tetris": "acme-internal" }
    });
    let (stores, offerings, pins) = policy::parse_device_stores(&doc).unwrap();
    assert_eq!(stores.len(), 2);
    assert!(
        stores
            .iter()
            .find(|s| s.id == "cognitum-official")
            .unwrap()
            .enabled
    ); // default true
    assert!(offerings["acme-internal"].contains("tetris"));
    assert_eq!(pins.get("tetris").unwrap(), "acme-internal");
}
