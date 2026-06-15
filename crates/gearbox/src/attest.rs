//! Build provenance + SBOM attestation (Phase 3+, T1-shaped) — a **signed** sidecar that binds
//! an artifact to *where it came from* (SLSA-style provenance) and *what is inside it* (an SBOM),
//! verifiable offline (protocol §13).
//!
//! ```jsonc
//! {
//!   "schema_version": 1,
//!   "subject": { "cog": "doom", "version": "0.1.0",
//!                "artifact": "cogs/arm/cog-doom-arm", "sha256": "238a6e0…" },
//!   "provenance": { "builder": "cogs-ci", "source_repo": "github.com/…/cogs",
//!                   "source_commit": "abc123…", "built_at": "2026-06-14T00:00:00Z" },
//!   "sbom": { "packages": [ { "name": "freedoom", "version": "0.13.0",
//!                             "license": "BSD-3-Clause", "sha256": "7323…" } ] },
//!   "signature": { "key_id": "…", "alg": "ed25519", "sig": "…" }   // §7.2
//! }
//! ```
//!
//! Minimal + self-contained — the JCS subset, signed with the existing Ed25519 envelope (no new
//! crypto, no new deps). The field names are deliberately **SLSA/SPDX-shaped** (`subject`+`sha256`
//! ↔ in-toto `subject.digest`; `builder`/`source_*` ↔ SLSA provenance; `packages[]` ↔ SPDX), so a
//! later "emit real in-toto/SPDX" step is a field reshaping, not a data-collection redesign.
//!
//! **Two independent guards** (like the bundle): the whole document — including `subject.sha256` —
//! is signed, so tampering the provenance/SBOM breaks the **signature**; swapping the artifact
//! breaks the **`sha256` binding** ([`check_artifact`]). A full `verify` needs both the trusted key
//! and the artifact bytes.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::signing::{self, TrustStore};

const SHA256_HEX_LEN: usize = 64;

fn is_sha256_hex(s: &str) -> bool {
    s.len() == SHA256_HEX_LEN
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// sha256 (lowercase hex) of arbitrary bytes — the artifact digest an attestation binds to.
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// One SBOM package entry.
pub struct Package {
    pub name: String,
    pub version: String,
    pub license: String,
    pub sha256: String,
}

/// What the attestation is about: a cog version's artifact and its digest (the binding).
pub struct Subject<'a> {
    pub cog: &'a str,
    pub version: &'a str,
    pub artifact: &'a str,
    pub sha256: &'a str,
}

/// Where the artifact came from (SLSA-style provenance).
pub struct Provenance<'a> {
    pub builder: &'a str,
    pub source_repo: &'a str,
    pub source_commit: &'a str,
    pub built_at: &'a str,
}

/// Build an (unsigned) attestation document.
pub fn build_attestation(
    subject: &Subject,
    provenance: &Provenance,
    packages: &[Package],
) -> Value {
    let pkgs: Vec<Value> = packages
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "version": p.version,
                "license": p.license,
                "sha256": p.sha256,
            })
        })
        .collect();
    json!({
        "schema_version": 1,
        "subject": {
            "cog": subject.cog,
            "version": subject.version,
            "artifact": subject.artifact,
            "sha256": subject.sha256,
        },
        "provenance": {
            "builder": provenance.builder,
            "source_repo": provenance.source_repo,
            "source_commit": provenance.source_commit,
            "built_at": provenance.built_at,
        },
        "sbom": { "packages": Value::Array(pkgs) },
    })
}

/// The artifact digest the attestation binds to (`subject.sha256`).
pub fn subject_sha256(doc: &Value) -> Result<&str, String> {
    doc.get("subject")
        .and_then(|s| s.get("sha256"))
        .and_then(Value::as_str)
        .ok_or_else(|| "attestation: subject.sha256 missing".into())
}

/// Verify the signature against a trusted key store, after a schema check; return the key id.
pub fn verify(doc: &Value, trust: &TrustStore) -> Result<String, String> {
    validate(doc)?;
    signing::verify_document(doc, trust)
}

/// Check that `artifact_bytes` are the bytes this attestation is about (the digest binding).
pub fn check_artifact(doc: &Value, artifact_bytes: &[u8]) -> Result<(), String> {
    let want = subject_sha256(doc)?;
    let got = sha256_hex(artifact_bytes);
    if got != want {
        return Err(format!(
            "artifact does not match attestation subject: sha256 {got} != {want}"
        ));
    }
    Ok(())
}

pub fn validate(doc: &Value) -> Result<(), String> {
    let o = doc
        .as_object()
        .ok_or("invalid attestation: not an object")?;
    if o.get("schema_version").and_then(Value::as_i64) != Some(1) {
        return Err("invalid attestation: schema_version must be 1".into());
    }
    let subject = o
        .get("subject")
        .and_then(Value::as_object)
        .ok_or("invalid attestation: subject must be an object")?;
    for f in ["cog", "version", "artifact"] {
        if !subject
            .get(f)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        {
            return Err(format!("invalid attestation: subject.{f} missing"));
        }
    }
    if !subject
        .get("sha256")
        .and_then(Value::as_str)
        .is_some_and(is_sha256_hex)
    {
        return Err("invalid attestation: subject.sha256 must be 64 lowercase hex".into());
    }
    let prov = o
        .get("provenance")
        .and_then(Value::as_object)
        .ok_or("invalid attestation: provenance must be an object")?;
    for f in ["builder", "source_repo", "source_commit", "built_at"] {
        if !prov
            .get(f)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        {
            return Err(format!("invalid attestation: provenance.{f} missing"));
        }
    }
    let packages = o
        .get("sbom")
        .and_then(|s| s.get("packages"))
        .and_then(Value::as_array)
        .ok_or("invalid attestation: sbom.packages must be a list")?;
    for (i, p) in packages.iter().enumerate() {
        let po = p
            .as_object()
            .ok_or_else(|| format!("invalid attestation: sbom.packages[{i}] not an object"))?;
        for f in ["name", "version", "license"] {
            if !po
                .get(f)
                .and_then(Value::as_str)
                .is_some_and(|s| !s.is_empty())
            {
                return Err(format!(
                    "invalid attestation: sbom.packages[{i}].{f} missing"
                ));
            }
        }
        if !po
            .get("sha256")
            .and_then(Value::as_str)
            .is_some_and(is_sha256_hex)
        {
            return Err(format!(
                "invalid attestation: sbom.packages[{i}].sha256 must be 64 lowercase hex"
            ));
        }
    }
    Ok(())
}

/// Parse repeated `name=version=license=sha256` tokens into SBOM packages (exactly four fields).
pub fn parse_packages(pairs: &[String]) -> Result<Vec<Package>, String> {
    pairs
        .iter()
        .map(|p| {
            let parts: Vec<&str> = p.split('=').collect();
            if parts.len() != 4 {
                return Err(format!(
                    "--package {p:?} must be name=version=license=sha256 (four fields)"
                ));
            }
            if parts.iter().any(|f| f.is_empty()) {
                return Err(format!(
                    "--package {p:?}: all four fields must be non-empty"
                ));
            }
            if !is_sha256_hex(parts[3]) {
                return Err(format!("--package {p:?}: sha256 must be 64 lowercase hex"));
            }
            Ok(Package {
                name: parts[0].to_string(),
                version: parts[1].to_string(),
                license: parts[2].to_string(),
                sha256: parts[3].to_string(),
            })
        })
        .collect()
}
