//! Build + validate an app-registry.json from a tree of cog.toml manifests (protocol §3).
//!
//! Native counterpart of `tools/cogstore/catalog.py` — same output shape, cross-checked
//! against the Python generator (same inputs -> same signature). Asset entries are
//! self-contained for install (carry `filename` + any `required_when`); under
//! `manifests_only` the binary is `{path, pending: true}` (cogs A3 PR-time gate).

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

const SHA256_HEX_LEN: usize = 64;

pub fn arch_of(binary: &str) -> Result<String, String> {
    let parts: Vec<&str> = binary.split('-').collect();
    if parts.len() < 3 {
        return Err(format!(
            "binary {binary:?} is not of the form cog-<name>-<arch>"
        ));
    }
    Ok((*parts.last().unwrap()).to_string())
}

fn str_field<'a>(o: &'a Map<String, Value>, k: &str, ctx: &str) -> Result<&'a str, String> {
    o.get(k)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{ctx}: missing/invalid `{k}`"))
}

/// Catalog artifact entry for a manifest `[[assets]]` block — self-contained for install.
pub fn asset_entry(a: &Map<String, Value>, arch: &str) -> Result<Value, String> {
    let id = str_field(a, "id", "asset")?;
    let rel = a
        .get("path")
        .and_then(Value::as_str)
        .or_else(|| a.get("gcs_path").and_then(Value::as_str))
        .ok_or_else(|| format!("asset {id}: needs `path` or `gcs_path`"))?;
    let filename = str_field(a, "filename", &format!("asset {id}"))?;
    let sha256 = str_field(a, "sha256", &format!("asset {id}"))?;
    let size = a
        .get("size_bytes")
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("asset {id}: missing `size_bytes`"))?;

    let mut e = Map::new();
    e.insert("id".into(), Value::from(id));
    e.insert("path".into(), Value::from(format!("cogs/{arch}/{rel}")));
    e.insert("filename".into(), Value::from(filename));
    e.insert("sha256".into(), Value::from(sha256));
    e.insert("size".into(), Value::from(size));
    if let Some(rw) = a.get("required_when") {
        e.insert("required_when".into(), rw.clone());
    }
    Ok(Value::Object(e))
}

pub fn build_cog_version(
    cog_dir: &Path,
    artifacts_dir: Option<&Path>,
    manifests_only: bool,
) -> Result<Value, String> {
    let toml_str = std::fs::read_to_string(cog_dir.join("cog.toml")).map_err(|e| e.to_string())?;
    let manifest: Value = toml::from_str(&toml_str).map_err(|e| format!("parse cog.toml: {e}"))?;
    let cog = manifest
        .get("cog")
        .and_then(Value::as_object)
        .ok_or("missing [cog]")?;
    let binary = str_field(cog, "binary", "[cog]")?;
    let version = str_field(cog, "version", "[cog]")?.to_string();
    let arch = arch_of(binary)?;
    let bin_rel = format!("cogs/{arch}/{binary}");

    let binary_art = if manifests_only {
        serde_json::json!({ "path": bin_rel, "pending": true })
    } else {
        let dir = artifacts_dir.ok_or("artifacts_dir required unless manifests_only")?;
        let bin_file = dir.join(&bin_rel);
        let bytes = std::fs::read(&bin_file)
            .map_err(|e| format!("binary artifact {}: {e}", bin_file.display()))?;
        let sha = hex::encode(Sha256::digest(&bytes));
        serde_json::json!({ "path": bin_rel, "sha256": sha, "size": bytes.len() })
    };

    let mut assets = Vec::new();
    if let Some(arr) = manifest.get("assets").and_then(Value::as_array) {
        for a in arr {
            let o = a.as_object().ok_or("an [[assets]] entry is not a table")?;
            assets.push(asset_entry(o, &arch)?);
        }
    }

    Ok(serde_json::json!({
        "version": version,
        "manifest": manifest,
        "artifacts": { "binary": binary_art, "assets": Value::Array(assets) },
    }))
}

