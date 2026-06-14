//! Catalog generation tests: build from the shared `tools/testdata` fixtures, validate,
//! sign, and verify. (Cross-implementation parity with the Python generator — same inputs
//! produce the same signature — is checked by `tools/`-vs-crate in the build pipeline.)

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine};

use gearbox::{catalog, signing};

const TEST_KEY_ID: &str = "gearbox-testvector-2026";
const TEST_PUBKEY_B64: &str = "A6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=";

fn testdata() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/testdata")
}

fn test_seed() -> [u8; 32] {
    std::array::from_fn(|i| i as u8) // 00 01 02 ... 1f — the published throwaway seed
}

#[test]
fn builds_validates_signs_verifies() {
    let cat = catalog::build_catalog(
        &testdata().join("cogs"),
        Some(&testdata().join("artifacts")),
        "cognitum-official",
        "2026-06-10T00:00:00Z",
        false,
    )
    .unwrap();
    catalog::validate(&cat).unwrap();

    let cogs = cat["cogs"].as_array().unwrap();
    let ids: Vec<&str> = cogs.iter().map(|c| c["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["adversarial", "doom"]); // sorted by directory

    let doom = &cogs[1];
    let arts = &doom["versions"][0]["artifacts"];
    assert!(
        arts["binary"]["sha256"].is_string(),
        "binary hashed in full mode"
    );
    assert_eq!(
        arts["assets"][0]["filename"], "freedoom1.wad",
        "asset carries filename"
    );

    let signed = signing::sign_catalog(&cat, &test_seed(), TEST_KEY_ID).unwrap();
    let pk: [u8; 32] = STANDARD
        .decode(TEST_PUBKEY_B64)
        .unwrap()
        .try_into()
        .unwrap();
    let trust = HashMap::from([(TEST_KEY_ID.to_string(), pk)]);
    assert_eq!(
        signing::verify_catalog(&signed, &trust).unwrap(),
        TEST_KEY_ID
    );
}

#[test]
fn manifests_only_has_pending_binary() {
    let cat = catalog::build_catalog(
        &testdata().join("cogs"),
        None,
        "cognitum-official",
        "2026-06-10T00:00:00Z",
        true,
    )
    .unwrap();
    catalog::validate(&cat).unwrap();

    let doom = cat["cogs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "doom")
        .unwrap();
    let bin = &doom["versions"][0]["artifacts"]["binary"];
    assert_eq!(bin["pending"], true);
    assert!(bin.get("sha256").is_none(), "pending binary has no hash");
}
