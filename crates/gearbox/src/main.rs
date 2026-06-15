//! `gearbox` CLI — native reference for the cog-store protocol.
//!
//! Subcommands:
//!   verify       <catalog.json> --key-id <ID> --pubkey-b64 <B64>
//!   catalog      --cogs-dir DIR (--artifacts-dir DIR | --manifests-only) --store-id ID
//!                --generated-at TS --out FILE [--sign-seed-hex HEX --key-id ID]
//!   sign         --in FILE --out FILE --sign-seed-hex HEX --key-id ID
//!   store-info create --store-id ID --name NAME [--description D] --catalog-url URL
//!                --key-id KID (--sign-seed-hex HEX | --pubkey-b64 B64) --out FILE
//!   store-info verify <store.json>

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::exit;

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;

use gearbox::{audit, bundle, catalog, server, signing, store};

const USAGE: &str = "\
usage:
  gearbox verify  <catalog.json> --key-id <ID> --pubkey-b64 <B64>
  gearbox catalog --cogs-dir DIR (--artifacts-dir DIR | --manifests-only) \\
                  --store-id ID --generated-at TS --out FILE [--sign-seed-hex HEX --key-id ID]
  gearbox sign    --in FILE --out FILE --sign-seed-hex HEX --key-id ID
  gearbox store-info create --store-id ID --name NAME [--description D] --catalog-url URL \\
                  --key-id KID (--sign-seed-hex HEX | --pubkey-b64 B64) --out FILE
  gearbox store-info verify <store.json>
  gearbox export  --catalog app-registry.json --store-info store.json --artifacts-dir DIR \\
                  --out BUNDLE --generated-at TS [--sign-seed-hex HEX --key-id ID]
  gearbox import  <bundle-dir> [--expect-fingerprint HEX]
  gearbox audit append --log FILE --ts TS --event EVENT --subject SUBJ [--detail k=v ...]
  gearbox audit verify --log FILE
  gearbox serve   --dir DIR [--port N] [--auth-token TOKEN]";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = match args.get(1).map(String::as_str) {
        Some("verify") => cmd_verify(&args[2..]),
        Some("catalog") => cmd_catalog(&args[2..]),
        Some("sign") => cmd_sign(&args[2..]),
        Some("store-info") => match args.get(2).map(String::as_str) {
            Some("create") => cmd_store_create(&args[3..]),
            Some("verify") => cmd_store_verify(&args[3..]),
            _ => {
                eprintln!("{USAGE}");
                2
            }
        },
        Some("export") => cmd_export(&args[2..]),
        Some("import") => cmd_import(&args[2..]),
        Some("audit") => match args.get(2).map(String::as_str) {
            Some("append") => cmd_audit_append(&args[3..]),
            Some("verify") => cmd_audit_verify(&args[3..]),
            _ => {
                eprintln!("{USAGE}");
                2
            }
        },
        Some("serve") => cmd_serve(&args[2..]),
        _ => {
            eprintln!("{USAGE}");
            2
        }
    };
    exit(code);
}

/// Parsed CLI args: `--key value` pairs, the boolean flags present, and positionals.
type ParsedFlags = (HashMap<String, String>, HashSet<String>, Vec<String>);

/// Split args into `(--key value)` pairs, boolean flags, and positionals.
fn parse_flags(args: &[String], bool_flags: &[&str]) -> Result<ParsedFlags, String> {
    let mut kv = HashMap::new();
    let mut flags = HashSet::new();
    let mut pos = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if a.starts_with("--") {
            if bool_flags.contains(&a.as_str()) {
                flags.insert(a.clone());
                i += 1;
            } else {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| format!("{a} needs a value"))?;
                kv.insert(a.clone(), v.clone());
                i += 2;
            }
        } else {
            pos.push(a.clone());
            i += 1;
        }
    }
    Ok((kv, flags, pos))
}

fn read_json(path: &str) -> Result<Value, String> {
    let bytes = fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("parse {path}: {e}"))
}

fn write_json(path: &str, value: &Value) -> Result<(), String> {
    let pretty = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(path, pretty + "\n").map_err(|e| format!("write {path}: {e}"))
}

fn decode_seed(h: &str) -> Result<[u8; 32], String> {
    let v = hex::decode(h.trim()).map_err(|e| format!("seed not hex: {e}"))?;
    v.try_into()
        .map_err(|_| "seed must decode to 32 bytes".to_string())
}

