"""Provenance + SBOM attestation (T1-shaped) — the Python cross-check oracle.

The native producer/consumer is the Rust `gearbox attest create/verify`
(`crates/gearbox/src/attest.rs`); this module is the parity oracle for the signed document,
exactly as `bundle.py`/`policy.py` are for theirs. `build_attestation()` MUST reproduce
`attestation.canonical.json` (and, signed, the `signature.sig`) of the frozen vector in
`docs/protocol/testvectors/attestation/`.

Field names are SLSA/SPDX-shaped (`subject`+`sha256`, `builder`/`source_*`, `packages[]`) so a
later "emit real in-toto/SPDX" step is a reshaping, not a redesign. Two independent guards: the
whole document (incl. `subject.sha256`) is signed, and `check_artifact` re-hashes the artifact.
"""
import hashlib

from . import signing

SHA256_HEX_LEN = 64


def _is_sha256_hex(s) -> bool:
    return isinstance(s, str) and len(s) == SHA256_HEX_LEN and all(c in "0123456789abcdef" for c in s)


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def build_attestation(subject: dict, provenance: dict, packages=None) -> dict:
    """Build an unsigned attestation (JCS subset: one int, strings, ASCII keys)."""
    return {
        "schema_version": 1,
        "subject": {
            "cog": subject["cog"],
            "version": subject["version"],
            "artifact": subject["artifact"],
            "sha256": subject["sha256"],
        },
        "provenance": {
            "builder": provenance["builder"],
            "source_repo": provenance["source_repo"],
            "source_commit": provenance["source_commit"],
            "built_at": provenance["built_at"],
        },
        "sbom": {
            "packages": [
                {
                    "name": p["name"],
                    "version": p["version"],
                    "license": p["license"],
                    "sha256": p["sha256"],
                }
                for p in (packages or [])
            ]
        },
    }


def validate(doc: dict) -> None:
    """Raise ValueError if `doc` is not a spec-valid attestation."""
    def req(cond, msg):
        if not cond:
            raise ValueError(f"invalid attestation: {msg}")

    req(doc.get("schema_version") == 1, "schema_version must be 1")
    s = doc.get("subject")
    req(isinstance(s, dict), "subject must be an object")
    for f in ("cog", "version", "artifact"):
        req(isinstance(s.get(f), str) and s[f], f"subject.{f} missing")
    req(_is_sha256_hex(s.get("sha256")), "subject.sha256 must be 64 lowercase hex")
    prov = doc.get("provenance")
    req(isinstance(prov, dict), "provenance must be an object")
    for f in ("builder", "source_repo", "source_commit", "built_at"):
        req(isinstance(prov.get(f), str) and prov[f], f"provenance.{f} missing")
    pkgs = (doc.get("sbom") or {}).get("packages")
    req(isinstance(pkgs, list), "sbom.packages must be a list")
    for i, p in enumerate(pkgs):
        req(isinstance(p, dict), f"sbom.packages[{i}] not an object")
        for f in ("name", "version", "license"):
            req(isinstance(p.get(f), str) and p[f], f"sbom.packages[{i}].{f} missing")
        req(_is_sha256_hex(p.get("sha256")), f"sbom.packages[{i}].sha256 must be 64 lowercase hex")


def sign(doc: dict, *, seed: bytes, key_id: str) -> dict:
    """Sign an attestation (same envelope as the catalog, §7.2)."""
    validate(doc)
    return signing.sign_catalog(doc, seed=seed, key_id=key_id)


def verify(doc: dict, trusted: dict) -> str:
    """Verify the signature against a trusted key map (after a schema check); return the key id."""
    validate(doc)
    return signing.verify_catalog(doc, trusted)


def check_artifact(doc: dict, artifact_bytes: bytes) -> None:
    """Check that `artifact_bytes` are the bytes this attestation is about (the digest binding)."""
    want = doc["subject"]["sha256"]
    got = sha256_hex(artifact_bytes)
    if got != want:
        raise ValueError(f"artifact does not match attestation subject: sha256 {got} != {want}")
