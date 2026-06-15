"""Air-gap bundle manifest (Phase 3, T0-A) — the Python cross-check oracle.

The native producer/consumer is the Rust `gearbox export` / `gearbox import`
(`crates/gearbox/src/bundle.rs`); this module is the parity oracle that keeps the signed
**bundle manifest** honest byte-for-byte, exactly as `catalog.py`/`signing.py` do for the
catalog. `build_manifest()` MUST reproduce `manifest.canonical.json` (and, signed, the
`signature.sig`) of the frozen vector in `docs/protocol/testvectors/bundle/`.

The manifest is the JCS subset (integer numbers, ASCII keys) and is signed with the **same**
envelope and key as the catalog (`signing.sign_catalog` works on any dict), so import has one
trust anchor and every file is hashed — nothing is trusted by path.
"""
import base64
import hashlib
import json
import pathlib

from . import catalog as cat
from . import signing

STORE_FILE = "store.json"
CATALOG_FILE = "app-registry.json"
MANIFEST_FILE = "manifest.json"
ARTIFACTS_DIR = "artifacts"


def _sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _store_trust(store_doc: dict) -> dict:
    """store.json listed keys as a {key_id: pubkey_b64} trust map (TOFU anchor)."""
    return {k["key_id"]: k["pubkey_b64"] for k in store_doc["keys"]}


def _store_fingerprint(pubkey_b64: str) -> str:
    """SHA-256 fingerprint (lowercase hex) of a raw 32-byte key (protocol §7.3)."""
    return hashlib.sha256(base64.b64decode(pubkey_b64)).hexdigest()


def _artifact_paths(catalog: dict) -> list[tuple[str, str, int]]:
    """(store-relative path, sha256, size) for every artifact a full catalog references.

    Mirrors `catalog::artifact_paths` in the Rust crate. Raises on a `pending` binary —
    a manifests-only catalog has no bytes to bundle.
    """
    out = []
    for c in catalog.get("cogs", []):
        cid = c.get("id", "?")
        for v in c.get("versions", []):
            arts = v.get("artifacts") or {}
            binary = arts.get("binary") or {}
            if binary.get("pending") is True:
                raise ValueError(
                    f"catalog: {cid} binary is pending — a manifests-only catalog cannot be bundled")
            out.append((binary["path"], binary["sha256"], int(binary["size"])))
            for a in arts.get("assets", []):
                out.append((a["path"], a["sha256"], int(a["size"])))
    return out


def build_manifest(bundle_dir, generated_at: str) -> dict:
    """Build the UNSIGNED bundle manifest for an on-disk bundle directory.

    `files[]` = the two signed docs + every catalog artifact, each hashed from the bytes on
    disk and **sorted by path** for a stable, reproducible document. Byte-for-byte equal to
    the Rust `bundle::build_manifest` for the same directory.
    """
    d = pathlib.Path(bundle_dir)
    catalog = json.loads((d / CATALOG_FILE).read_bytes())
    cat.validate(catalog)
    store_doc = json.loads((d / STORE_FILE).read_bytes())

    store_id = catalog["store_id"]
    if store_doc.get("store_id") != store_id:
        raise ValueError(
            f"store_id mismatch: catalog {store_id!r} vs store.json {store_doc.get('store_id')!r}")

    rel_paths = {CATALOG_FILE, STORE_FILE}
    for path, _sha, _size in _artifact_paths(catalog):
        rel_paths.add(f"{ARTIFACTS_DIR}/{path}")

    files = []
    for rel in sorted(rel_paths):
        data = (d / rel).read_bytes()
        files.append({"path": rel, "sha256": _sha256_hex(data), "size": len(data)})

    return {
        "schema_version": 1,
        "store_id": store_id,
        "generated_at": generated_at,
        "catalog_sha256": _sha256_hex((d / CATALOG_FILE).read_bytes()),
        "files": files,
    }


