//! Store-info document (`store.json`) — the identity a store publishes for trust-on-first-use
//! (Phase 2, protocol §9). It carries the store's public key(s) so a Seed can show a
//! **fingerprint**, the user confirms, and the keys are **pinned**. Self-signed with the same
//! envelope as the catalog: the signature is integrity (the doc isn't truncated/altered);
//! authority comes from the user confirming the fingerprint. After pinning, a Seed verifies
//! against the pinned key instead.

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::signing::{self, TrustStore};

fn raw_key(pubkey_b64: &str) -> Result<[u8; 32], String> {
    STANDARD
        .decode(pubkey_b64)
        .ok()
        .and_then(|v| v.try_into().ok())
        .ok_or_else(|| format!("public key must be base64 of 32 bytes: {pubkey_b64:?}"))
}

/// SHA-256 fingerprint (lowercase hex) of a raw 32-byte key (protocol §7.3).
pub fn fingerprint(pubkey_b64: &str) -> Result<String, String> {
    Ok(hex::encode(Sha256::digest(raw_key(pubkey_b64)?)))
}

/// Build an (unsigned) store-info document with a single key.
pub fn build_store_info(
    store_id: &str,
    name: &str,
    description: &str,
    catalog_url: &str,
    key_id: &str,
    pubkey_b64: &str,
) -> Result<Value, String> {
    raw_key(pubkey_b64)?; // validate the key up front
    Ok(json!({
        "schema_version": 1,
        "store_id": store_id,
        "name": name,
        "description": description,
        "keys": [ { "key_id": key_id, "alg": "ed25519", "pubkey_b64": pubkey_b64 } ],
        "catalog_url": catalog_url,
    }))
}

/// The store's own listed keys as a trust store.
pub fn trust_from_keys(store: &Value) -> Result<TrustStore, String> {
    let keys = store
        .get("keys")
        .and_then(Value::as_array)
        .ok_or("store.json: missing keys[]")?;
    let mut t = TrustStore::new();
    for k in keys {
        let kid = k
            .get("key_id")
            .and_then(Value::as_str)
            .ok_or("store.json key: missing key_id")?;
        let pb = k
            .get("pubkey_b64")
            .and_then(Value::as_str)
            .ok_or("store.json key: missing pubkey_b64")?;
        t.insert(kid.to_string(), raw_key(pb)?);
    }
    Ok(t)
}

/// `(key_id, fingerprint)` for each key — what the TOFU prompt shows.
pub fn fingerprints(store: &Value) -> Result<Vec<(String, String)>, String> {
    let keys = store
        .get("keys")
        .and_then(Value::as_array)
        .ok_or("store.json: missing keys[]")?;
    keys.iter()
        .map(|k| {
            let kid = k
                .get("key_id")
                .and_then(Value::as_str)
                .ok_or("key missing key_id")?;
            let pb = k
                .get("pubkey_b64")
                .and_then(Value::as_str)
                .ok_or("key missing pubkey_b64")?;
            Ok((kid.to_string(), fingerprint(pb)?))
        })
        .collect()
}

/// Verify a self-signed `store.json` against its OWN listed keys (integrity). TOFU authority
/// is the user confirming the fingerprint; after pinning, verify against the pinned key.
pub fn verify_self_signed(store: &Value) -> Result<String, String> {
    signing::verify_document(store, &trust_from_keys(store)?)
}

pub fn validate(store: &Value) -> Result<(), String> {
    let o = store
        .as_object()
        .ok_or("invalid store.json: not an object")?;
    if o.get("schema_version").and_then(Value::as_i64) != Some(1) {
        return Err("invalid store.json: schema_version must be 1".into());
    }
    for f in ["store_id", "name", "catalog_url"] {
        if !o
            .get(f)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        {
            return Err(format!("invalid store.json: `{f}` missing"));
        }
    }
    let url = o.get("catalog_url").and_then(Value::as_str).unwrap();
    if !url.contains("://") {
        return Err(format!(
            "invalid store.json: catalog_url must be an absolute URL, got {url:?}"
        ));
    }
    let keys = o
        .get("keys")
        .and_then(Value::as_array)
        .filter(|k| !k.is_empty())
        .ok_or("invalid store.json: keys[] must be non-empty")?;
    for k in keys {
        let ko = k
            .as_object()
            .ok_or("invalid store.json: a key is not an object")?;
        if ko.get("alg").and_then(Value::as_str) != Some("ed25519") {
            return Err("invalid store.json: key alg must be ed25519".into());
        }
        if !ko
            .get("key_id")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        {
            return Err("invalid store.json: key.key_id missing".into());
        }
        let pb = ko
            .get("pubkey_b64")
            .and_then(Value::as_str)
            .ok_or("invalid store.json: key.pubkey_b64 missing")?;
        raw_key(pb)?; // must be base64 of 32 bytes
    }
    Ok(())
}
