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

## Regenerate

Deterministic from the seed above. Re-running the generator yields byte-identical
files; if your JCS output differs from `catalog.canonical.json`, your
canonicalization is wrong.
