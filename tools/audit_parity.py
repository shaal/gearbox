#!/usr/bin/env python3
"""Cross-impl parity oracle for the audit hash chain (Phase 3, T0-B).

Given an `audit.jsonl` produced by the Rust `gearbox audit append`, this:
  1. verifies the chain in Python (Rust produces -> Python verifies),
  2. rebuilds the log by re-appending each record's own fields and asserts the result is
     **byte-identical** — proving the two impls' JCS + sha2 hashing agree, the same guarantee
     the catalog/bundle parity steps make, and
  3. writes the Python-rebuilt copy to `<log>.rebuilt.jsonl` so the caller can have the Rust
     binary verify a Python-produced log (Python produces -> Rust verifies).

  python3 tools/audit_parity.py <log.jsonl> [head.json]

If a Rust-signed `head.json` is given, also re-sign its body in Python (asserting the signature
is byte-identical) and verify it against the log. Reads TV_SEED/TV_KEY_ID from the environment
for that step. Exit 0 on agreement; non-zero on any divergence.
"""
import json
import os
import pathlib
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cogstore import audit, signing  # noqa: E402


def main(argv=None) -> int:
    argv = argv if argv is not None else sys.argv[1:]
    if not (1 <= len(argv) <= 2):
        print("usage: audit_parity.py <log.jsonl> [head.json]", file=sys.stderr)
        return 2
    log = pathlib.Path(argv[0])

    records = audit.read_log(log)
    report = audit.verify(records)
    print(f"Python verified the Rust log: {report}")

    rebuilt = pathlib.Path(str(log) + ".rebuilt.jsonl")  # append, don't replace the .jsonl suffix
    if rebuilt.exists():
        rebuilt.unlink()
    for r in records:
        audit.append(rebuilt, r["ts"], r["event"], r["subject"], r["detail"])

    if rebuilt.read_bytes() != log.read_bytes():
        print("::error::Rust/Python audit logs diverge (hash chain not byte-identical)",
              file=sys.stderr)
        return 1
    print(f"Audit logs are byte-identical; wrote Python-rebuilt copy to {rebuilt}")

    if len(argv) == 2:
        seed = bytes.fromhex(os.environ["TV_SEED"])
        key_id = os.environ["TV_KEY_ID"]
        rs = json.loads(pathlib.Path(argv[1]).read_text())
        body = {k: v for k, v in rs.items() if k != "signature"}
        sig_py = signing.sign_catalog(body, seed=seed, key_id=key_id)["signature"]["sig"]
        print(f"head py: {sig_py}")
        print(f"head rs: {rs['signature']['sig']}")
        if sig_py != rs["signature"]["sig"]:
            print("::error::Rust/Python signed-head signatures diverge", file=sys.stderr)
            return 1
        h = audit.verify_head(records, rs, {key_id: signing.public_key_b64(seed)})
        print(f"Signed head byte-identical; Python verify_head: {h}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
