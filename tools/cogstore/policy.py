"""Managed-mode policy (Phase 3, T0-C) — the Python cross-check oracle.

The native producer/consumer is the Rust `gearbox policy create/verify/check`
(`crates/gearbox/src/policy.rs`); this module is the parity oracle for the **signed document**
(the part that touches signing + canonicalization), exactly as `bundle.py` is for the bundle
manifest. The projection/resolution (`Policy::project` + the resolver) is Rust-only and is
covered by the crate's tests — Python keeps the signed `policy.json` honest byte-for-byte.

`build_policy()` MUST reproduce `policy.canonical.json` (and, signed, the `signature.sig`) of
the frozen vector in `docs/protocol/testvectors/policy/`.
"""
from . import signing

OFFICIAL_STORE_ID = "cognitum-official"


def build_policy(allow_stores=None, deny_public=False, forced_pins=None,
                 allow_user_add_store=False) -> dict:
    """Build an unsigned managed policy document (the JCS subset: bools, ints, ASCII keys)."""
    return {
        "schema_version": 1,
        "managed": True,
        "allow_stores": list(allow_stores or []),
        "deny_public": bool(deny_public),
        "forced_pins": dict(forced_pins or {}),
        "allow_user_add_store": bool(allow_user_add_store),
    }


def validate(doc: dict) -> None:
    """Raise ValueError if `doc` is not a spec-valid policy."""
    def req(cond, msg):
        if not cond:
            raise ValueError(f"invalid policy: {msg}")

    req(doc.get("schema_version") == 1, "schema_version must be 1")
    for f in ("managed", "deny_public", "allow_user_add_store"):
        req(isinstance(doc.get(f), bool), f"{f} must be a boolean")
    req(isinstance(doc.get("allow_stores"), list), "allow_stores must be a list")
    req(all(isinstance(s, str) for s in doc["allow_stores"]),
        "allow_stores entries must be strings")
    req(isinstance(doc.get("forced_pins"), dict), "forced_pins must be an object")
    req(all(isinstance(v, str) for v in doc["forced_pins"].values()),
        "forced_pins values must be store id strings")


def sign(doc: dict, *, seed: bytes, key_id: str) -> dict:
    """Sign a policy with the org policy key (same envelope as the catalog, §7.2)."""
    validate(doc)
    return signing.sign_catalog(doc, seed=seed, key_id=key_id)


def verify_signed(doc: dict, trusted: dict) -> str:
    """Verify a policy against a trusted org-key map; return the signing key id.

    Fail-closed: a malformed schema or a bad signature raises (ValueError / InvalidSignature).
    """
    validate(doc)
    return signing.verify_catalog(doc, trusted)