fn maybe_sign(catalog: Value, kv: &HashMap<String, String>) -> Result<(Value, bool), String> {
    match (kv.get("--sign-seed-hex"), kv.get("--key-id")) {
        (None, _) => Ok((catalog, false)),
        (Some(_), None) => Err("--key-id is required when --sign-seed-hex is given".into()),
        (Some(seed_hex), Some(key_id)) => {
            let seed = decode_seed(seed_hex)?;
            Ok((signing::sign_catalog(&catalog, &seed, key_id)?, true))
        }
    }
}

fn cmd_verify(args: &[String]) -> i32 {
    let (kv, _, pos) = match parse_flags(args, &[]) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return 2;
        }
    };
    let (Some(path), Some(key_id), Some(pubkey_b64)) =
        (pos.first(), kv.get("--key-id"), kv.get("--pubkey-b64"))
    else {
        eprintln!("{USAGE}");
        return 2;
    };
    let catalog = match read_json(path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("FAIL: {e}");
            return 1;
        }
    };
    let pk: [u8; 32] = match STANDARD
        .decode(pubkey_b64)
        .ok()
        .and_then(|v| v.try_into().ok())
    {
        Some(a) => a,
        None => {
            eprintln!("FAIL: --pubkey-b64 must be base64 of exactly 32 bytes");
            return 2;
        }
    };
    let trust = HashMap::from([(key_id.clone(), pk)]);
    match signing::verify_catalog(&catalog, &trust) {
        Ok(kid) => {
            println!("OK: catalog verified by {kid}");
            0
        }
        Err(e) => {
            eprintln!("FAIL: {e}");
            1
        }
    }
}

fn cmd_catalog(args: &[String]) -> i32 {
    let (kv, flags, _) = match parse_flags(args, &["--manifests-only"]) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return 2;
        }
    };
    let (Some(cogs_dir), Some(store_id), Some(generated_at), Some(out)) = (
        kv.get("--cogs-dir"),
        kv.get("--store-id"),
        kv.get("--generated-at"),
        kv.get("--out"),
    ) else {
        eprintln!("{USAGE}");
        return 2;
    };
    let manifests_only = flags.contains("--manifests-only");
    let artifacts_dir = kv.get("--artifacts-dir");
    if !manifests_only && artifacts_dir.is_none() {
        eprintln!("--artifacts-dir is required unless --manifests-only");
        return 2;
    }

    let built = catalog::build_catalog(
        Path::new(cogs_dir),
        artifacts_dir.map(Path::new),
        store_id,
        generated_at,
        manifests_only,
    );
    let catalog = match built {
        Ok(c) => c,
        Err(e) => {
            eprintln!("FAIL: {e}");
            return 1;
        }
    };
    let (catalog, signed) = match maybe_sign(catalog, &kv) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    if let Err(e) = write_json(out, &catalog) {
        eprintln!("FAIL: {e}");
        return 1;
    }
    let mode = if manifests_only {
        "manifests-only"
    } else {
        "full"
    };
    let status = if signed { "signed" } else { "UNSIGNED" };
    let n = catalog["cogs"].as_array().map_or(0, |a| a.len());
    println!("wrote {out} — {n} cog(s), {mode}, {status}");
    0
}

fn cmd_sign(args: &[String]) -> i32 {
    let (kv, _, _) = match parse_flags(args, &[]) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return 2;
        }
    };
    let (Some(inp), Some(out), Some(seed_hex), Some(key_id)) = (
        kv.get("--in"),
        kv.get("--out"),
        kv.get("--sign-seed-hex"),
        kv.get("--key-id"),
    ) else {
        eprintln!("{USAGE}");
        return 2;
    };
    let result = read_json(inp)
        .and_then(|c| decode_seed(seed_hex).map(|s| (c, s)))
        .and_then(|(c, s)| signing::sign_document(&c, &s, key_id))
        .and_then(|signed| write_json(out, &signed));
    match result {
        Ok(()) => {
            println!("OK: signed {out} ({key_id})");
            0
        }
        Err(e) => {
            eprintln!("FAIL: {e}");
            1
        }
    }
}

