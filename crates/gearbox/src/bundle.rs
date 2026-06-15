//! Air-gap bundle (Phase 3, T0-A) — a self-contained, signed, `file://`-installable store.
//!
//! A bundle is a directory that is a **complete, verifiable store on a filesystem**:
//!
//! ```text
//! <bundle>/
//!   store.json            store-info (self-signed; protocol §8)
//!   app-registry.json     signed catalog (protocol §3/§7)
//!   artifacts/cogs/<arch>/…  every binary + asset the catalog references
//!   manifest.json         bundle manifest: schema_version, store_id, generated_at,
//!                         catalog_sha256, and files[] (per-file sha256+size) — signed with
//!                         the SAME envelope + SAME key as the catalog (protocol §7.2)
//! ```
//!
//! Design choice (vs. the Phase-3 plan's sketch of a detached `manifest.sig`): the manifest
//! carries an **embedded** `signature` member, exactly like the catalog and store-info docs.
//! That reuses `signing::sign_document` / `verify_document` and inherits the byte-for-byte
//! JCS parity those already have under CI — "no new crypto" (plan §3/§11). One key signs both
//! the catalog and the manifest, so `import` has a single trust anchor and nothing is trusted
//! by path: every file is hashed in `manifest.json`.
//!
//! `import` runs the *identical* `signing::verify_catalog` + per-artifact `sha256` checks an
//! online fetch runs; only the byte source differs (a `file://` bundle dir vs. HTTP). An
//! air-gapped install is therefore no less trusted than an online one.

use std::path::{Component, Path, PathBuf};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::signing::TrustStore;
use crate::{catalog, signing, store};

pub const STORE_FILE: &str = "store.json";
pub const CATALOG_FILE: &str = "app-registry.json";
pub const MANIFEST_FILE: &str = "manifest.json";
pub const ARTIFACTS_DIR: &str = "artifacts";

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Join `rel` under `base`, rejecting `..`/absolute/prefix components (no traversal). A signed
/// manifest is still untrusted input until verified, so paths are guarded defensively.
fn safe_join(base: &Path, rel: &str) -> Result<PathBuf, String> {
    let mut p = base.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(c) => p.push(c),
            Component::CurDir => {}
            _ => return Err(format!("unsafe path in bundle: {rel:?}")),
        }
    }
    Ok(p)
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("read {}: {e}", path.display()))
}

fn read_json(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&read(path)?).map_err(|e| format!("parse {}: {e}", path.display()))
}

/// Build the **unsigned** bundle manifest for an on-disk bundle directory.
///
/// `files[]` is derived from the catalog (the two signed docs + every `artifact_paths` entry),
/// each hashed from the bytes actually present in `<dir>`, and sorted by path for a stable,
/// reproducible document. The result is in the JCS subset (integer numbers, ASCII keys), so it
/// signs and cross-checks byte-for-byte against the Python oracle.
pub fn build_manifest(dir: &Path, generated_at: &str) -> Result<Value, String> {
    let cat = read_json(&dir.join(CATALOG_FILE))?;
    catalog::validate(&cat)?;
    let store_doc = read_json(&dir.join(STORE_FILE))?;
    store::validate(&store_doc)?;

    let store_id = cat
        .get("store_id")
        .and_then(Value::as_str)
        .ok_or("catalog: store_id missing")?;
    let store_store_id = store_doc.get("store_id").and_then(Value::as_str);
    if store_store_id != Some(store_id) {
        return Err(format!(
            "store_id mismatch: catalog {store_id:?} vs store.json {store_store_id:?}"
        ));
    }

    // Bundle-relative paths in a deterministic, catalog-derived order, then sorted.
    let mut rel_paths = vec![CATALOG_FILE.to_string(), STORE_FILE.to_string()];
    for a in catalog::artifact_paths(&cat)? {
        rel_paths.push(format!("{ARTIFACTS_DIR}/{}", a.path));
    }
    rel_paths.sort();
    rel_paths.dedup();

    let mut files = Vec::new();
    for rel in &rel_paths {
        let bytes = read(&safe_join(dir, rel)?)?;
        files.push(json!({
            "path": rel,
            "sha256": sha256_hex(&bytes),
            "size": bytes.len() as i64,
        }));
    }

    let catalog_sha256 = sha256_hex(&read(&dir.join(CATALOG_FILE))?);
    Ok(json!({
        "schema_version": 1,
        "store_id": store_id,
        "generated_at": generated_at,
        "catalog_sha256": catalog_sha256,
        "files": files,
    }))
}

