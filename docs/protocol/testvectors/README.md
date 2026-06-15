# Catalog signing — test vector

The **executable contract** for cog-store catalog signing
(see [`../cog-store-protocol.md`](../cog-store-protocol.md) §7). Any signer or
verifier MUST reproduce these exact canonical bytes and this exact signature.

> ⚠️ **Test key — DO NOT USE IN PRODUCTION.** The private seed below is published
> here deliberately so anyone can regenerate the vector. It protects nothing.

## Algorithm

1. Take the catalog object; remove its top-level `signature` member.
2. Serialize the remainder with **RFC 8785 (JSON Canonicalization Scheme)** →
   the *signing input* (`catalog.canonical.json`, UTF-8, no trailing newline).
3. `sig = Ed25519(privkey, signing_input)`; embed as base64 in `signature.sig`.

The verifier ([`verify.py`](verify.py)) reverses it: parse → drop `signature` →
JCS → check `sig` against the trusted public key for `key_id`.

## This vector

| field | value |
|---|---|
| `key_id` | `gearbox-testvector-2026` |
| Ed25519 private seed (hex, 32 B) | `000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f` |
| Ed25519 public key (base64, 32 B) | `A6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=` |
| Public-key fingerprint (SHA-256 hex of the 32-byte key) | `56475aa75463474c0285df5dbf2bcab73da651358839e9b77481b2eab107708c` |
| Signing input | [`catalog.canonical.json`](catalog.canonical.json) (650 bytes) |
| Signing-input SHA-256 (hex) | `f2b6f83a31f5c03860169ae76445a55619c8f2ac4bc3ddb22d131cf6d4fe4687` |
| Signature (base64, 64 B) | `Q2VExF2MDkXRc1suwDBtbre/3c63wKeOec0WeX2RB8hzFUhftJXZJz8x/BWZxXkddT/1zMXA/dE/UqZPI/DfDw==` |

## Verify it yourself

**Python** (stdlib + `cryptography`):

```
python3 verify.py
# OK: signature valid over 650 canonical bytes
```

**OpenSSL** (independent implementation) — rebuild the public key as an SPKI PEM
(the `302a300506032b6570032100` prefix is the standard Ed25519 SPKI header), then
verify the raw 64-byte signature over the canonical bytes:

```
printf '%s' 'A6EHv/POEL4dcN0Y50vAmWfk1jCbpQ1fHdyGZBJVMbg=' | base64 -d \
  | (printf '\x30\x2a\x30\x05\x06\x03\x2b\x65\x70\x03\x21\x00'; cat) \
  | openssl pkey -pubin -inform DER -out pub.pem
printf '%s' 'Q2VExF2MDkXRc1suwDBtbre/3c63wKeOec0WeX2RB8hzFUhftJXZJz8x/BWZxXkddT/1zMXA/dE/UqZPI/DfDw==' | base64 -d > sig.bin
openssl pkeyutl -verify -pubin -inkey pub.pem -rawin \
  -in catalog.canonical.json -sigfile sig.bin
# Signature Verified Successfully
```

## Store-info vector

`store.signed.json` + `store.canonical.json` are the **store-info document** equivalents
(protocol §8) — a `store.json` self-signed with the **same** test key
(`gearbox-testvector-2026`). Its `description` carries a non-ASCII em-dash **on purpose**:
string values may be any UTF-8 (object keys stay ASCII, numbers stay integers), and the Rust
(`crates/gearbox`) and Python (`tools/`) implementations canonicalize it identically — each
asserts JCS == its `*.canonical.json` in its test suite.

## Air-gap bundle vector

[`bundle/`](bundle/) is the frozen contract for the **air-gap bundle manifest**
(protocol §10, Phase 3 T0-A). It is a complete, self-contained bundle:

| file | role |
|---|---|
| `bundle/store.json` | store-info, self-signed with the test key (`gearbox-bundle-testvector`) |
| `bundle/app-registry.json` | signed catalog over the staged `adversarial` cog |
| `bundle/artifacts/cogs/arm/cog-adversarial-arm` | the one artifact the catalog references |
| `bundle/manifest.signed.json` | the signed bundle manifest |
| `bundle/manifest.canonical.json` | JCS bytes of the manifest **without** `signature` (the signing input) |

