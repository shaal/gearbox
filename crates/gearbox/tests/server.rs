//! In-process tests for the reference store server: serves files, 404s on missing paths,
//! blocks traversal, and enforces bearer auth. Uses a tiny raw-TCP HTTP client.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use gearbox::server;

static N: AtomicUsize = AtomicUsize::new(0);

fn tmpdir() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "gbsrv-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}

/// Minimal HTTP GET: returns (status_code, body).
fn get(addr: SocketAddr, path: &str, bearer: Option<&str>) -> (u16, Vec<u8>) {
    let mut s = TcpStream::connect(addr).unwrap();
    let mut req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n");
    if let Some(t) = bearer {
        req.push_str(&format!("Authorization: Bearer {t}\r\n"));
    }
    req.push_str("\r\n");
    s.write_all(req.as_bytes()).unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).unwrap();
    let sep = buf.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let status_line: &[u8] = &buf[..buf.iter().position(|&b| b == b'\r').unwrap()];
    let code: u16 = std::str::from_utf8(status_line)
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    (code, buf[sep + 4..].to_vec())
}

fn start(dir: PathBuf, auth: Option<String>) -> SocketAddr {
    let listener = server::bind(0).unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || server::run(listener, dir, auth));
    addr
}

#[test]
fn serves_files_404s_and_blocks_traversal() {
    let dir = tmpdir();
    std::fs::create_dir_all(dir.join("cogs/arm")).unwrap();
    std::fs::write(dir.join("store.json"), br#"{"store_id":"demo"}"#).unwrap();
    std::fs::write(dir.join("cogs/arm/cog-x-arm"), b"binary-bytes").unwrap();
    let addr = start(dir, None);

    let (code, body) = get(addr, "/store.json", None);
    assert_eq!(code, 200);
    assert_eq!(body, br#"{"store_id":"demo"}"#);

    assert_eq!(
        get(addr, "/cogs/arm/cog-x-arm", None),
        (200, b"binary-bytes".to_vec())
    );
    assert_eq!(get(addr, "/missing.json", None).0, 404);
    assert!(matches!(get(addr, "/../../etc/passwd", None).0, 403 | 404));
    assert_eq!(get(addr, "/health", None).0, 200);
}

#[test]
fn enforces_bearer_auth() {
    let dir = tmpdir();
    std::fs::write(dir.join("store.json"), b"{}").unwrap();
    let addr = start(dir, Some("s3cr3t".to_string()));

    assert_eq!(get(addr, "/store.json", None).0, 401);
    assert_eq!(get(addr, "/store.json", Some("wrong")).0, 401);
    assert_eq!(get(addr, "/store.json", Some("s3cr3t")).0, 200);
}
