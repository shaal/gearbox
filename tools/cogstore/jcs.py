"""RFC 8785 (JSON Canonicalization Scheme) — the subset cog-store catalogs use.

Catalogs are restricted to ASCII strings and integer numbers (protocol §7.1), where
RFC 8785 coincides exactly with
    json.dumps(obj, sort_keys=True, separators=(",", ":"), ensure_ascii=False)

`canonical()` MUST produce bytes identical to
docs/protocol/testvectors/catalog.canonical.json — the package self-test asserts it,
so a regression here is caught immediately.

For general content (floats, non-ASCII) a full RFC 8785 implementation is required; the
guard below refuses anything outside the supported subset rather than emit wrong bytes.
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
        try:
            o.encode("ascii")
        except UnicodeEncodeError:
            raise ValueError(f"{path}: non-ASCII string outside this JCS subset")
        return
    if isinstance(o, dict):
        for k, v in o.items():
            if not isinstance(k, str):
                raise ValueError(f"{path}: non-string object key {k!r}")
            _assert_subset(v, f"{path}.{k}")
        return
    if isinstance(o, list):
        for i, v in enumerate(o):
            _assert_subset(v, f"{path}[{i}]")
        return
    raise ValueError(f"{path}: unsupported type {type(o).__name__}")