/// Optional Ed25519 signing material for `export`.
pub struct SignOpts<'a> {
    pub seed: [u8; 32],
    pub key_id: &'a str,
}

/// Summary of a produced bundle.
pub struct ExportReport {
    pub out: PathBuf,
    pub n_artifacts: usize,
    pub signed: bool,
}

/// Produce a bundle directory from an existing (signed) catalog, store-info, and a staging dir
/// of artifacts. The catalog and store-info are copied **byte-for-byte** (never re-serialized),
/// so `catalog_sha256` reflects the exact bytes a Seed will verify. Each referenced artifact is
/// copied and re-hashed against the catalog before the manifest is written, so a mis-staged
/// artifact fails at export, not at the customer's air-gapped install.
pub fn export(
    catalog_path: &Path,
    store_path: &Path,
    artifacts_dir: &Path,
    out: &Path,
    generated_at: &str,
    sign: Option<&SignOpts>,
) -> Result<ExportReport, String> {
    let catalog_bytes = read(catalog_path)?;
    let cat: Value = serde_json::from_slice(&catalog_bytes)
        .map_err(|e| format!("parse {}: {e}", catalog_path.display()))?;
    catalog::validate(&cat)?;
    let store_bytes = read(store_path)?;
    let store_doc: Value = serde_json::from_slice(&store_bytes)
        .map_err(|e| format!("parse {}: {e}", store_path.display()))?;
    store::validate(&store_doc)?;

    let artifacts = catalog::artifact_paths(&cat)?;

    // Lay the bundle out: copy the two signed docs verbatim, then every artifact.
    std::fs::create_dir_all(out).map_err(|e| format!("mkdir {}: {e}", out.display()))?;
    std::fs::write(out.join(STORE_FILE), &store_bytes)
        .map_err(|e| format!("write store.json: {e}"))?;
    std::fs::write(out.join(CATALOG_FILE), &catalog_bytes)
        .map_err(|e| format!("write app-registry.json: {e}"))?;

    for a in &artifacts {
        let src = safe_join(artifacts_dir, &a.path)?;
        let bytes = read(&src)?;
        let got = sha256_hex(&bytes);
        if got != a.sha256 {
            return Err(format!(
                "staged artifact {} sha256 {got} != catalog {} — refusing to bundle a mismatch",
                a.path, a.sha256
            ));
        }
        let dst = safe_join(&out.join(ARTIFACTS_DIR), &a.path)?;
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir {}: {e}", parent.display()))?;
        }
        std::fs::write(&dst, &bytes).map_err(|e| format!("write {}: {e}", dst.display()))?;
    }

    let manifest = build_manifest(out, generated_at)?;
    let (manifest, signed) = match sign {
        Some(s) => (signing::sign_document(&manifest, &s.seed, s.key_id)?, true),
        None => (manifest, false),
    };
    let pretty = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())? + "\n";
    std::fs::write(out.join(MANIFEST_FILE), pretty)
        .map_err(|e| format!("write manifest.json: {e}"))?;

    Ok(ExportReport {
        out: out.to_path_buf(),
        n_artifacts: artifacts.len(),
        signed,
    })
}

/// What a successful `import` verified.
#[derive(Debug, Clone)]
pub struct VerifyReport {
    pub store_id: String,
    pub key_id: String,
    pub fingerprint: String,
    pub n_cogs: usize,
    pub n_artifacts: usize,
}

