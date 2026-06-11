# Cog Store Protocol (v0 — draft)

**Status**: Draft
**Date**: 2026-06-10
**Implements**: [ADR-0001 (Pluggable cog stores)](../adr/ADR-0001-pluggable-cog-stores.md)

This is the contract a **store operator** and the **Seed runtime** both implement so a
Seed can install cogs from any store — official, private, or alternative-public — with
the same safety guarantees. It is intentionally minimal for v0; richer lifecycle and
discovery fields are tracked in [`../research/cog-store-enhancements.md`](../research/cog-store-enhancements.md).

---

## 1. Roles

- **Cog** — a `cog.toml` manifest + binary + assets (defined in `cognitum-one/cogs`).
  Manifests are **store-agnostic**: asset paths are relative.
- **Store** — a catalog + an artifact base + a signing key. A store *publishes* cogs.
- **Seed** — the device. Holds a list of trusted stores and *installs* cogs from them.

## 2. Store descriptor (Seed-side)

A Seed holds an ordered list of stores. Each:

| Field | Req | Meaning |
|---|---|---|
| `id` | ✓ | Stable store id; used as the install namespace (`<id>/<cog-id>`). |
| `name` | ✓ | Human label for the dashboard. |
| `catalog_url` | ✓ | Where to fetch `app-registry.json`. |
| `artifact_base` | ✓ | Base for resolving relative artifact paths. Scheme selects the fetcher: `gs://`, `https://`, `s3://`, `oci://`, `file://`. |
| `trust` | ✓ | One or more ed25519 public-key ids that this store's catalog MUST be signed by. |
| `auth` | — | Transport auth for private stores: `{type: bearer\|mtls\|gcp\|aws, ...}`. |
| `priority` | — | Lower wins when a bare cog id resolves to multiple stores. Default 100. |
| `enabled` | — | If false, the store is configured but not queried. Default true. |

## 3. Catalog (`app-registry.json`)

A signed, portable JSON document. Artifact references are **relative** to the store's
`artifact_base` — the catalog never hardcodes a bucket.

```jsonc
{
  "schema_version": 1,
  "store_id": "acme-private",
  "generated_at": "2026-06-10T00:00:00Z",
  "cogs": [
    {
      "id": "doom",
      "versions": [
        {
          "version": "0.1.0",
          "manifest": { /* the cog.toml, normalized to JSON */ },
          "artifacts": {
            "binary": { "path": "cogs/arm/cog-doom-arm", "sha256": "…", "size": 1234567 },
            "assets": [
              { "id": "freedoom1-wad", "path": "cogs/arm/wads/freedoom1.wad",
                "filename": "freedoom1.wad", "sha256": "7323bcc1…", "size": 28795076 }
            ]
          }
        }
      ]
    }
  ],
  "signature": { "key_id": "acme-signing-2026", "alg": "ed25519", "sig": "base64…" }
}
```

The `signature` covers the **RFC 8785 (JCS) canonicalization of the catalog with its own
`signature` member removed** — see §7 for the exact algorithm and a committed test vector.
Per-version signatures are permitted as an alternative to a single catalog signature
(useful when multiple publishers share one store).

### Relationship to `cog.toml`

The catalog's `manifest` is the cog's `cog.toml` normalized to JSON. The one addition on
the cog side is a scheme-agnostic relative `path` in `[[assets]]`; `gcs_path` remains a
back-compat alias meaning "relative to the official GCS base." See [the plan](../plans/pluggable-cog-stores.md) §4.2.

Each `artifacts.assets[]` entry is **self-contained for install**: alongside `path` /
`sha256` / `size` it carries the manifest's `filename` (the on-device destination) and any
`required_when` (conditional install), so the Seed need not cross-reference the embedded
`manifest` by `id`.

## 4. Install algorithm (Seed-side)

```
for each enabled store (by priority):
    catalog = fetch(store.catalog_url)            # with store.auth if present
    verify_signature(catalog, against = store.trust)   # §7; REJECT if unsigned/untrusted
    resolve requested cog id (namespaced or by priority)
    for each artifact in the selected version:
        url   = join(store.artifact_base, artifact.path)
        bytes = fetch(url)                          # scheme-appropriate fetcher
        assert sha256(bytes) == artifact.sha256     # integrity vs the SIGNED manifest
    install to /var/lib/cognitum/apps/<cog-id>/
    apply runtime policy (permissions, resources) — unchanged from today
```

Two guarantees:

1. **Authenticity** comes from the signature check against the store's trust anchor.
2. **Integrity** comes from sha256 against the *signed* manifest — never against a hash
   the transport alone supplied.

## 5. Trust establishment

- **Official store** — its key ships with the Seed.
- **Private store** — its key is provisioned by an admin (fleet config); no end-user prompt.
- **Alternative public store** — added by URL; the Seed shows the key **fingerprint** and
  the user confirms (trust-on-first-use). The fingerprint is pinned; a later key change
  re-prompts.

## 6. Policy (managed devices)

