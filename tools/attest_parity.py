#!/usr/bin/env python3
"""Cross-impl parity oracle for the provenance+SBOM attestation (T1-shaped).

Given an `attestation.json` produced by the Rust `gearbox attest create`, re-sign the same
document body in Python and assert the Ed25519 signature is **byte-identical** (the same
guarantee the catalog/bundle/policy parity steps make), then verify the Rust-signed
attestation (Rust signs -> Python verifies).

  python3 tools/attest_parity.py <attestation.json>

Reads TV_SEED, TV_KEY_ID from the environment. Exit 0 on agreement; non-zero on divergence.
"""
import json
import os
import pathlib
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cogstore import attest, signing  # noqa: E402


def main(argv=None) -> int:
    argv = argv if argv is not None else sys.argv[1:]
    if len(argv) != 1:
        print("usage: attest_parity.py <attestation.json>", file=sys.stderr)
        return 2
    seed = bytes.fromhex(os.environ["TV_SEED"])
    key_id = os.environ["TV_KEY_ID"]
    rs = json.loads(pathlib.Path(argv[0]).read_text())

    body = {k: v for k, v in rs.items() if k != "signature"}
    sig_py = attest.sign(body, seed=seed, key_id=key_id)["signature"]["sig"]
    sig_rs = rs["signature"]["sig"]
    print(f"py: {sig_py}")
    print(f"rs: {sig_rs}")
    if sig_py != sig_rs:
        print("::error::Rust/Python attestation signatures diverge", file=sys.stderr)
        return 1

    kid = attest.verify(rs, {key_id: signing.public_key_b64(seed)})
    print(f"Python verified the Rust attestation (signed by {kid}); signatures byte-identical.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
