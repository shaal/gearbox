//! Attestation conformance (T1-shaped). The Rust producer/consumer reproduces the frozen vector
//! (docs/protocol/testvectors/attestation/) byte-for-byte, the digest binding catches a swapped
//! artifact, and a tampered field breaks the signature — the same signing contract the Python
//! oracle (`tools/cogstore/attest.py`) holds.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;

use gearbox::attest::{self, Package, Provenance, Subject};
use gearbox::{jcs, signing};

const TEST_KEY_ID: &str = "gearbox-testvector-2026";
const TEST_PUBKEY_B64: &str = "A6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=";
const DOOM_SHA: &str = "238a6e038d11d2b9851396b8ec167ad2f5c8724525100473c2a3f06c9ea43561";
const FREEDOOM_SHA: &str = "7323bcc168c5a45ff10749b339960e98314740a734c30d4b9f3337001f9e703d";

fn tvdir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/protocol/testvectors/attestation")
}

fn artifacts() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/testdata/artifacts/cogs/arm")
}

fn seed() -> [u8; 32] {
    std::array::from_fn(|i| i as u8)
}

fn trust() -> HashMap<String, [u8; 32]> {
    let pk: [u8; 32] = STANDARD
        .decode(TEST_PUBKEY_B64)
        .unwrap()
        .try_into()
        .unwrap();
    HashMap::from([(TEST_KEY_ID.to_string(), pk)])
}

fn signed_attestation() -> Value {
    serde_json::from_slice(&std::fs::read(tvdir().join("attestation.signed.json")).unwrap())
        .unwrap()
}

/// Rebuild the vector's attestation from its source fields.
fn rebuild() -> Value {
    attest::build_attestation(
        &Subject {
            cog: "doom",
            version: "0.1.0",
            artifact: "cogs/arm/cog-doom-arm",
            sha256: DOOM_SHA,
        },
        &Provenance {
            builder: "cogs-ci",
            source_repo: "github.com/cognitum-one/cogs",
            source_commit: "abc1234def5678",
            built_at: "2026-06-10T00:00:00Z",
        },
        &[Package {
            name: "freedoom".into(),
            version: "0.13.0".into(),
            license: "BSD-3-Clause".into(),
            sha256: FREEDOOM_SHA.into(),
        }],
    )
}

#[test]
fn build_reproduces_frozen_canonical_bytes() {
    let canon = jcs::canonical(&rebuild()).unwrap();
    let expected = std::fs::read(tvdir().join("attestation.canonical.json")).unwrap();
    assert_eq!(
        canon, expected,
        "Rust attestation differs from the frozen vector"
    );
}

#[test]
fn signing_reproduces_frozen_signature() {
    let signed = signing::sign_document(&rebuild(), &seed(), TEST_KEY_ID).unwrap();
    assert_eq!(
        signed["signature"]["sig"],
        signed_attestation()["signature"]["sig"],
        "Rust attestation signature differs from the frozen vector"
    );
}

#[test]
fn frozen_attestation_verifies() {
    let kid = attest::verify(&signed_attestation(), &trust()).unwrap();
    assert_eq!(kid, TEST_KEY_ID);
}

#[test]
fn artifact_binding_matches_and_rejects_a_swap() {
    let doc = signed_attestation();
    let doom = std::fs::read(artifacts().join("cog-doom-arm")).unwrap();
    attest::check_artifact(&doc, &doom).unwrap(); // the right artifact binds

    let other = std::fs::read(artifacts().join("cog-adversarial-arm")).unwrap();
    assert!(
        attest::check_artifact(&doc, &other).is_err(),
        "a different artifact must not satisfy the binding"
    );
}

#[test]
fn tampered_field_breaks_the_signature() {
    // The whole document is signed, so editing the SBOM or the subject digest fails verification.
    let mut a = signed_attestation();
    a["sbom"]["packages"][0]["version"] = serde_json::json!("9.9.9");
    assert!(attest::verify(&a, &trust()).is_err());

    let mut b = signed_attestation();
    b["subject"]["sha256"] = serde_json::json!("0".repeat(64));
    assert!(attest::verify(&b, &trust()).is_err());

    // Untrusted key is rejected too.
    let empty: HashMap<String, [u8; 32]> = HashMap::new();
    assert!(attest::verify(&signed_attestation(), &empty).is_err());
}

#[test]
fn validate_rejects_a_short_subject_digest() {
    let mut a = signed_attestation();
    a.as_object_mut().unwrap().remove("signature");
    a["subject"]["sha256"] = serde_json::json!("abc"); // not 64 hex
    assert!(attest::validate(&a).is_err());
}

#[test]
fn empty_sbom_is_valid_and_signs() {
    // An artifact with no declared dependencies (e.g. a self-contained binary) still produces a
    // valid, signable attestation with an empty package list.
    let doc = attest::build_attestation(
        &Subject {
            cog: "adversarial",
            version: "1.0.0",
            artifact: "cogs/arm/cog-adversarial-arm",
            sha256: &"d".repeat(64),
        },
        &Provenance {
            builder: "cogs-ci",
            source_repo: "github.com/cognitum-one/cogs",
            source_commit: "deadbeef",
            built_at: "2026-06-10T00:00:00Z",
        },
        &[],
    );
    attest::validate(&doc).unwrap();
    assert_eq!(doc["sbom"]["packages"].as_array().unwrap().len(), 0);
    let signed = signing::sign_document(&doc, &seed(), TEST_KEY_ID).unwrap();
    assert_eq!(attest::verify(&signed, &trust()).unwrap(), TEST_KEY_ID);
}

#[test]
fn parse_packages_requires_four_valid_fields() {
    let ok =
        attest::parse_packages(&[format!("freedoom=0.13.0=BSD-3-Clause={FREEDOOM_SHA}")]).unwrap();
    assert_eq!(ok.len(), 1);
    assert_eq!(ok[0].name, "freedoom");

    assert!(attest::parse_packages(&["a=b=c".to_string()]).is_err()); // only 3 fields
    assert!(attest::parse_packages(&["a=b==d".to_string()]).is_err()); // empty license
    assert!(attest::parse_packages(&["n=v=l=notahash".to_string()]).is_err()); // bad sha256
}