A device policy may:
- restrict installs to an **allowlist** of store ids,
- forbid adding new stores (hide "Add store"),
- pin specific cogs to specific stores,
- require a minimum signing posture (e.g. provenance attestation present).

## 7. Signing

Catalogs are signed with **Ed25519** over the **RFC 8785 (JSON Canonicalization Scheme,
JCS)** serialization of the catalog. A standard canonicalization is mandatory — it is the
only way the signer (store) and the verifier (Seed) agree on bytes. **Do not hand-roll
JSON canonicalization;** use a JCS implementation.

### 7.1 What is signed

1. Take the catalog object.
2. Remove its top-level `signature` member (if present).
3. Serialize the remainder with **RFC 8785 JCS** → the *signing input* (UTF-8 bytes).
4. `sig = Ed25519(privkey, signing_input)`.

The verifier reverses it: parse JSON → remove `signature` → JCS → verify `sig` against the
trusted public key for `key_id`. Because the signing input is the catalog *minus* its own
signature, a catalog can carry its signature inside the same document.

JCS pins the parts that bite: object members sorted by UTF-16 code-unit order, no
insignificant whitespace, minimal string escaping, and ECMAScript number formatting.
**Constraints** (so the reference implementations agree byte-for-byte): numbers are
**integers**, object **keys** are **ASCII**, and string **values** may be any UTF-8 (emitted
as UTF-8 per RFC 8785). The tooling rejects floats and non-ASCII keys rather than risk a
divergent encoding.

### 7.2 Signature envelope

```json
"signature": {
  "key_id": "cognitum-release-2026",
  "alg": "ed25519",
  "sig": "<base64 of the 64-byte Ed25519 signature>"
}
```

- `key_id` — identifies the public key; **date-scoped** so rotation is additive (publish a
  new key id, trust both during an overlap window, then retire the old one).
- `alg` — `ed25519` (the only value in v0).
- `sig` — standard base64 (RFC 4648 §4, padded) of the 64-byte signature.

### 7.3 Keys, trust & fingerprints

- Public keys are raw 32-byte Ed25519 keys. A store descriptor's `trust` lists the
  `key_id`s a catalog from that store MUST be signed by.
- **Fingerprint** (for the TOFU display in §5) = lowercase-hex SHA-256 of the raw 32-byte
  public key.
- Phase 1: the official store has exactly one trusted key, embedded in the Seed. Rotation
  overlap and revocation lists are v1 (§8).

### 7.4 Test vector (the contract is executable)

A committed, runnable vector lives in [`testvectors/`](testvectors/):

| file | what |
|---|---|
| `catalog.signed.json` | a sample catalog with a valid `signature` |
| `catalog.canonical.json` | the exact JCS signing input (the bytes that were signed) |
| `verify.py` | standalone verifier (stdlib + `cryptography`) |
| `README.md` | the throwaway test keypair, canonical SHA-256, signature, and `python`/`openssl` verify recipes |

It is cross-checked by two independent implementations (Python `cryptography` and
OpenSSL). Implementers MUST reproduce the same canonical bytes and verify the same
signature — if your canonicalizer's output differs from `catalog.canonical.json`
byte-for-byte, your JCS is wrong.

## 8. Store-info document (`store.json`) — trust-on-first-use

Before a Seed can trust a *new* store it needs that store's public key(s). A store therefore
publishes a small **store-info document** at a well-known path (`store.json`):

```jsonc
{
  "schema_version": 1,
  "store_id": "acme-internal",
  "name": "ACME Internal Cogs",
  "description": "Internal cogs for ACME devices.",
  "keys": [ { "key_id": "acme-signing-2026", "alg": "ed25519", "pubkey_b64": "…" } ],
  "catalog_url": "https://cogs.acme.internal/app-registry.json",
  "signature": { "key_id": "acme-signing-2026", "alg": "ed25519", "sig": "…" }
}
```

- It is **self-signed** with the same envelope + JCS as the catalog (§7), by one of its own
  listed keys. The self-signature is *integrity* (the doc isn't truncated/altered) — **not**
  authority.
- **Add-store flow (TOFU):** fetch `store.json` → show each key's **fingerprint** (§7.3) →
  the user confirms → the keys are **pinned** as that store's trust anchor. Thereafter the
  store's catalog (and any refreshed `store.json`) must verify against a pinned key (§4/§7);
  a later key change re-prompts (SSH-known-hosts model). The official store ships its key
  with the Seed (no prompt); private stores are admin-provisioned.

A committed reference + vector lives in [`testvectors/`](testvectors/) (`store.signed.json`,
`store.canonical.json`). This is a Phase-2 surface — see the
[Phase 2 plan §4](../plans/phase-2-implementation.md).

## 9. Open items (v0 → v1)

- Key rotation: multiple valid keys per store during overlap; revocation list.
- Whether to require a transparency-log inclusion proof for public stores.
- `oci://` artifact layout (reuse OCI referrers for signatures/SBOM).
- JCS number formatting, if non-integer numeric fields are ever introduced.
- Versioning/upgrade semantics — see the enhancements research.
