#!/usr/bin/env python3
"""Verify a signed cog-store catalog against a trusted Ed25519 key (protocol §7).

Used by the publish pipeline's verify-before-upload step (seed A4): after signing the
official catalog, confirm it verifies under the official public key before it ships.

  python3 verify_catalog.py app-registry.json --key-id cognitum-release-2026 --pubkey-b64 <b64>
  python3 verify_catalog.py app-registry.json --trust-file trust.json   # {"key_id": "<b64>"}

Exit 0 and prints the signing key_id on success; non-zero on any failure.
"""
import argparse
import json
import os
import pathlib
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cogstore import signing                       # noqa: E402
from cryptography.exceptions import InvalidSignature  # noqa: E402


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Verify a signed cog-store catalog.")
    ap.add_argument("catalog", help="path to app-registry.json")
    ap.add_argument("--key-id", help="trusted key_id (with --pubkey-b64)")
    ap.add_argument("--pubkey-b64", help="base64 raw 32-byte Ed25519 public key")
    ap.add_argument("--trust-file", help="JSON mapping key_id -> base64 raw public key")
    args = ap.parse_args(argv)

    if args.trust_file:
        trusted = json.loads(pathlib.Path(args.trust_file).read_text())
    elif args.key_id and args.pubkey_b64:
        trusted = {args.key_id: args.pubkey_b64}
    else:
        ap.error("provide --trust-file, or both --key-id and --pubkey-b64")

    catalog = json.loads(pathlib.Path(args.catalog).read_text())
    try:
        kid = signing.verify_catalog(catalog, trusted)
    except InvalidSignature as e:
        print(f"FAIL: {e}", file=sys.stderr)
        return 1
    print(f"OK: catalog verified by {kid}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
