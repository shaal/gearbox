//! `gearbox` CLI — native reference for the cog-store protocol.
//!
//! Phase-1 subcommand: `verify` (the seed B4 / A4-pre-upload check). Catalog generation +
//! signing parity with the Python `tools/` is the next slice.

use std::collections::HashMap;
use std::fs;
use std::process::exit;

use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value;

use gearbox::signing;

const USAGE: &str = "usage: gearbox verify <catalog.json> --key-id <ID> --pubkey-b64 <B64>";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("verify") => exit(cmd_verify(&args[2..])),
        _ => {
            eprintln!("{USAGE}");
            exit(2);
        }
    }
}

fn cmd_verify(args: &[String]) -> i32 {
    let mut path: Option<String> = None;
    let mut key_id: Option<String> = None;
    let mut pubkey_b64: Option<String> = None;

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--key-id" => key_id = it.next().cloned(),
            "--pubkey-b64" => pubkey_b64 = it.next().cloned(),
            p if !p.starts_with("--") => path = Some(p.to_string()),
            other => {
                eprintln!("unknown argument {other:?}\n{USAGE}");
                return 2;
            }
        }
    }

    let (Some(path), Some(key_id), Some(pubkey_b64)) = (path, key_id, pubkey_b64) else {
        eprintln!("{USAGE}");
        return 2;
    };

    let catalog: Value = match fs::read(&path)
        .map_err(|e| e.to_string())
        .and_then(|b| serde_json::from_slice(&b).map_err(|e| e.to_string()))
    {
        Ok(v) => v,
        Err(e) => {
            eprintln!("FAIL: read/parse {path}: {e}");
            return 1;
        }
    };

    let pk: [u8; 32] = match STANDARD.decode(&pubkey_b64).ok().and_then(|v| v.try_into().ok()) {
        Some(a) => a,
        None => {
            eprintln!("FAIL: --pubkey-b64 must be base64 of exactly 32 bytes");
            return 2;
        }
    };

    let mut trust: HashMap<String, [u8; 32]> = HashMap::new();
    trust.insert(key_id, pk);

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