def export(catalog_path, store_path, artifacts_dir, out_dir, generated_at: str,
           *, seed: bytes = None, key_id: str = None) -> dict:
    """Lay out a bundle directory and write a (optionally signed) manifest.json.

    Copies the catalog and store-info **byte-for-byte** (so `catalog_sha256` is the exact
    served bytes), re-hashes each artifact against the catalog before writing, then signs the
    manifest with the same envelope as the catalog.
    """
    out = pathlib.Path(out_dir)
    catalog_bytes = pathlib.Path(catalog_path).read_bytes()
    catalog = json.loads(catalog_bytes)
    cat.validate(catalog)
    store_bytes = pathlib.Path(store_path).read_bytes()
    json.loads(store_bytes)  # store.json must at least parse before we copy it

    (out / ARTIFACTS_DIR).mkdir(parents=True, exist_ok=True)
    (out / STORE_FILE).write_bytes(store_bytes)
    (out / CATALOG_FILE).write_bytes(catalog_bytes)

    artifacts = _artifact_paths(catalog)
    for path, sha, _size in artifacts:
        data = (pathlib.Path(artifacts_dir) / path).read_bytes()
        got = _sha256_hex(data)
        if got != sha:
            raise ValueError(
                f"staged artifact {path} sha256 {got} != catalog {sha} — refusing to bundle a mismatch")
        dst = out / ARTIFACTS_DIR / path
        dst.parent.mkdir(parents=True, exist_ok=True)
        dst.write_bytes(data)

    manifest = build_manifest(out, generated_at)
    if seed is not None:
        if not key_id:
            raise ValueError("key_id is required when seed is given")
        manifest = signing.sign_catalog(manifest, seed=seed, key_id=key_id)
    (out / MANIFEST_FILE).write_text(json.dumps(manifest, indent=2) + "\n")
    return manifest


def verify_bundle(bundle_dir, expect_fingerprint: str = None) -> dict:
    """Verify a bundle through the same trust path as an online install (see bundle.rs §1-5).

    Returns a small report dict on success; raises ValueError / InvalidSignature on any
    signature failure, hash mismatch, or a single flipped artifact byte.
    """
    d = pathlib.Path(bundle_dir)
    store_doc = json.loads((d / STORE_FILE).read_bytes())
    trust = _store_trust(store_doc)
    if "signature" in store_doc:
        signing.verify_catalog(store_doc, trust)  # self-signature (same envelope)
    fingerprints = {kid: _store_fingerprint(pb) for kid, pb in trust.items()}
    if expect_fingerprint is not None and expect_fingerprint not in fingerprints.values():
        raise ValueError(
            f"no store key matches the pinned fingerprint {expect_fingerprint}")

    catalog = json.loads((d / CATALOG_FILE).read_bytes())
    cat.validate(catalog)
    catalog_key = signing.verify_catalog(catalog, trust)

    manifest = json.loads((d / MANIFEST_FILE).read_bytes())
    manifest_key = signing.verify_catalog(manifest, trust)  # same envelope as the catalog
    if manifest_key != catalog_key:
        raise ValueError(
            f"manifest signed by {manifest_key!r} but catalog by {catalog_key!r} — expected one anchor")

    catalog_sha256 = _sha256_hex((d / CATALOG_FILE).read_bytes())
    if manifest.get("catalog_sha256") != catalog_sha256:
        raise ValueError("manifest catalog_sha256 does not match app-registry.json")

    listed = {}
    for f in manifest["files"]:
        path = f["path"]
        data = (d / path).read_bytes()
        if _sha256_hex(data) != f["sha256"]:
            raise ValueError(f"file {path}: sha256 mismatch (bundle tampered)")
        if len(data) != f["size"]:
            raise ValueError(f"file {path}: size mismatch (bundle tampered)")
        listed[path] = f["sha256"]
    for required in (STORE_FILE, CATALOG_FILE):
        if required not in listed:
            raise ValueError(f"manifest does not list {required}")

    artifacts = _artifact_paths(catalog)
    for path, sha, _size in artifacts:
        rel = f"{ARTIFACTS_DIR}/{path}"
        if _sha256_hex((d / rel).read_bytes()) != sha:
            raise ValueError(f"artifact {path}: sha256 does not match catalog")
        if listed.get(rel) != sha:
            raise ValueError(f"artifact {path} not covered by the signed manifest")

    return {
        "store_id": catalog["store_id"],
        "key_id": catalog_key,
        "fingerprint": fingerprints.get(catalog_key),
        "n_cogs": len(catalog.get("cogs", [])),
        "n_artifacts": len(artifacts),
    }
