"""RFC 8785 (JSON Canonicalization Scheme) — the subset cog-store documents use.

Documents are restricted to **integer numbers** and **ASCII object keys**; string *values*
may be any UTF-8. Under those constraints RFC 8785 coincides exactly with
    json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
and matches the Rust reference (`crates/gearbox`) byte-for-byte.

`canonical()` MUST produce bytes identical to the committed test vectors
(docs/protocol/testvectors/*.canonical.json) — the self-tests assert it.

Floats and non-ASCII keys are refused (they would need full RFC 8785 number formatting /
UTF-16 key ordering) rather than emit bytes that might diverge from a conforming impl.
"""
import json


def canonical(obj) -> bytes:
    """Return the RFC 8785 canonical UTF-8 bytes of `obj` (ASCII+integer subset)."""
    _assert_subset(obj)
    return json.dumps(obj, sort_keys=True, separators=(",", ":"),
                      ensure_ascii=False).encode("utf-8")


def _assert_subset(o, path="$"):
    # bool must be checked before int (bool is a subclass of int in Python).
    if isinstance(o, bool) or o is None or isinstance(o, int):
        return
    if isinstance(o, float):
        raise ValueError(f"{path}: float not allowed — use integers (protocol §7.1)")
    if isinstance(o, str):
        return  # string values may be any UTF-8 (emitted as UTF-8 per RFC 8785)
    if isinstance(o, dict):
        for k, v in o.items():
            if not isinstance(k, str):
                raise ValueError(f"{path}: non-string object key {k!r}")
            try:
                k.encode("ascii")  # keys must be ASCII: json.dumps sorts by code point,
            except UnicodeEncodeError:  # which only matches RFC 8785 UTF-16 order for ASCII
                raise ValueError(f"{path}: non-ASCII object key {k!r} (keys must be ASCII)")
            _assert_subset(v, f"{path}.{k}")
        return
    if isinstance(o, list):
        for i, v in enumerate(o):
            _assert_subset(v, f"{path}[{i}]")
        return
    raise ValueError(f"{path}: unsupported type {type(o).__name__}")
