//! Audit log conformance (Phase 3, T0-B). The Rust append/verify must reproduce the frozen
//! chain vector (docs/protocol/testvectors/audit/) byte-for-byte and reject any edit, reorder,
//! or mid-log deletion at the right `seq` — the same contract the Python oracle
//! (`tools/cogstore/audit.py`) holds.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{json, Map, Value};

use gearbox::audit;

const HEAD_SELF: &str = "65a00c0ac86fd4ad8b16919bc9b5022939481ce87bcb783818ae8d78ae8ea2d3";

static N: AtomicUsize = AtomicUsize::new(0);

fn tmpfile() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "gbaudit-{}-{}.jsonl",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_file(&p);
    p
}

fn vector() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/protocol/testvectors/audit/log.jsonl")
}

fn detail(pairs: &[(&str, &str)]) -> Map<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), Value::from(*v)))
        .collect()
}

/// Append the vector's four records to a fresh file (the canonical add_store -> verify_catalog
/// -> install -> policy_deny sequence, with a non-ASCII detail value).
fn build_vector_log(path: &Path) {
    audit::append(
        path,
        "2026-06-14T15:00:00Z",
        "add_store",
        "acme-internal",
        detail(&[
            ("key_id", "acme-signing-2026"),
            (
                "fingerprint",
                "56475aa75463474c0285df5dbf2bcab73da651358839e9b77481b2eab107708c",
            ),
            ("result", "ok"),
        ]),
    )
    .unwrap();
    audit::append(
        path,
        "2026-06-14T15:01:00Z",
        "verify_catalog",
        "acme-internal",
        detail(&[("key_id", "acme-signing-2026"), ("result", "ok")]),
    )
    .unwrap();
    audit::append(
        path,
        "2026-06-14T15:02:00Z",
        "install",
        "acme-internal/doom@0.1.0",
        detail(&[
            (
                "sha256",
                "238a6e038d11d2b9851396b8ec167ad2f5c8724525100473c2a3f06c9ea43561",
            ),
            ("result", "ok"),
        ]),
    )
    .unwrap();
    audit::append(
        path,
        "2026-06-14T15:03:00Z",
        "policy_deny",
        "cognitum-official/doom",
        detail(&[
            ("reason", "store not allowed — managed policy"),
            ("result", "deny"),
        ]),
    )
    .unwrap();
}

#[test]
fn verifies_frozen_vector() {
    let records = audit::read_log(&vector()).unwrap();
    let report = audit::verify(&records).unwrap();
    assert_eq!(report.n, 4);
    assert_eq!(report.head_self, HEAD_SELF);
}

#[test]
fn every_record_self_matches_stored() {
    for rec in audit::read_log(&vector()).unwrap() {
        let stored = rec.get("self").and_then(Value::as_str).unwrap();
        assert_eq!(audit::record_self(&rec).unwrap(), stored);
    }
}

#[test]
fn append_reproduces_frozen_vector_byte_for_byte() {
    let path = tmpfile();
    build_vector_log(&path);
    let produced = std::fs::read(&path).unwrap();
    let frozen = std::fs::read(vector()).unwrap();
    assert_eq!(
        produced, frozen,
        "Rust-appended log differs from the frozen vector"
    );
}

#[test]
fn chain_links_prev_to_previous_self() {
    let records = audit::read_log(&vector()).unwrap();
    assert_eq!(records[0]["prev"], json!(audit::ZERO_PREV));
    for w in records.windows(2) {
        assert_eq!(w[1]["prev"], w[0]["self"], "prev must be the previous self");
    }
}

#[test]
fn tampered_record_fails_at_its_seq() {
    let mut records = audit::read_log(&vector()).unwrap();
    records[1]["detail"]["result"] = json!("EVIL"); // edit a signed-over field
    let brk = audit::verify(&records).unwrap_err();
    assert_eq!(brk.seq, 1);
    assert!(brk.reason.contains("self mismatch"), "got: {}", brk.reason);
}

#[test]
fn reordering_records_fails() {
    let mut records = audit::read_log(&vector()).unwrap();
    records.swap(1, 2);
    assert!(audit::verify(&records).is_err());
}

#[test]
fn mid_log_deletion_fails() {
    let mut records = audit::read_log(&vector()).unwrap();
    records.remove(1); // drop a middle record -> seq gap + broken linkage
    let brk = audit::verify(&records).unwrap_err();
    assert_eq!(brk.seq, 2, "the record now out of place");
}

#[test]
fn append_extends_an_existing_chain() {
    let path = tmpfile();
    build_vector_log(&path);
    let rec = audit::append(
        &path,
        "2026-06-14T15:04:00Z",
        "key_change",
        "acme-internal",
        detail(&[("result", "ok")]),
    )
    .unwrap();
    assert_eq!(rec["seq"], json!(4));
    assert_eq!(
        rec["prev"],
        json!(HEAD_SELF),
        "new prev is the old head self"
    );
    let report = audit::verify(&audit::read_log(&path).unwrap()).unwrap();
    assert_eq!(report.n, 5);
}

#[test]
fn parse_details_splits_on_first_equals_and_guards_keys() {
    // A value may contain '=' (split on the FIRST one); a key must be present and ASCII.
    let d = audit::parse_details(&[
        "result=ok".to_string(),
        "expr=a==b".to_string(), // value keeps the trailing "=b"
    ])
    .unwrap();
    assert_eq!(d.get("result").unwrap(), "ok");
    assert_eq!(d.get("expr").unwrap(), "a==b");

    assert!(audit::parse_details(&["noequals".to_string()]).is_err());
    assert!(audit::parse_details(&["=novalue".to_string()]).is_err()); // empty key
    assert!(audit::parse_details(&["café=x".to_string()]).is_err()); // non-ASCII key
}

#[test]
fn empty_and_single_record_logs_verify() {
    let empty: Vec<Value> = Vec::new();
    assert_eq!(audit::verify(&empty).unwrap().n, 0);

    let path = tmpfile();
    audit::append(&path, "2026-06-14T00:00:00Z", "install", "x", Map::new()).unwrap();
    let records = audit::read_log(&path).unwrap();
    assert_eq!(records[0]["seq"], json!(0));
    assert_eq!(records[0]["prev"], json!(audit::ZERO_PREV));
    assert_eq!(audit::verify(&records).unwrap().n, 1);
}
