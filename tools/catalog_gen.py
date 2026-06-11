#!/usr/bin/env python3
"""Generate a spec-valid (optionally signed) app-registry.json from a cog tree.

Reference implementation of gearbox#2 — the catalog generator that cognitum-one/cogs
CI invokes as a publish step. See ../docs/protocol/cog-store-protocol.md. The eventual
native Rust `gearbox` CLI is gearbox#3; both implement the same protocol and are pinned
by the test vector in ../docs/protocol/testvectors/.

Example:
  python3 catalog_gen.py \\
      --cogs-dir ../../cogs/src/cogs \\
      --artifacts-dir dist \\
      --store-id cognitum-official \\
      --generated-at 2026-06-10T00:00:00Z \\
      --out app-registry.json \\
      --sign-seed-hex "$(cat "$STORE_SIGNING_KEY")" --key-id cognitum-release-2026

The signing seed is a 32-byte Ed25519 seed in hex. In CI it comes from the
STORE_SIGNING_KEY secret and is never committed. Omit --sign-seed-hex to emit an
unsigned catalog (e.g. for a preview/staging channel).
"""
import argparse
import json
import os
import pathlib
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cogstore import catalog as cat        # noqa: E402
from cogstore import signing               # noqa: E402


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(
        description="Generate a cog-store catalog (app-registry.json).")
    ap.add_argument("--cogs-dir", required=True,
                    help="tree of <cog>/cog.toml manifests (e.g. cogs/src/cogs)")
    ap.add_argument("--artifacts-dir", required=True,
                    help="staging dir holding built binaries at cogs/<arch>/<binary>")
    ap.add_argument("--store-id", required=True)
    ap.add_argument("--generated-at", required=True,
                    help="RFC3339 timestamp; pass it in so catalogs stay reproducible")
    ap.add_argument("--out", required=True)
    ap.add_argument("--sign-seed-hex",
                    help="32-byte Ed25519 seed (hex) to sign with; omit for unsigned")
    ap.add_argument("--key-id", help="key_id for the signature (required with --sign-seed-hex)")
    args = ap.parse_args(argv)

    catalog = cat.build_catalog(
        args.cogs_dir, args.artifacts_dir,
        store_id=args.store_id, generated_at=args.generated_at)

    signed = False
    if args.sign_seed_hex:
        if not args.key_id:
            ap.error("--key-id is required when --sign-seed-hex is given")
        seed = bytes.fromhex(args.sign_seed_hex.strip())
        if len(seed) != 32:
            ap.error("--sign-seed-hex must decode to exactly 32 bytes")
        catalog = signing.sign_catalog(catalog, seed=seed, key_id=args.key_id)
        signed = True

    out = pathlib.Path(args.out)
    out.write_text(json.dumps(catalog, indent=2) + "\n")
    status = f"signed ({args.key_id})" if signed else "UNSIGNED"
    print(f"wrote {out} — {len(catalog['cogs'])} cog(s), {status}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
