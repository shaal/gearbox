//! Air-gap bundle conformance (Phase 3, T0-A). The Rust producer/consumer must reproduce the
//! frozen bundle vector (docs/protocol/testvectors/bundle/) byte-for-byte and reject any
//! tampered byte — the same contract the Python oracle (`tools/cogstore/bundle.py`) holds.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use gearbox::{bundle, catalog, jcs, signing, store};
use sha2::{Digest, Sha256};

const TEST_SEED_HEX: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";
const TEST_KEY_ID: &str = "gearbox-testvector-2026";
const TEST_GENERATED_AT: &str = "2026-06-10T00:00:00Z";

static N: AtomicUsize = AtomicUsize::new(0);

fn tmpdir() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "gbbundle-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn tvdir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/protocol/testvectors/bundle")
}

fn seed() -> [u8; 32] {
    hex::decode(TEST_SEED_HEX).unwrap().try_into().unwrap()
}

/// Copy the frozen vector into a fresh dir as a *runnable* bundle: the signed manifest is named
/// `manifest.signed.json` in the vector (matching the `*.signed.json` convention) but a real
/// bundle's manifest is `manifest.json`. The `*.canonical.json` helper file is dropped.
fn materialize() -> PathBuf {
    let src = tvdir();
    let dst = tmpdir();
    copy_tree(&src, &dst);
    std::fs::rename(dst.join("manifest.signed.json"), dst.join("manifest.json")).unwrap();
    std::fs::remove_file(dst.join("manifest.canonical.json")).unwrap();
    dst
}

fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let to = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), to).unwrap();
        }
    }
}

#[test]
fn build_manifest_reproduces_frozen_canonical_bytes() {
    let b = materialize();
    let manifest = bundle::build_manifest(&b, TEST_GENERATED_AT).unwrap();
    let canon = jcs::canonical(&manifest).unwrap();
    let expected = std::fs::read(tvdir().join("manifest.canonical.json")).unwrap();
    assert_eq!(
        canon, expected,
        "Rust bundle manifest differs from the frozen vector"
    );
}

#[test]
fn signing_reproduces_frozen_signature() {
    let b = materialize();
    let manifest = bundle::build_manifest(&b, TEST_GENERATED_AT).unwrap();
    let signed = signing::sign_document(&manifest, &seed(), TEST_KEY_ID).unwrap();
    let frozen: serde_json::Value =
        serde_json::from_slice(&std::fs::read(tvdir().join("manifest.signed.json")).unwrap())
            .unwrap();
    assert_eq!(
        signed["signature"]["sig"], frozen["signature"]["sig"],
        "Rust bundle signature differs from the frozen vector"
    );
}

#[test]
fn verifies_frozen_bundle() {
    let report = bundle::verify_bundle(&materialize(), None).unwrap();
    assert_eq!(report.store_id, "gearbox-bundle-testvector");
    assert_eq!(report.key_id, TEST_KEY_ID);
    assert_eq!(report.n_cogs, 1);
    assert_eq!(report.n_artifacts, 1);
}

#[test]
fn pinned_fingerprint_matches_and_wrong_one_is_rejected() {
    let b = materialize();
    let fp = "56475aa75463474c0285df5dbf2bcab73da651358839e9b77481b2eab107708c";
    assert!(bundle::verify_bundle(&b, Some(fp)).is_ok());
    assert!(bundle::verify_bundle(&b, Some(&"0".repeat(64))).is_err());
}

#[test]
fn unsigned_store_still_imports_via_tofu() {
    // A store.json self-signature is integrity, not authority (protocol §8): with it removed,
    // import still trusts the store's listed keys (TOFU) and the catalog + manifest still verify.
    let b = materialize();
    let mut store_doc: serde_json::Value =
        serde_json::from_slice(&std::fs::read(b.join("store.json")).unwrap()).unwrap();
    store_doc.as_object_mut().unwrap().remove("signature");
    // Rewrite store.json *and* re-hash it in the manifest, then re-sign the manifest so the file
    // list stays internally consistent (only the store self-signature is being dropped).
    let store_bytes = serde_json::to_string_pretty(&store_doc).unwrap() + "\n";
    std::fs::write(b.join("store.json"), &store_bytes).unwrap();
    let manifest = bundle::build_manifest(&b, TEST_GENERATED_AT).unwrap();
    let signed = signing::sign_document(&manifest, &seed(), TEST_KEY_ID).unwrap();
    std::fs::write(
        b.join("manifest.json"),
        serde_json::to_string_pretty(&signed).unwrap() + "\n",
    )
    .unwrap();

    let report = bundle::verify_bundle(&b, None).unwrap();
    assert_eq!(report.key_id, TEST_KEY_ID);
}

#[test]
fn tampered_artifact_byte_fails_import() {
    let b = materialize();
    let art = b.join("artifacts/cogs/arm/cog-adversarial-arm");
    let mut bytes = std::fs::read(&art).unwrap();
    bytes.push(b'X'); // a single appended byte
    std::fs::write(&art, &bytes).unwrap();
    let err = bundle::verify_bundle(&b, None).unwrap_err();
    assert!(err.contains("sha256 mismatch"), "got: {err}");
}

#[test]
fn tampered_manifest_hash_breaks_signature() {
    let b = materialize();
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(b.join("manifest.json")).unwrap()).unwrap();
    // Rewrite a recorded hash to match a (would-be) swapped artifact: the signature no longer
    // covers it, so verification fails at the manifest signature, not the file hash.
    manifest["files"][1]["sha256"] = serde_json::json!("0".repeat(64));
    std::fs::write(
        b.join("manifest.json"),
        serde_json::to_string(&manifest).unwrap(),
    )
    .unwrap();
    assert!(bundle::verify_bundle(&b, None).is_err());
}