/// Verify a bundle end-to-end through the same trust path as an online install.
///
/// 1. `store.json` validates; if self-signed, its self-signature verifies. Its listed keys are
///    the trust anchor (TOFU). `expect_fingerprint`, if given, asserts a *pinned* key matched —
///    the returning-device path rather than first-use.
/// 2. The catalog verifies via `signing::verify_catalog` against that trust (identical to online).
/// 3. The manifest verifies via the same envelope and is signed by the **same key** as the catalog.
/// 4. `catalog_sha256` and every `files[]` hash match the on-disk bytes (nothing trusted by path).
/// 5. Every artifact the catalog references is re-hashed with the **same per-artifact sha256
///    check** an online fetch runs — only the byte source (the `file://` bundle) differs.
///
/// Any signature failure or a single flipped artifact byte makes this return `Err`.
pub fn verify_bundle(dir: &Path, expect_fingerprint: Option<&str>) -> Result<VerifyReport, String> {
    // 1. store-info + TOFU trust anchor.
    let store_doc = read_json(&dir.join(STORE_FILE))?;
    store::validate(&store_doc)?;
    let trust: TrustStore = store::trust_from_keys(&store_doc)?;
    if store_doc.get("signature").is_some() {
        store::verify_self_signed(&store_doc)
            .map_err(|e| format!("store.json self-signature: {e}"))?;
    }
    let fingerprints = store::fingerprints(&store_doc)?;
    if let Some(want) = expect_fingerprint {
        if !fingerprints.iter().any(|(_, fp)| fp == want) {
            return Err(format!(
                "no store key matches the pinned fingerprint {want} (keys: {})",
                fingerprints
                    .iter()
                    .map(|(_, fp)| fp.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }

    // 2. catalog — the identical verify_catalog an online Seed runs.
    let cat = read_json(&dir.join(CATALOG_FILE))?;
    catalog::validate(&cat)?;
    let catalog_key =
        signing::verify_catalog(&cat, &trust).map_err(|e| format!("catalog signature: {e}"))?;

    // 3. manifest — same envelope, and required to be the SAME key as the catalog.
    let manifest = read_json(&dir.join(MANIFEST_FILE))?;
    let manifest_key = signing::verify_document(&manifest, &trust)
        .map_err(|e| format!("manifest signature: {e}"))?;
    if manifest_key != catalog_key {
        return Err(format!(
            "manifest signed by {manifest_key:?} but catalog by {catalog_key:?} — expected one trust anchor"
        ));
    }

    // 4. manifest internal consistency + every listed file's hash matches on-disk bytes.
    let catalog_sha256 = sha256_hex(&read(&dir.join(CATALOG_FILE))?);
    if manifest.get("catalog_sha256").and_then(Value::as_str) != Some(catalog_sha256.as_str()) {
        return Err("manifest catalog_sha256 does not match app-registry.json".into());
    }
    let files = manifest
        .get("files")
        .and_then(Value::as_array)
        .ok_or("manifest: files[] missing")?;
    let mut listed = std::collections::HashMap::new();
    for f in files {
        let path = f
            .get("path")
            .and_then(Value::as_str)
            .ok_or("manifest file: path missing")?;
        let want_sha = f
            .get("sha256")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("manifest file {path}: sha256 missing"))?;
        let want_size = f
            .get("size")
            .and_then(Value::as_i64)
            .ok_or_else(|| format!("manifest file {path}: size missing"))?;
        let bytes = read(&safe_join(dir, path)?)?;
        if sha256_hex(&bytes) != want_sha {
            return Err(format!("file {path}: sha256 mismatch (bundle tampered)"));
        }
        if bytes.len() as i64 != want_size {
            return Err(format!("file {path}: size mismatch (bundle tampered)"));
        }
        listed.insert(path.to_string(), want_sha.to_string());
    }
    for required in [STORE_FILE, CATALOG_FILE] {
        if !listed.contains_key(required) {
            return Err(format!("manifest does not list {required}"));
        }
    }

    // 5. per-artifact sha256 against the verified catalog — the same check as online, plus a
    //    cross-check that the signed manifest also covers each artifact.
    let artifacts = catalog::artifact_paths(&cat)?;
    for a in &artifacts {
        let rel = format!("{ARTIFACTS_DIR}/{}", a.path);
        let bytes = read(&safe_join(dir, &rel)?)?;
        if sha256_hex(&bytes) != a.sha256 {
            return Err(format!(
                "artifact {}: sha256 does not match catalog",
                a.path
            ));
        }
        match listed.get(&rel) {
            Some(m) if m == &a.sha256 => {}
            Some(_) => {
                return Err(format!(
                    "artifact {}: manifest hash != catalog hash",
                    a.path
                ))
            }
            None => {
                return Err(format!(
                    "artifact {} not covered by the signed manifest",
                    a.path
                ))
            }
        }
    }

    let store_id = cat
        .get("store_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let fingerprint = fingerprints
        .iter()
        .find(|(kid, _)| kid == &catalog_key)
        .map(|(_, fp)| fp.clone())
        .or_else(|| fingerprints.first().map(|(_, fp)| fp.clone()))
        .unwrap_or_default();
    Ok(VerifyReport {
        store_id,
        key_id: catalog_key,
        fingerprint,
        n_cogs: cat
            .get("cogs")
            .and_then(Value::as_array)
            .map_or(0, |a| a.len()),
        n_artifacts: artifacts.len(),
    })
}
