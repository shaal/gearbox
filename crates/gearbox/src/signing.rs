//! Ed25519 sign / verify of a catalog over its JCS canonicalization (protocol §7).
//!
//! Producer-independent counterpart to the device-side verifier (seed B4) and the Python
//! `tools/` signer: same algorithm, same test vector. The signing input is the catalog
//! with its own `signature` member removed, then JCS-canonicalized.

use std::collections::HashMap;

use base64::{engine::general_purpose::STANDARD, Engine};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use serde_json::Value;

use crate::jcs;

/// Trusted keys: `key_id -> raw 32-byte Ed25519 public key`.
pub type TrustStore = HashMap<String, [u8; 32]>;

fn signing_input(catalog: &Value) -> Result<Vec<u8>, String> {
    let mut body = catalog.clone();
    body.as_object_mut()
        .ok_or("catalog is not a JSON object")?
        .remove("signature");
    jcs::canonical(&body)
}

/// Return a copy of `catalog` carrying a `signature` member (protocol §7.2).
pub fn sign_catalog(catalog: &Value, seed: &[u8; 32], key_id: &str) -> Result<Value, String> {
    let msg = signing_input(catalog)?;
    let sig = SigningKey::from_bytes(seed).sign(&msg);
    let mut out = catalog.clone();
    let obj = out.as_object_mut().ok_or("catalog is not a JSON object")?;
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

/// Verify a catalog's signature; on success return the `key_id` that signed it.
pub fn verify_catalog(catalog: &Value, trust: &TrustStore) -> Result<String, String> {
    let obj = catalog.as_object().ok_or("catalog is not a JSON object")?;
    let sig = obj.get("signature").ok_or("catalog has no signature")?;
    let key_id = sig.get("key_id").and_then(Value::as_str).ok_or("malformed signature.key_id")?;
    let alg = sig.get("alg").and_then(Value::as_str).ok_or("malformed signature.alg")?;
    let sig_b64 = sig.get("sig").and_then(Value::as_str).ok_or("malformed signature.sig")?;

    if alg != "ed25519" {
        return Err(format!("unsupported alg {alg:?}"));
    }
    let pk = trust.get(key_id).ok_or_else(|| format!("untrusted key_id {key_id:?}"))?;
    let vk = VerifyingKey::from_bytes(pk).map_err(|e| format!("bad public key: {e}"))?;

    let msg = signing_input(catalog)?;
    let raw = STANDARD.decode(sig_b64).map_err(|e| format!("bad base64: {e}"))?;
    let signature = Signature::from_slice(&raw).map_err(|e| format!("bad signature: {e}"))?;
    vk.verify_strict(&msg, &signature)
        .map_err(|_| "signature did not verify".to_string())?;
    Ok(key_id.to_string())
}
