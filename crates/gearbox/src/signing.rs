//! Ed25519 sign / verify of a JSON document over its JCS canonicalization (protocol §7).
//!
//! The catalog and the store-info document share one signature envelope, so the core is
//! generic (`sign_document` / `verify_document`); `sign_catalog` / `verify_catalog` are
//! aliases. Producer-independent counterpart to the device-side verifier (seed B4) and the
//! Python `tools/` signer: same algorithm, same test vector.

use std::collections::HashMap;

use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde_json::Value;

use crate::jcs;

/// Trusted keys: `key_id -> raw 32-byte Ed25519 public key`.
pub type TrustStore = HashMap<String, [u8; 32]>;

/// The signing input: the document with its own `signature` member removed, JCS-canonicalized.
fn signing_input(doc: &Value) -> Result<Vec<u8>, String> {
    let mut body = doc.clone();
    body.as_object_mut()
        .ok_or("document is not a JSON object")?
        .remove("signature");
    jcs::canonical(&body)
}

/// Base64 of the raw 32-byte public key for an Ed25519 seed.
pub fn public_key_b64(seed: &[u8; 32]) -> String {
    STANDARD.encode(SigningKey::from_bytes(seed).verifying_key().to_bytes())
}

/// Return a copy of `doc` carrying a `signature` member (protocol §7.2).
pub fn sign_document(doc: &Value, seed: &[u8; 32], key_id: &str) -> Result<Value, String> {
    let msg = signing_input(doc)?;
    let sig = SigningKey::from_bytes(seed).sign(&msg);
    let mut out = doc.clone();
    let obj = out.as_object_mut().ok_or("document is not a JSON object")?;
    obj.remove("signature");
    obj.insert(
        "signature".to_string(),
        serde_json::json!({
            "key_id": key_id,
            "alg": "ed25519",
            "sig": STANDARD.encode(sig.to_bytes()),
        }),
    );
    Ok(out)
}

/// Verify a document's signature; on success return the `key_id` that signed it.
pub fn verify_document(doc: &Value, trust: &TrustStore) -> Result<String, String> {
    let obj = doc.as_object().ok_or("document is not a JSON object")?;
    let sig = obj.get("signature").ok_or("document has no signature")?;
    let key_id = sig.get("key_id").and_then(Value::as_str).ok_or("malformed signature.key_id")?;
    let alg = sig.get("alg").and_then(Value::as_str).ok_or("malformed signature.alg")?;
    let sig_b64 = sig.get("sig").and_then(Value::as_str).ok_or("malformed signature.sig")?;

    if alg != "ed25519" {
        return Err(format!("unsupported alg {alg:?}"));
    }
    let pk = trust.get(key_id).ok_or_else(|| format!("untrusted key_id {key_id:?}"))?;
    let vk = VerifyingKey::from_bytes(pk).map_err(|e| format!("bad public key: {e}"))?;

    let msg = signing_input(doc)?;
    let raw = STANDARD.decode(sig_b64).map_err(|e| format!("bad base64: {e}"))?;
    let signature = Signature::from_slice(&raw).map_err(|e| format!("bad signature: {e}"))?;
    vk.verify_strict(&msg, &signature)
        .map_err(|_| "signature did not verify".to_string())?;
    Ok(key_id.to_string())
}

/// Catalog is just a signed document — aliases for readability at call sites.
pub fn sign_catalog(catalog: &Value, seed: &[u8; 32], key_id: &str) -> Result<Value, String> {
    sign_document(catalog, seed, key_id)
}
pub fn verify_catalog(catalog: &Value, trust: &TrustStore) -> Result<String, String> {
    verify_document(catalog, trust)
}
