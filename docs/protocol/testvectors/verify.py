#!/usr/bin/env python3
"""Standalone verifier for the cog-store catalog signing test vector.

Re-canonicalizes catalog.signed.json (RFC 8785 JCS), then checks the Ed25519
signature against the embedded trusted test key. Requires: cryptography.

  python3 verify.py   ->  OK: signature valid over <N> canonical bytes

The trusted key here is a published throwaway. A real Seed embeds the official
release public key the same way.
"""
import json, base64, sys, pathlib
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
from cryptography.exceptions import InvalidSignature

TRUSTED = {  # key_id -> base64 raw 32-byte Ed25519 public key
    "gearbox-testvector-2026": "A6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=",
}

def jcs(obj):
    # RFC 8785 for ASCII-string + integer documents. Use a real JCS library for
    # general content (floats, non-ASCII).
    return json.dumps(obj, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=False).encode("utf-8")

def main():
    doc = json.loads(pathlib.Path(__file__).with_name("catalog.signed.json").read_text())
    sigblk = doc.pop("signature")
    if sigblk.get("alg") != "ed25519":
        print("FAIL: unexpected alg", sigblk.get("alg")); sys.exit(1)
    key_b64 = TRUSTED.get(sigblk["key_id"])
    if key_b64 is None:
        print("FAIL: untrusted key_id", sigblk["key_id"]); sys.exit(1)
    signing_input = jcs(doc)
    pub = Ed25519PublicKey.from_public_bytes(base64.b64decode(key_b64))
    try:
        pub.verify(base64.b64decode(sigblk["sig"]), signing_input)
    except InvalidSignature:
        print("FAIL: signature invalid"); sys.exit(1)
    print("OK: signature valid over", len(signing_input), "canonical bytes")

if __name__ == "__main__":
    main()