The manifest signs the **same key** as the catalog (`gearbox-testvector-2026`), with the §7.2
embedded envelope — so a verifier has one trust anchor and every file is hashed in `files[]`.
Its `signature.sig` is
`9//BsmgI6zI3R2PKG1kWaH+PEjk31qiErtdGhw4/+kG6ygxys4c9g1494PEBSbVzwL2xCzLz857XCpkY9+tqDQ==`.

Any producer MUST reproduce `manifest.canonical.json` byte-for-byte and that signature. The
Rust (`crates/gearbox/tests/bundle.rs`) and Python (`tools/selftest.sh`, case 6) suites assert
it; the CI **parity** job has Rust export a bundle and Python re-sign its manifest and confirm
the signatures are byte-identical. A real bundle's manifest is named `manifest.json` — the
vector uses `manifest.signed.json` only to match the `*.signed.json` naming here.

### Verify / regenerate the bundle vector

```
# Verify (Python oracle): reproduce the canonical bytes + signature, then verify the bundle.
python3 - <<'PY'
import sys, pathlib; sys.path.insert(0, "tools")
from cogstore import bundle, jcs, signing
tv = pathlib.Path("docs/protocol/testvectors/bundle")
m = bundle.build_manifest(tv, "2026-06-10T00:00:00Z")
assert jcs.canonical(m) == (tv/"manifest.canonical.json").read_bytes()
seed = bytes.fromhex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
print(signing.sign_catalog(m, seed=seed, key_id="gearbox-testvector-2026")["signature"]["sig"])
PY

# Regenerate from scratch with the native CLI (cogs dir holding only the adversarial cog):
gearbox catalog --cogs-dir <dir> --artifacts-dir tools/testdata/artifacts \
  --store-id gearbox-bundle-testvector --generated-at 2026-06-10T00:00:00Z \
  --out app-registry.json --sign-seed-hex <seed> --key-id gearbox-testvector-2026
gearbox export --catalog app-registry.json --store-info store.json \
  --artifacts-dir tools/testdata/artifacts --out bundle \
  --generated-at 2026-06-10T00:00:00Z --sign-seed-hex <seed> --key-id gearbox-testvector-2026
```

## Audit log vector

[`audit/log.jsonl`](audit/log.jsonl) is the frozen contract for the **hash-chained audit log**
(protocol §11, Phase 3 T0-B): a four-record `add_store → verify_catalog → install → policy_deny`
chain. Each record is one line of **JCS canonical bytes**; `self = sha256(JCS(record − self))`
and `prev` = the previous record's `self` (64 zeros for `seq` 0). The `policy_deny` record's
`detail.reason` carries a non-ASCII em-dash **on purpose** — string values may be any UTF-8, and
the Rust (`crates/gearbox`) and Python (`tools/`) implementations canonicalize it identically.

The chain's **head `self`** is
`65a00c0ac86fd4ad8b16919bc9b5022939481ce87bcb783818ae8d78ae8ea2d3`. Any producer MUST reproduce
every `self`/`prev` byte-for-byte. The Rust (`crates/gearbox/tests/audit.rs`) and Python
(`tools/selftest.sh`, case 7) suites assert it; the CI **parity** job has Rust append a chain,
Python rebuild it byte-for-byte and verify it, then Rust verify the Python-rebuilt log. `verify`
catches any edit/reorder/mid-deletion at the first bad `seq` (a tail truncation is the known
keyless limit — §11.2).

```
# Verify (Python oracle): recompute the chain + reproduce it byte-for-byte from the same fields.
python3 - <<'PY'
import sys, pathlib; sys.path.insert(0, "tools")
from cogstore import audit
tv = pathlib.Path("docs/protocol/testvectors/audit/log.jsonl")
print(audit.verify(audit.read_log(tv)))   # {'n': 4, 'head_self': '65a00c0a…'}
PY

# Regenerate from scratch with the native CLI:
gearbox audit append --log audit/log.jsonl --ts 2026-06-14T15:00:00Z --event add_store \
  --subject acme-internal --detail key_id=acme-signing-2026 \
  --detail fingerprint=56475aa7…708c --detail result=ok
# …verify_catalog, install, policy_deny — see crates/gearbox/tests/audit.rs for the exact fields.
```

## Regenerate

Deterministic from the seed above. Re-running the generator yields byte-identical
files; if your JCS output differs from `catalog.canonical.json` (or `store.canonical.json`),
your canonicalization is wrong.