fn cmd_store_create(args: &[String]) -> i32 {
    let (kv, _, _) = match parse_flags(args, &[]) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return 2;
        }
    };
    let (Some(store_id), Some(name), Some(catalog_url), Some(key_id), Some(out)) = (
        kv.get("--store-id"),
        kv.get("--name"),
        kv.get("--catalog-url"),
        kv.get("--key-id"),
        kv.get("--out"),
    ) else {
        eprintln!("{USAGE}");
        return 2;
    };
    let description = kv.get("--description").map(String::as_str).unwrap_or("");

    // Either derive the public key from a private seed (and self-sign), or take a bare
    // --pubkey-b64 (unsigned). If both are given, they must agree.
    let (pubkey_b64, seed): (String, Option<[u8; 32]>) =
        match (kv.get("--sign-seed-hex"), kv.get("--pubkey-b64")) {
            (Some(hex_seed), pb) => {
                let seed = match decode_seed(hex_seed) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("FAIL: --sign-seed-hex {e}");
                        return 2;
                    }
                };
                let derived = signing::public_key_b64(&seed);
                if let Some(pb) = pb {
                    if pb != &derived {
                        eprintln!(
                        "FAIL: --pubkey-b64 does not match the key derived from --sign-seed-hex"
                    );
                        return 1;
                    }
                }
                (derived, Some(seed))
            }
            (None, Some(pb)) => (pb.clone(), None),
            (None, None) => {
                eprintln!("provide --sign-seed-hex (to self-sign) or --pubkey-b64");
                return 2;
            }
        };

    let doc = match store::build_store_info(
        store_id,
        name,
        description,
        catalog_url,
        key_id,
        &pubkey_b64,
    ) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("FAIL: {e}");
            return 1;
        }
    };
    let signed = seed.is_some();
    let doc = match seed {
        Some(s) => match signing::sign_document(&doc, &s, key_id) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("FAIL: {e}");
                return 1;
            }
        },
        None => doc,
    };
    if let Err(e) = store::validate(&doc) {
        eprintln!("FAIL: {e}");
        return 1;
    }
    if let Err(e) = write_json(out, &doc) {
        eprintln!("FAIL: {e}");
        return 1;
    }
    println!(
        "wrote {out} — store {store_id} ({}, key {key_id})",
        if signed { "self-signed" } else { "unsigned" }
    );
    0
}

fn cmd_export(args: &[String]) -> i32 {
    let (kv, _, _) = match parse_flags(args, &[]) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return 2;
        }
    };
    let (Some(catalog), Some(store_info), Some(artifacts_dir), Some(out), Some(generated_at)) = (
        kv.get("--catalog"),
        kv.get("--store-info"),
        kv.get("--artifacts-dir"),
        kv.get("--out"),
        kv.get("--generated-at"),
    ) else {
        eprintln!("{USAGE}");
        return 2;
    };

    // Same sign-options rule as the catalog generator: a seed needs a key id.
    let sign = match (kv.get("--sign-seed-hex"), kv.get("--key-id")) {
        (None, _) => None,
        (Some(_), None) => {
            eprintln!("--key-id is required when --sign-seed-hex is given");
            return 2;
        }
        (Some(seed_hex), Some(key_id)) => match decode_seed(seed_hex) {
            Ok(seed) => Some(bundle::SignOpts { seed, key_id }),
            Err(e) => {
                eprintln!("FAIL: {e}");
                return 2;
            }
        },
    };

    match bundle::export(
        Path::new(catalog),
        Path::new(store_info),
        Path::new(artifacts_dir),
        Path::new(out),
        generated_at,
        sign.as_ref(),
    ) {
        Ok(r) => {
            println!(
                "wrote bundle {} — {} artifact(s), manifest {}",
                r.out.display(),
                r.n_artifacts,
                if r.signed { "signed" } else { "UNSIGNED" }
            );
            0
        }
        Err(e) => {
            eprintln!("FAIL: {e}");
            1
        }
    }
}

fn cmd_import(args: &[String]) -> i32 {
    let (kv, _, pos) = match parse_flags(args, &[]) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return 2;
        }
    };
    let Some(dir) = pos.first().or_else(|| kv.get("--dir")) else {
        eprintln!("usage: gearbox import <bundle-dir> [--expect-fingerprint HEX]");
        return 2;
    };
    let expect = kv.get("--expect-fingerprint").map(String::as_str);
    match bundle::verify_bundle(Path::new(dir), expect) {
        Ok(r) => {
            println!(
                "OK: bundle verified — store {} signed by {} (fingerprint {})",
                r.store_id, r.key_id, r.fingerprint
            );
            println!(
                "    {} cog(s), {} artifact(s) re-hashed via file:// — same trust as online",
                r.n_cogs, r.n_artifacts
            );
            0
        }
        Err(e) => {
            eprintln!("FAIL: {e}");
            1
        }
    }
}

