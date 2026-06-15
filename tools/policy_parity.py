#!/usr/bin/env python3
"""Cross-impl parity oracle for the managed policy document (Phase 3, T0-C).

Given a `policy.json` produced by the Rust `gearbox policy create`, this re-signs the same
document body in Python and asserts the Ed25519 signature is **byte-identical** (the same
guarantee the catalog/bundle parity steps make), then verifies the Rust-signed policy
(Rust signs -> Python verifies).

  python3 tools/policy_parity.py <policy.json>

Reads TV_SEED, TV_KEY_ID from the environment (the published throwaway test key). Exit 0 on
agreement; non-zero on any divergence.
"""
import json
import os
import pathlib
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cogstore import policy, signing  # noqa: E402


def main(argv=None) -> int:
    argv = argv if argv is not None else sys.argv[1:]
    if len(argv) != 1:
        print("usage: policy_parity.py <policy.json>", file=sys.stderr)
        return 2
    seed = bytes.fromhex(os.environ["TV_SEED"])
    key_id = os.environ["TV_KEY_ID"]
    rs = json.loads(pathlib.Path(argv[0]).read_text())

    # Re-sign the Rust document's body in Python; the signature must match byte-for-byte.
    body = {k: v for k, v in rs.items() if k != "signature"}
    sig_py = policy.sign(body, seed=seed, key_id=key_id)["signature"]["sig"]
    sig_rs = rs["signature"]["sig"]
    print(f"py: {sig_py}")
    print(f"rs: {sig_rs}")
    if sig_py != sig_rs:
        print("::error::Rust/Python policy signatures diverge", file=sys.stderr)
        return 1

    # Rust signs -> Python verifies (fail-closed gate).
    kid = policy.verify_signed(rs, {key_id: signing.public_key_b64(seed)})
    print(f"Python verified the Rust policy (signed by {kid}); signatures byte-identical.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