#[test]
fn export_roundtrips_and_matches_frozen_manifest() {
    // Export a fresh bundle from the vector's own store.json + app-registry.json + artifacts,
    // then verify it imports — the full produce→consume loop on independently-laid-out bytes.
    let src = materialize();
    let out = tmpdir().join("bundle");
    let sign = bundle::SignOpts {
        seed: seed(),
        key_id: TEST_KEY_ID,
    };
    let report = bundle::export(
        &src.join("app-registry.json"),
        &src.join("store.json"),
        &src.join("artifacts"),
        &out,
        TEST_GENERATED_AT,
        Some(&sign),
    )
    .unwrap();
    assert!(report.signed);
    assert_eq!(report.n_artifacts, 1);

    // The freshly exported manifest must equal the frozen vector byte-for-byte (canonical).
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("manifest.json")).unwrap()).unwrap();
    let mut body = manifest.clone();
    body.as_object_mut().unwrap().remove("signature");
    let canon = jcs::canonical(&body).unwrap();
    let expected = std::fs::read(tvdir().join("manifest.canonical.json")).unwrap();
    assert_eq!(
        canon, expected,
        "exported manifest diverged from the vector"
    );

    bundle::verify_bundle(&out, None).unwrap();
}

/// A bundle with **two cogs**, one carrying a **downloadable asset**, built from scratch via the
/// library so the asset path and multi-artifact enumeration are exercised end to end — not just
/// the binary-only frozen vector. Then a single flipped *asset* byte must fail import.
#[test]
fn export_import_with_asset_and_multiple_cogs() {
    let work = tmpdir();
    let cogs = work.join("cogs");
    let stage = work.join("stage");
    std::fs::create_dir_all(cogs.join("alpha")).unwrap();
    std::fs::create_dir_all(cogs.join("beta")).unwrap();
    std::fs::create_dir_all(stage.join("cogs/arm/data")).unwrap();

    // Stage binaries + one asset; the asset's catalog sha256/size come from its cog.toml, so we
    // compute them from the bytes we write and embed them (the generator's source-of-truth model).
    std::fs::write(stage.join("cogs/arm/cog-alpha-arm"), b"alpha binary\n").unwrap();
    std::fs::write(stage.join("cogs/arm/cog-beta-arm"), b"beta binary\n").unwrap();
    let asset_bytes = b"beta calibration blob\n";
    std::fs::write(stage.join("cogs/arm/data/blob.bin"), asset_bytes).unwrap();
    let asset_sha = hex::encode(Sha256::digest(asset_bytes));
    let asset_size = asset_bytes.len();

    std::fs::write(
        cogs.join("alpha/cog.toml"),
        "[cog]\nid = \"alpha\"\nname = \"Alpha\"\nversion = \"1.0.0\"\ncategory = \"demo\"\n\
         binary = \"cog-alpha-arm\"\nhardware_requirement = \"pi-zero-2w\"\n",
    )
    .unwrap();
    std::fs::write(
        cogs.join("beta/cog.toml"),
        format!(
            "[cog]\nid = \"beta\"\nname = \"Beta\"\nversion = \"1.0.0\"\ncategory = \"demo\"\n\
             binary = \"cog-beta-arm\"\nhardware_requirement = \"pi-zero-2w\"\n\n\
             [[assets]]\nid = \"blob\"\nfilename = \"blob.bin\"\nsize_bytes = {asset_size}\n\
             sha256 = \"{asset_sha}\"\ngcs_path = \"data/blob.bin\"\n"
        ),
    )
    .unwrap();

    let seed = seed();
    let cat = catalog::build_catalog(
        &cogs,
        Some(&stage),
        "acme-internal",
        TEST_GENERATED_AT,
        false,
    )
    .unwrap();
    let signed_cat = signing::sign_catalog(&cat, &seed, TEST_KEY_ID).unwrap();
    std::fs::write(
        work.join("app-registry.json"),
        serde_json::to_string_pretty(&signed_cat).unwrap() + "\n",
    )
    .unwrap();

    let store_doc = store::build_store_info(
        "acme-internal",
        "ACME",
        "demo",
        "file://./app-registry.json",
        TEST_KEY_ID,
        &signing::public_key_b64(&seed),
    )
    .unwrap();
    let signed_store = signing::sign_document(&store_doc, &seed, TEST_KEY_ID).unwrap();
    std::fs::write(
        work.join("store.json"),
        serde_json::to_string_pretty(&signed_store).unwrap() + "\n",
    )
    .unwrap();

    let out = work.join("bundle");
    let sign = bundle::SignOpts {
        seed,
        key_id: TEST_KEY_ID,
    };
    let report = bundle::export(
        &work.join("app-registry.json"),
        &work.join("store.json"),
        &stage,
        &out,
        TEST_GENERATED_AT,
        Some(&sign),
    )
    .unwrap();
    assert_eq!(report.n_artifacts, 3, "2 binaries + 1 asset");

    let v = bundle::verify_bundle(&out, None).unwrap();
    assert_eq!(v.n_cogs, 2);
    assert_eq!(v.n_artifacts, 3);

    // Flip a byte of the ASSET (not the binary) — import must still refuse it.
    let asset = out.join("artifacts/cogs/arm/data/blob.bin");
    let mut b = std::fs::read(&asset).unwrap();
    b[0] ^= 0x01;
    std::fs::write(&asset, &b).unwrap();
    assert!(bundle::verify_bundle(&out, None).is_err());
}
