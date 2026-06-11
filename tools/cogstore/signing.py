"""Ed25519 signing / verification over JCS canonical bytes (protocol §7).

The signing input is the catalog with its own `signature` member removed, then
JCS-canonicalized (see `jcs.canonical`). This matches the frozen test vector.
"""
import base64

from cryptography.hazmat.primitives.asymmetric.ed25519 import (
    Ed25519PrivateKey, Ed25519PublicKey)
from cryptography.exceptions import InvalidSignature

from . import jcs

ALG = "ed25519"


def _body(catalog: dict) -> dict:
    """The catalog without its `signature` member — i.e. the signing input source."""
    return {k: v for k, v in catalog.items() if k != "signature"}


def sign_catalog(catalog: dict, *, seed: bytes, key_id: str) -> dict:
    """Return a copy of `catalog` carrying a `signature` member (protocol §7.2)."""
    body = _body(catalog)
    sig = Ed25519PrivateKey.from_private_bytes(seed).sign(jcs.canonical(body))
    out = dict(body)
    out["signature"] = {"key_id": key_id, "alg": ALG,
                        "sig": base64.b64encode(sig).decode()}
    return out


def verify_catalog(catalog: dict, trusted: dict) -> str:
    """Verify the catalog signature; return the key_id used.

    `trusted` maps key_id -> base64 raw 32-byte Ed25519 public key. Raises
    cryptography.exceptions.InvalidSignature on any failure.
    """
    sigblk = catalog.get("signature")
    if not sigblk:
        raise InvalidSignature("no signature member")
    if sigblk.get("alg") != ALG:
        raise InvalidSignature(f"unexpected alg {sigblk.get('alg')!r}")
    kid = sigblk.get("key_id")
    if kid not in trusted:
        raise InvalidSignature(f"untrusted key_id {kid!r}")
    pub = Ed25519PublicKey.from_public_bytes(base64.b64decode(trusted[kid]))
    pub.verify(base64.b64decode(sigblk["sig"]), jcs.canonical(_body(catalog)))
    return kid


def public_key_b64(seed: bytes) -> str:
    """Base64 of the raw 32-byte public key for an Ed25519 seed."""
    raw = Ed25519PrivateKey.from_private_bytes(seed).public_key().public_bytes_raw()
    return base64.b64encode(raw).decode()