fn cmd_audit_append(args: &[String]) -> i32 {
    // `--detail` repeats, which `parse_flags` (a HashMap) cannot hold — pull every `--detail
    // VALUE` pair out first, then parse the remaining single-valued flags normally.
    let mut details: Vec<String> = Vec::new();
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--detail" {
            match args.get(i + 1) {
                Some(v) => {
                    details.push(v.clone());
                    i += 2;
                }
                None => {
                    eprintln!("--detail needs a key=value");
                    return 2;
                }
            }
        } else {
            rest.push(args[i].clone());
            i += 1;
        }
    }

    let (kv, _, _) = match parse_flags(&rest, &[]) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return 2;
        }
    };
    let (Some(log), Some(ts), Some(event), Some(subject)) = (
        kv.get("--log"),
        kv.get("--ts"),
        kv.get("--event"),
        kv.get("--subject"),
    ) else {
        eprintln!("usage: gearbox audit append --log FILE --ts TS --event EVENT --subject SUBJ [--detail k=v ...]");
        return 2;
    };
    let detail = match audit::parse_details(&details) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("FAIL: {e}");
            return 2;
        }
    };
    match audit::append(Path::new(log), ts, event, subject, detail) {
        Ok(rec) => {
            let seq = rec.get("seq").and_then(Value::as_i64).unwrap_or(-1);
            let self_hash = rec.get("self").and_then(Value::as_str).unwrap_or("");
            println!("appended seq {seq} ({event}) to {log} — self {self_hash}");
            0
        }
        Err(e) => {
            eprintln!("FAIL: {e}");
            1
        }
    }
}

fn cmd_audit_verify(args: &[String]) -> i32 {
    let (kv, _, pos) = match parse_flags(args, &[]) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return 2;
        }
    };
    let Some(log) = kv.get("--log").or_else(|| pos.first()) else {
        eprintln!("usage: gearbox audit verify --log FILE");
        return 2;
    };
    // A missing log file is a verify error (a typo'd path must not pass vacuously); an existing
    // but empty file is a valid 0-record chain (read_log handles that).
    if !Path::new(log).exists() {
        eprintln!("FAIL: no such audit log: {log}");
        return 1;
    }
    let records = match audit::read_log(Path::new(log)) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("FAIL: {e}");
            return 1;
        }
    };
    match audit::verify(&records) {
        Ok(report) => {
            println!(
                "OK: audit log verified — {} record(s), head self {}",
                report.n, report.head_self
            );
            0
        }
        Err(brk) => {
            eprintln!("FAIL: audit chain broken at {brk}");
            1
        }
    }
}

fn cmd_serve(args: &[String]) -> i32 {
    let (kv, _, _) = match parse_flags(args, &[]) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return 2;
        }
    };
    let Some(dir) = kv.get("--dir") else {
        eprintln!("usage: gearbox serve --dir DIR [--port N] [--auth-token TOKEN]");
        return 2;
    };
    let port: u16 = match kv.get("--port") {
        Some(s) => match s.parse() {
            Ok(p) => p,
            Err(_) => {
                eprintln!("--port must be 0..65535");
                return 2;
            }
        },
        None => 8088,
    };
    let auth = kv.get("--auth-token").map(String::as_str);
    match server::serve(Path::new(dir), port, auth) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("FAIL: {e}");
            1
        }
    }
}

fn cmd_store_verify(args: &[String]) -> i32 {
    let (kv, _, pos) = match parse_flags(args, &[]) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return 2;
        }
    };
    let Some(path) = pos.first().or_else(|| kv.get("--in")) else {
        eprintln!("usage: gearbox store-info verify <store.json>");
        return 2;
    };
    let doc = match read_json(path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("FAIL: {e}");
            return 1;
        }
    };
    if let Err(e) = store::validate(&doc) {
        eprintln!("FAIL: {e}");
        return 1;
    }
    match store::fingerprints(&doc) {
        Ok(fps) => {
            for (kid, fp) in fps {
                println!("key {kid}: fingerprint {fp}");
            }
        }
        Err(e) => {
            eprintln!("FAIL: {e}");
            return 1;
        }
    }
    if doc.get("signature").is_some() {
        match store::verify_self_signed(&doc) {
            Ok(kid) => {
                println!("OK: store.json self-signed and verified by {kid}");
                0
            }
            Err(e) => {
                eprintln!("FAIL: self-signature: {e}");
                1
            }
        }
    } else {
        println!("note: store.json is unsigned — confirm the fingerprint(s) above to pin (TOFU)");
        0
    }
}
