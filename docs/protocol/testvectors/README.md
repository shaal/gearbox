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
catches any edit/reorder/mid-deletion at the first bad `seq`; a tail truncation is caught by the
**signed head** (`head.signed.json` + `head.canonical.json`, protocol §11.4), which commits
`log_id`/`count`/`head_self` so `audit verify --head` rejects a log truncated below the checkpoint.
Head `signature.sig`:
`ANdIAGBBuhBO/23PiTFWlxfLJSzKHzSViulEZo+C4CegdwPTj5O4v6hqcxdEv8bqHQjPl2tka9aStzmA0xGcCQ==`. Both the
chain and the head are cross-checked byte-for-byte by the CI parity job.

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

## Managed policy vector

[`policy/`](policy/) is the frozen contract for the **managed-mode policy** (protocol §12,
[ADR-0003](../adr/ADR-0003-managed-mode-policy.md), Phase 3 T0-C):

| file | role |
|---|---|
| `policy/policy.signed.json` | a managed policy (`allow_stores:[acme-internal]`, `deny_public`, forced pin `doom→acme-internal`), signed with the test key |
| `policy/policy.canonical.json` | JCS bytes of the policy **without** `signature` (the signing input) |

Signed with the §7.2 envelope by the org policy key (`gearbox-testvector-2026`); its
`signature.sig` is
`55FGtdWVZpb4ViTIxsAla9SmWIWUDs3CdHw5Wi07jg4MAZ+wJR5uzKTUaeaI1UuFS4JSYsHjc/xlS6wBzUexCg==`.
Any producer MUST reproduce those canonical bytes and that signature. The Rust crate
(`crates/gearbox/tests/policy.rs`) and Python oracle (`tools/cogstore/policy.py`,
`tools/selftest.sh` case 8) assert it; the CI **parity** job has Rust sign a policy and Python
re-sign its body and confirm the signatures are byte-identical. A forged/unsigned/wrong-key
policy is rejected fail-closed (§12.2). The projection + resolution (`allow_stores` / `deny_public`
/ forced pins → resolver) is Rust-only and covered by `tests/policy.rs`.

```
# Verify (Python oracle): reproduce the canonical bytes + signature.
python3 - <<'PY'
import sys, pathlib; sys.path.insert(0, "tools")
from cogstore import policy, jcs, signing
tv = pathlib.Path("docs/protocol/testvectors/policy")
p = policy.build_policy(allow_stores=["acme-internal"], deny_public=True,
                        forced_pins={"doom": "acme-internal"})
assert jcs.canonical(p) == (tv/"policy.canonical.json").read_bytes()
seed = bytes.fromhex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
print(policy.sign(p, seed=seed, key_id="gearbox-testvector-2026")["signature"]["sig"])
PY

# Regenerate with the native CLI:
gearbox policy create --out policy/policy.signed.json --sign-seed-hex <seed> \
  --key-id gearbox-testvector-2026 --allow-stores acme-internal --deny-public --forced-pin doom=acme-internal
```

## Attestation vector

[`attestation/`](attestation/) is the frozen contract for the **provenance + SBOM attestation**
(protocol §13): a signed `attestation.json` binding the `doom` binary
(`sha256 238a6e0…`) to its source provenance and a one-package SBOM (FreeDoom). Signed with the
§7.2 envelope by `gearbox-testvector-2026`; its `signature.sig` is
`j/eQWRTmxAucAG170sHM75g9PeRAz9dsWt7jr5KhcIc6UxPDT+EZh/+tdF1bgntSWSIwFc3hIpHMUgA666AiDA==`.

Any producer MUST reproduce `attestation.canonical.json` byte-for-byte and that signature. The
Rust crate (`crates/gearbox/tests/attest.rs`) and Python oracle (`tools/cogstore/attest.py`,
`tools/selftest.sh` case 9) assert it; the CI **parity** job has Rust sign an attestation and
Python re-sign its body byte-identical + verify. Two guards: tampering any field breaks the
**signature**; the recorded `subject.sha256` is checked against the real artifact bytes (the
**digest binding**, verifiable against `tools/testdata/artifacts/cogs/arm/cog-doom-arm`).

```
# Verify (Python oracle): reproduce the canonical bytes + signature, then check the binding.
python3 - <<'PY'
import sys, pathlib; sys.path.insert(0, "tools")
from cogstore import attest, jcs, signing
tv = pathlib.Path("docs/protocol/testvectors/attestation")
a = attest.build_attestation(
    {"cog":"doom","version":"0.1.0","artifact":"cogs/arm/cog-doom-arm",
     "sha256":"238a6e038d11d2b9851396b8ec167ad2f5c8724525100473c2a3f06c9ea43561"},
    {"builder":"cogs-ci","source_repo":"github.com/cognitum-one/cogs",
     "source_commit":"abc1234def5678","built_at":"2026-06-10T00:00:00Z"},
    [{"name":"freedoom","version":"0.13.0","license":"BSD-3-Clause",
      "sha256":"7323bcc168c5a45ff10749b339960e98314740a734c30d4b9f3337001f9e703d"}])
assert jcs.canonical(a) == (tv/"attestation.canonical.json").read_bytes()
seed = bytes.fromhex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
print(attest.sign(a, seed=seed, key_id="gearbox-testvector-2026")["signature"]["sig"])
PY
```

## Regenerate

Deterministic from the seed above. Re-running the generator yields byte-identical
files; if your JCS output differs from `catalog.canonical.json` (or `store.canonical.json`),
your canonicalization is wrong.
