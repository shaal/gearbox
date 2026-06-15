//! Audit / event log (Phase 3, T0-B) — an append-only, hash-chained JSONL log that is
//! **tamper-evident offline** with no server and no key.
//!
//! Each line is one record, hash-chained to the previous one (Merkle-style):
//!
//! ```jsonc
//! {
//!   "seq": 0,                          // contiguous from 0
//!   "ts": "2026-06-14T15:00:00Z",      // caller-supplied (reproducible; the seed passes real time)
//!   "event": "install",               // add_store | verify_catalog | install | policy_deny | key_change
//!   "subject": "acme-internal/doom@1.2.0",
//!   "detail": { "result": "ok" },      // free-form string→string map (ASCII keys, UTF-8 values)
//!   "prev": "<self of the previous record, or 64 zeros for seq 0>",
//!   "self": "<sha256 of JCS(this record without `self`)>"
//! }
//! ```
//!
//! `prev`/`self` reuse `jcs` + `sha2` only — **no new crypto, no key** for the chain itself.
//! Because `prev` is part of the bytes that `self` hashes, every `self` transitively commits to
//! the whole prior chain: a flipped byte anywhere breaks `self` here and `prev` on the next record.
//!
//! Detectable offline by `verify`: any **edit**, **reordering**, or **mid-log deletion** — it
//! reports the first bad `seq`. A pure *tail* truncation yields a still-valid shorter prefix the
//! keyless chain alone can't catch; [`sign a head`](build_head) over the tip and `verify_head`
//! closes that — tamper-*evident* becomes tamper-*proof* up to the signed checkpoint (protocol
//! §11.4).
//!
//! Each stored line is the **JCS canonical bytes** of the record, so an `audit.jsonl` written by
//! this crate and by the Python oracle (`tools/cogstore/audit.py`) are byte-identical.

use std::path::Path;

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::jcs;
use crate::signing::{self, TrustStore};

/// `prev` of the first record (`seq` 0): 64 hex zeros (no predecessor to chain to).
pub const ZERO_PREV: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// The event vocabulary the protocol names (§5). Other values are accepted (forward-compat for
/// new seed hooks) but a typo'd event is easy to catch against this set.
pub const KNOWN_EVENTS: [&str; 5] = [
    "add_store",
    "verify_catalog",
    "install",
    "policy_deny",
    "key_change",
];

/// sha256 of the JCS canonical bytes of `record` with its `self` member removed — the value a
/// record's `self` must equal, and the value the next record's `prev` chains to.
pub fn record_self(record: &Value) -> Result<String, String> {
    let mut body = record.clone();
    body.as_object_mut()
        .ok_or("record is not a JSON object")?
        .remove("self");
    Ok(hex::encode(Sha256::digest(jcs::canonical(&body)?)))
}

/// Build a complete record (including its `self`) from its fields and the chain's current head.
pub fn build_record(
    seq: i64,
    ts: &str,
    event: &str,
    subject: &str,
    detail: Map<String, Value>,
    prev: &str,
) -> Result<Value, String> {
    let mut record = json!({
        "seq": seq,
        "ts": ts,
        "event": event,
        "subject": subject,
        "detail": Value::Object(detail),
        "prev": prev,
    });
    let self_hash = record_self(&record)?;
    record
        .as_object_mut()
        .unwrap()
        .insert("self".to_string(), Value::from(self_hash));
    Ok(record)
}

/// Parse a log file into its records (one per non-empty line). A missing file is an empty log.
pub fn read_log(path: &Path) -> Result<Vec<Value>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("read {}: {e}", path.display())),
    };
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = serde_json::from_str(line).map_err(|e| format!("line {}: {e}", i + 1))?;
        out.push(v);
    }
    Ok(out)
}

/// The chain head: `(next_seq, prev_for_next)` derived from the last record, or `(0, ZERO_PREV)`
/// for an empty log.
fn head(records: &[Value]) -> Result<(i64, String), String> {
    match records.last() {
        None => Ok((0, ZERO_PREV.to_string())),
        Some(last) => {
            let seq = last
                .get("seq")
                .and_then(Value::as_i64)
                .ok_or("last record missing integer `seq`")?;
            let self_hash = last
                .get("self")
                .and_then(Value::as_str)
                .ok_or("last record missing `self`")?;
            Ok((seq + 1, self_hash.to_string()))
        }
    }
}

/// Append one chained record to the log (creating it if absent) and return the new record. The
/// stored line is the record's JCS canonical bytes, so the file stays byte-stable across impls.
pub fn append(
    path: &Path,
    ts: &str,
    event: &str,
    subject: &str,
    detail: Map<String, Value>,
) -> Result<Value, String> {
    let records = read_log(path)?;
    let (seq, prev) = head(&records)?;
    let record = build_record(seq, ts, event, subject, detail, &prev)?;

    let mut line = jcs::canonical(&record)?;
    line.push(b'\n');
    let mut bytes = std::fs::read(path).unwrap_or_default();
    bytes.extend_from_slice(&line);
    std::fs::write(path, &bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(record)
}

/// A successful verification: how many records, and the head `self` (the tip to optionally sign).
#[derive(Debug, Clone)]
pub struct VerifyReport {
    pub n: usize,
    pub head_self: String,
}

/// The first place the chain breaks: which `seq` and why.
#[derive(Debug, Clone)]
pub struct VerifyBreak {
    pub seq: i64,
    pub reason: String,
}

impl std::fmt::Display for VerifyBreak {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "seq {}: {}", self.seq, self.reason)
    }
}

