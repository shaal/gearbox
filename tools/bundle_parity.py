#!/usr/bin/env python3
"""Cross-impl parity oracle for the air-gap bundle manifest (Phase 3, T0-A).

Given a bundle directory produced by the Rust `gearbox export`, independently rebuild and
sign the bundle manifest in Python and assert the Ed25519 signature is **byte-identical** —
the same guarantee the catalog parity job makes, extended to the signed bundle manifest. Then
verify the whole Rust-produced bundle end-to-end in Python (Rust signs -> Python verifies).

  python3 tools/bundle_parity.py <bundle-dir>

Reads TV_SEED, TV_KEY_ID, TV_GENERATED_AT from the environment (the published throwaway test
key the CI parity job already uses). Exit 0 on agreement; non-zero on any divergence.
"""
import json
import os
import pathlib
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cogstore import bundle, signing  # noqa: E402


def main(argv=None) -> int:
    argv = argv if argv is not None else sys.argv[1:]
    if len(argv) != 1:
        print("usage: bundle_parity.py <bundle-dir>", file=sys.stderr)
        return 2
    d = pathlib.Path(argv[0])
    seed = bytes.fromhex(os.environ["TV_SEED"])
    key_id = os.environ["TV_KEY_ID"]
    generated_at = os.environ["TV_GENERATED_AT"]

    # Python rebuilds the manifest over the SAME on-disk bytes and signs it.
    manifest = bundle.build_manifest(d, generated_at)
    sig_py = signing.sign_catalog(manifest, seed=seed, key_id=key_id)["signature"]["sig"]
    sig_rs = json.loads((d / "manifest.json").read_text())["signature"]["sig"]
    print(f"py: {sig_py}")
    print(f"rs: {sig_rs}")
    if sig_py != sig_rs:
        print("::error::Rust/Python bundle manifest signatures diverge", file=sys.stderr)
        return 1

    # Rust signs -> Python verifies the whole bundle (store + catalog + manifest + artifacts).
    report = bundle.verify_bundle(d)
    print(f"Python verified the Rust bundle: {report}")
    print("Bundle manifest signatures are byte-identical.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
