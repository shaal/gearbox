//! Store-info (`store.json`) tests: vector conformance (Rust JCS == committed canonical),
//! self-signature verification, fingerprints, a build→sign→verify roundtrip (with non-ASCII),
//! and tamper/bad-key rejection.

use std::path::{Path, PathBuf};

use serde_json::Value;

use gearbox::{jcs, signing, store};

const TEST_KEY_ID: &str = "gearbox-testvector-2026";
const EXPECTED_FINGERPRINT: &str =
    "56475aa75463474c0285df5dbf2bcab73da651358839e9b77481b2eab107708c";

fn tvdir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/protocol/testvectors")
}

fn test_seed() -> [u8; 32] {
    std::array::from_fn(|i| i as u8)
}

fn signed_store() -> Value {
    serde_json::from_slice(&std::fs::read(tvdir().join("store.signed.json")).unwrap()).unwrap()
}

#[test]
fn store_jcs_reproduces_frozen_canonical() {
    let mut body = signed_store();
    body.as_object_mut().unwrap().remove("signature");
    let canon = jcs::canonical(&body).unwrap();
    let expected = std::fs::read(tvdir().join("store.canonical.json")).unwrap();
    assert_eq!(
        canon, expected,
        "Rust JCS of store.json differs from the committed vector"
    );
}

#[test]
fn store_self_signature_verifies() {
    assert_eq!(
        store::verify_self_signed(&signed_store()).unwrap(),
        TEST_KEY_ID
    );
}

#[test]
fn store_fingerprint_matches() {
    let fps = store::fingerprints(&signed_store()).unwrap();
    assert_eq!(
        fps,
        vec![(TEST_KEY_ID.to_string(), EXPECTED_FINGERPRINT.to_string())]
    );
}

#[test]
fn build_sign_validate_roundtrip_with_non_ascii() {
    let pubkey = signing::public_key_b64(&test_seed());
    let doc = store::build_store_info(
        "acme",
        "ACME — Internal",                           // non-ASCII name
        "Internes Cog-Verzeichnis für ACME-Geräte.", // non-ASCII description
        "https://cogs.acme.internal/app-registry.json",
        "acme-2026",
        &pubkey,
    )
    .unwrap();
    let signed = signing::sign_document(&doc, &test_seed(), "acme-2026").unwrap();
    store::validate(&signed).unwrap();
    assert_eq!(store::verify_self_signed(&signed).unwrap(), "acme-2026");
}

#[test]
fn tampered_store_rejected() {
    let mut s = signed_store();
    s["catalog_url"] = serde_json::json!("https://evil.example/app-registry.json");
    assert!(store::verify_self_signed(&s).is_err());
}

#[test]
fn rejects_bad_pubkey() {
    assert!(
        store::build_store_info("a", "A", "d", "https://x.example/c", "k", "not-base64!!").is_err()
    );
}