/// Recompute the chain. `Ok` if every record's `self` is correct, `seq` is contiguous from 0, and
/// each `prev` links to the previous `self`; otherwise the first bad `seq` and the reason.
pub fn verify(records: &[Value]) -> Result<VerifyReport, VerifyBreak> {
    let mut prev_expected = ZERO_PREV.to_string();
    let mut head_self = ZERO_PREV.to_string();

    for (i, rec) in records.iter().enumerate() {
        let expected_seq = i as i64;
        let seq = rec.get("seq").and_then(Value::as_i64).unwrap_or(-1);
        let stored_self = rec.get("self").and_then(Value::as_str).unwrap_or("");

        // 1. content integrity — does `self` match the record's own bytes?
        match record_self(rec) {
            Ok(computed) if computed == stored_self => {}
            Ok(_) => {
                return Err(VerifyBreak {
                    seq,
                    reason: "record content altered (self mismatch)".into(),
                })
            }
            Err(e) => return Err(VerifyBreak { seq, reason: e }),
        }
        // 2. sequence — contiguous from 0 (catches reordering, gaps, mid-log deletion).
        if seq != expected_seq {
            return Err(VerifyBreak {
                seq,
                reason: format!("out-of-order or missing record (expected seq {expected_seq})"),
            });
        }
        // 3. linkage — does `prev` chain to the previous record's `self`?
        let prev = rec.get("prev").and_then(Value::as_str).unwrap_or("");
        if prev != prev_expected {
            return Err(VerifyBreak {
                seq,
                reason: "broken chain (prev != previous self)".into(),
            });
        }
        prev_expected = stored_self.to_string();
        head_self = stored_self.to_string();
    }

    Ok(VerifyReport {
        n: records.len(),
        head_self,
    })
}

fn is_hex64(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// Build an (unsigned) **signed-head** document — a checkpoint committing the chain's tip
/// (`count` records ending in `head_self`) so a tail truncation below the checkpoint is
/// detectable. `head_self` is the cryptographic tie to a specific log (a different log has a
/// different tip); `log_id` is a human-facing label, signed but not cross-checked against the
/// log's contents (the records carry no log id).
pub fn build_head(log_id: &str, count: usize, head_self: &str, signed_at: &str) -> Value {
    json!({
        "schema_version": 1,
        "log_id": log_id,
        "count": count as i64,
        "head_self": head_self,
        "signed_at": signed_at,
    })
}

/// A successfully checked signed head.
#[derive(Debug, Clone)]
pub struct HeadReport {
    pub key_id: String,
    pub log_id: String,
    pub count: usize,
}

fn validate_head(head: &Value) -> Result<(), String> {
    let o = head.as_object().ok_or("invalid head: not an object")?;
    if o.get("schema_version").and_then(Value::as_i64) != Some(1) {
        return Err("invalid head: schema_version must be 1".into());
    }
    if !o
        .get("log_id")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        return Err("invalid head: log_id missing".into());
    }
    if !o
        .get("count")
        .and_then(Value::as_i64)
        .is_some_and(|n| n >= 0)
    {
        return Err("invalid head: count must be a non-negative integer".into());
    }
    if !o
        .get("head_self")
        .and_then(Value::as_str)
        .is_some_and(is_hex64)
    {
        return Err("invalid head: head_self must be 64 lowercase hex".into());
    }
    if !o
        .get("signed_at")
        .and_then(Value::as_str)
        .is_some_and(|s| !s.is_empty())
    {
        return Err("invalid head: signed_at missing".into());
    }
    Ok(())
}

/// Verify a signed head against a (chain-verified) log: its signature, then that the records
/// still contain its signed prefix intact — `len >= count` and `records[count-1].self ==
/// head_self`. This makes everything **up to the checkpoint tamper-proof**: lopping off records
/// below `count`, or any edit inside the prefix (which would change `head_self`), fails.
///
/// Records appended *after* the head was signed are beyond the checkpoint — re-sign the head to
/// extend the guarantee (see protocol §11.4). Callers should run [`verify`] first; this checks
/// the head, not the chain's internal integrity.
pub fn verify_head(
    records: &[Value],
    head: &Value,
    trust: &TrustStore,
) -> Result<HeadReport, String> {
    validate_head(head)?;
    let key_id =
        signing::verify_document(head, trust).map_err(|e| format!("head signature: {e}"))?;
    let count = head.get("count").and_then(Value::as_i64).unwrap_or(0);
    let want_self = head.get("head_self").and_then(Value::as_str).unwrap_or("");
    let log_id = head
        .get("log_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();

    if (records.len() as i64) < count {
        return Err(format!(
            "log truncated: {} record(s) < signed count {count}",
            records.len()
        ));
    }
    let got_self = if count == 0 {
        ZERO_PREV
    } else {
        records[(count - 1) as usize]
            .get("self")
            .and_then(Value::as_str)
            .unwrap_or("")
    };
    if got_self != want_self {
        return Err(format!(
            "signed head does not match the log at record {} (chain diverged)",
            count.max(1) - 1
        ));
    }
    Ok(HeadReport {
        key_id,
        log_id,
        count: count as usize,
    })
}

/// Parse repeated `--detail key=value` tokens into an ordered map (split on the first `=`). Keys
/// must be ASCII (the JCS object-key constraint, §7.1); values may be any UTF-8.
pub fn parse_details(pairs: &[String]) -> Result<Map<String, Value>, String> {
    let mut m = Map::new();
    for p in pairs {
        let (k, v) = p
            .split_once('=')
            .ok_or_else(|| format!("--detail {p:?} must be key=value"))?;
        if k.is_empty() {
            return Err(format!("--detail {p:?}: empty key"));
        }
        if !k.is_ascii() {
            return Err(format!("--detail key {k:?} must be ASCII"));
        }
        m.insert(k.to_string(), Value::from(v));
    }
    Ok(m)
}