/// The cog ids a catalog offers — for building a `resolve::Resolver`'s offerings map.
pub fn cog_ids(catalog: &Value) -> Vec<String> {
    catalog
        .get("cogs")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|c| c.get("id").and_then(Value::as_str).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

pub fn build_catalog(
    cogs_dir: &Path,
    artifacts_dir: Option<&Path>,
    store_id: &str,
    generated_at: &str,
    manifests_only: bool,
) -> Result<Value, String> {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(cogs_dir)
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok().map(|e| e.path()))
        .filter(|p| p.join("cog.toml").is_file())
        .collect();
    dirs.sort();

    let mut cogs = Vec::new();
    for d in &dirs {
        let ver = build_cog_version(d, artifacts_dir, manifests_only)?;
        let id = ver["manifest"]["cog"]["id"]
            .as_str()
            .ok_or("missing cog.id")?
            .to_string();
        cogs.push(serde_json::json!({ "id": id, "versions": [ver] }));
    }

    let catalog = serde_json::json!({
        "schema_version": 1,
        "store_id": store_id,
        "generated_at": generated_at,
        "cogs": cogs,
    });
    validate(&catalog)?;
    Ok(catalog)
}

pub fn validate(catalog: &Value) -> Result<(), String> {
    let o = catalog
        .as_object()
        .ok_or("invalid catalog: not an object")?;
    if o.get("schema_version").and_then(Value::as_i64) != Some(1) {
        return Err("invalid catalog: schema_version must be 1".into());
    }
    if !o
        .get("store_id")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        return Err("invalid catalog: store_id missing".into());
    }
    if o.get("generated_at").and_then(Value::as_str).is_none() {
        return Err("invalid catalog: generated_at must be a string".into());
    }
    let cogs = o
        .get("cogs")
        .and_then(Value::as_array)
        .ok_or("invalid catalog: cogs must be a list")?;

    let mut seen = std::collections::HashSet::new();
    for c in cogs {
        let cid = c
            .get("id")
            .and_then(Value::as_str)
            .ok_or("invalid catalog: cog.id missing")?;
        if !seen.insert(cid) {
            return Err(format!("invalid catalog: duplicate cog id {cid:?}"));
        }
        let versions = c
            .get("versions")
            .and_then(Value::as_array)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| format!("invalid catalog: {cid}.versions empty"))?;
        for v in versions {
            if v.get("version").and_then(Value::as_str).is_none() {
                return Err(format!("invalid catalog: {cid}.version missing"));
            }
            if v.get("manifest").and_then(Value::as_object).is_none() {
                return Err(format!("invalid catalog: {cid}.manifest missing"));
            }
            let arts = v
                .get("artifacts")
                .and_then(Value::as_object)
                .ok_or_else(|| format!("invalid catalog: {cid}.artifacts missing"))?;
            check_artifact(arts.get("binary"), &format!("{cid} binary"), true)?;
            let assets = arts
                .get("assets")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("invalid catalog: {cid} assets must be a list"))?;
            for a in assets {
                let aid = a.get("id").and_then(Value::as_str).unwrap_or("?");
                check_artifact(Some(a), &format!("{cid} asset {aid}"), false)?;
                if !a
                    .get("filename")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.is_empty())
                {
                    return Err(format!(
                        "invalid catalog: {cid} asset {aid}: filename missing"
                    ));
                }
                if let Some(rw) = a.get("required_when") {
                    if !rw.is_string() {
                        return Err(format!(
                            "invalid catalog: {cid} asset {aid}: required_when must be a string"
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

fn check_artifact(a: Option<&Value>, where_: &str, allow_pending: bool) -> Result<(), String> {
    let o = a
        .and_then(Value::as_object)
        .ok_or_else(|| format!("invalid catalog: {where_}: missing"))?;
    let p = o
        .get("path")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("invalid catalog: {where_}: path missing"))?;
    if p.contains("://") || p.starts_with('/') {
        return Err(format!(
            "invalid catalog: {where_}: path must be relative, got {p:?}"
        ));
    }
    if allow_pending && o.get("pending").and_then(Value::as_bool) == Some(true) {
        return Ok(());
    }
    let sha_ok = o.get("sha256").and_then(Value::as_str).is_some_and(|s| {
        s.len() == SHA256_HEX_LEN
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    });
    if !sha_ok {
        return Err(format!(
            "invalid catalog: {where_}: sha256 must be 64 lowercase hex"
        ));
    }
    if !o
        .get("size")
        .and_then(Value::as_i64)
        .is_some_and(|n| n >= 0)
    {
        return Err(format!(
            "invalid catalog: {where_}: size must be a non-negative integer"
        ));
    }
    Ok(())
}
