//! Conformance: the Rust JCS + verifier must reproduce the frozen test vector
//! (docs/protocol/testvectors/) byte-for-byte — the cross-language gate that ties this
//! crate (and seed B4, built the same way) to the signer (Python tools / A4).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;

use gearbox::{jcs, signing};

// Published throwaway test key (matches the Gearbox vector). DO NOT use in production.
const TEST_KEY_ID: &str = "gearbox-testvector-2026";
const TEST_PUBKEY_B64: &str = "A6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=";

fn tvdir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/protocol/testvectors")
}

fn signed_catalog() -> Value {
    let bytes = std::fs::read(tvdir().join("catalog.signed.json")).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn test_trust() -> HashMap<String, [u8; 32]> {
    let pk: [u8; 32] = STANDARD.decode(TEST_PUBKEY_B64).unwrap().try_into().unwrap();
    HashMap::from([(TEST_KEY_ID.to_string(), pk)])
}

#[test]
fn jcs_reproduces_frozen_canonical_bytes() {
    let mut body = signed_catalog();
    body.as_object_mut().unwrap().remove("signature");
    let canon = jcs::canonical(&body).unwrap();
    let expected = std::fs::read(tvdir().join("catalog.canonical.json")).unwrap();
    assert_eq!(
        canon, expected,
        "Rust JCS output differs from the frozen test vector"
    );
}

#[test]
fn verifies_frozen_vector() {
    let kid = signing::verify_catalog(&signed_catalog(), &test_trust()).unwrap();
    assert_eq!(kid, TEST_KEY_ID);
}

#[test]
fn tamper_is_rejected() {
    let mut c = signed_catalog();
    c["store_id"] = serde_json::json!("evil"); // mutate a signed field
    assert!(signing::verify_catalog(&c, &test_trust()).is_err());
}

#[test]
fn untrusted_key_is_rejected() {
    let empty: HashMap<String, [u8; 32]> = HashMap::new();
    assert!(signing::verify_catalog(&signed_catalog(), &empty).is_err());
}

#[test]
fn wrong_alg_is_rejected() {
    let mut c = signed_catalog();
    c["signature"]["alg"] = serde_json::json!("rsa");
    assert!(signing::verify_catalog(&c, &test_trust()).is_err());
}
