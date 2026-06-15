"""Audit / event log (Phase 3, T0-B) — the Python cross-check oracle.

The native producer/consumer is the Rust `gearbox audit append` / `audit verify`
(`crates/gearbox/src/audit.rs`); this module is the parity oracle that keeps the hash chain
honest byte-for-byte. An append-only JSONL log; each record is hash-chained to the previous:

    self = sha256(JCS(record without `self`))
    prev = the previous record's `self`   (or 64 zeros for seq 0)

`prev`/`self` reuse `jcs` + `sha2` only — no key. Because `prev` is inside the bytes `self`
hashes, every `self` commits to the whole prior chain. `verify` detects any edit, reordering,
or mid-log deletion offline and reports the first bad `seq`; a pure tail truncation yields a
valid shorter prefix (the known limit of a keyless chain). Each stored line is the JCS canonical
bytes of the record, so a log written here is byte-identical to one written by the Rust crate.
"""
import hashlib
import pathlib

from . import jcs

ZERO_PREV = "0" * 64

KNOWN_EVENTS = ("add_store", "verify_catalog", "install", "policy_deny", "key_change")


def record_self(record: dict) -> str:
    """sha256 of the JCS canonical bytes of `record` with its `self` removed."""
    body = {k: v for k, v in record.items() if k != "self"}
    return hashlib.sha256(jcs.canonical(body)).hexdigest()


def build_record(seq: int, ts: str, event: str, subject: str, detail: dict, prev: str) -> dict:
    """Build a complete record (including `self`) from its fields and the chain head."""
    record = {
        "seq": seq,
        "ts": ts,
        "event": event,
        "subject": subject,
        "detail": detail,
        "prev": prev,
    }
    record["self"] = record_self(record)
    return record


def read_log(path) -> list[dict]:
    """Parse a log file into its records (one per non-empty line). Missing file -> empty log."""
    p = pathlib.Path(path)
    if not p.exists():
        return []
    import json
    out = []
    for line in p.read_text(encoding="utf-8").splitlines():
        if line.strip():
            out.append(json.loads(line))
    return out


def _head(records: list[dict]) -> tuple[int, str]:
    if not records:
        return 0, ZERO_PREV
    last = records[-1]
    return int(last["seq"]) + 1, last["self"]


def append(path, ts: str, event: str, subject: str, detail: dict) -> dict:
    """Append one chained record (creating the log if absent) and return it.

    The stored line is the record's JCS canonical bytes, so the file matches the Rust crate's
    output exactly.
    """
    p = pathlib.Path(path)
    records = read_log(p)
    seq, prev = _head(records)
    record = build_record(seq, ts, event, subject, detail, prev)
    with p.open("ab") as f:
        f.write(jcs.canonical(record))
        f.write(b"\n")
    return record


class ChainBreak(Exception):
    """Raised by `verify` at the first record where the chain breaks (carries the bad seq)."""

    def __init__(self, seq, reason):
        super().__init__(f"seq {seq}: {reason}")
        self.seq = seq
        self.reason = reason


def verify(records: list[dict]) -> dict:
    """Recompute the chain. Return {n, head_self} on success; raise ChainBreak on the first
    record whose `self` is wrong, whose `seq` is non-contiguous, or whose `prev` does not link."""
    prev_expected = ZERO_PREV
    head_self = ZERO_PREV
    for i, rec in enumerate(records):
        seq = rec.get("seq")
        stored_self = rec.get("self", "")
        if record_self(rec) != stored_self:
            raise ChainBreak(seq, "record content altered (self mismatch)")
        if seq != i:
            raise ChainBreak(seq, f"out-of-order or missing record (expected seq {i})")
        if rec.get("prev") != prev_expected:
            raise ChainBreak(seq, "broken chain (prev != previous self)")
        prev_expected = stored_self
        head_self = stored_self
    return {"n": len(records), "head_self": head_self}


def parse_details(pairs: list[str]) -> dict:
    """Parse repeated `key=value` tokens into a dict (split on the first `=`); keys must be ASCII."""
    out = {}
    for p in pairs:
        if "=" not in p:
            raise ValueError(f"--detail {p!r} must be key=value")
        k, v = p.split("=", 1)
        if not k:
            raise ValueError(f"--detail {p!r}: empty key")
        k.encode("ascii")  # JCS object keys must be ASCII (raises UnicodeEncodeError otherwise)
        out[k] = v
    return out
