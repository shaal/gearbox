//! Minimal reference store server (dev only): serves a store directory over HTTP so the full
//! add-store (TOFU) -> verify -> install loop can be exercised end to end. NOT a production
//! server — it is a dependency-free `std` implementation: thread-per-connection, HTTP/1.1
//! with `Connection: close`, optional bearer auth, and path-traversal guarded.
//!
//! Layout it expects under `dir`:
//!   store.json            (the store-info document, protocol §8)
//!   app-registry.json     (the catalog, protocol §3 — the store's catalog_url points here)
//!   cogs/<arch>/...        (binaries + assets, the catalog's relative artifact paths)

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};
use std::thread;

/// Bind a listener (use port 0 for an OS-assigned port; read it via `local_addr`).
pub fn bind(port: u16) -> Result<TcpListener, String> {
    TcpListener::bind(("127.0.0.1", port)).map_err(|e| format!("bind 127.0.0.1:{port}: {e}"))
}

/// Serve connections forever (one thread per connection).
pub fn run(listener: TcpListener, dir: PathBuf, auth_token: Option<String>) {
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let dir = dir.clone();
                let auth = auth_token.clone();
                thread::spawn(move || {
                    let _ = handle(s, &dir, auth.as_deref());
                });
            }
            Err(e) => eprintln!("accept: {e}"),
        }
    }
}

/// Bind + announce + serve (blocks).
pub fn serve(dir: &Path, port: u16, auth_token: Option<&str>) -> Result<(), String> {
    let listener = bind(port)?;
    let addr = listener.local_addr().map_err(|e| e.to_string())?;
    eprintln!(
        "gearbox serve: http://{addr} serving {} {}",
        dir.display(),
        if auth_token.is_some() {
            "(bearer auth required)"
        } else {
            "(open)"
        }
    );
    run(listener, dir.to_path_buf(), auth_token.map(String::from));
    Ok(())
}

fn handle(stream: TcpStream, dir: &Path, auth_token: Option<&str>) -> std::io::Result<()> {
    let mut writer = stream.try_clone()?;
    let mut reader = BufReader::new(stream);

    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }
    let mut it = request_line.split_whitespace();
    let method = it.next().unwrap_or("");
    let raw_path = it.next().unwrap_or("/");

    let mut auth_header: Option<String> = None;
    for _ in 0..100 {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let t = line.trim_end();
        if t.is_empty() {
            break;
        }
        if t.to_ascii_lowercase().starts_with("authorization:") {
            if let Some(colon) = t.find(':') {
                auth_header = Some(t[colon + 1..].trim().to_string());
            }
        }
    }

    if method != "GET" {
        return respond(&mut writer, 405, "text/plain", b"method not allowed");
    }
    if let Some(token) = auth_token {
        let expected = format!("Bearer {token}");
        if auth_header.as_deref() != Some(expected.as_str()) {
            return respond_status(
                &mut writer,
                401,
                "Unauthorized",
                &[
                    ("WWW-Authenticate", "Bearer"),
                    ("Content-Type", "text/plain"),
                ],
                b"unauthorized",
            );
        }
    }

    let path = raw_path.split('?').next().unwrap_or("/");
    if path == "/health" {
        return respond(&mut writer, 200, "text/plain", b"ok");
    }
    let rel = path.trim_start_matches('/');
    let safe = match safe_join(dir, rel) {
        Some(p) => p,
        None => return respond(&mut writer, 403, "text/plain", b"forbidden"),
    };
    match std::fs::read(&safe) {
        Ok(body) => respond(&mut writer, 200, content_type(&safe), &body),
        Err(_) => respond(&mut writer, 404, "text/plain", b"not found"),
    }
}

/// Join `rel` under `dir`, rejecting any `..` / absolute / prefix component (no traversal).
fn safe_join(dir: &Path, rel: &str) -> Option<PathBuf> {
    let mut p = dir.to_path_buf();
    for comp in Path::new(rel).components() {
        match comp {
            Component::Normal(c) => p.push(c),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(p)
}

fn content_type(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()) {
        Some("json") => "application/json",
        Some("html") => "text/html",
        _ => "application/octet-stream",
    }
}

fn respond(stream: &mut TcpStream, code: u16, ct: &str, body: &[u8]) -> std::io::Result<()> {
    respond_status(stream, code, reason(code), &[("Content-Type", ct)], body)
}

fn respond_status(
    stream: &mut TcpStream,
    code: u16,
    reason: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> std::io::Result<()> {
    let mut h = format!("HTTP/1.1 {code} {reason}\r\n");
    for (k, v) in headers {
        h.push_str(&format!("{k}: {v}\r\n"));
    }
    h.push_str(&format!(
        "Content-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    ));
    stream.write_all(h.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn reason(code: u16) -> &'static str {
    match code {
        200 => "OK",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "OK",
    }
}
