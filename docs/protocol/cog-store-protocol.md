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
                "sha256": "7323bcc1…", "size": 28795076 }
            ]
          }
        }
      ]
    }
  ],
  "signature": { "key_id": "acme-signing-2026", "alg": "ed25519", "sig": "base64…" }
}
```

The `signature` covers the canonical serialization of everything above it
(`schema_version` … `cogs`). Per-version signatures are permitted as an alternative to a
single catalog signature (useful when multiple publishers share one store).

### Relationship to `cog.toml`

The catalog's `manifest` is the cog's `cog.toml` normalized to JSON. The one addition on
the cog side is a scheme-agnostic relative `path` in `[[assets]]`; `gcs_path` remains a
back-compat alias meaning "relative to the official GCS base." See [the plan](../plans/pluggable-cog-stores.md) §4.2.

## 4. Install algorithm (Seed-side)

```
for each enabled store (by priority):
    catalog = fetch(store.catalog_url)            # with store.auth if present
    verify_signature(catalog, against = store.trust)   # REJECT if unsigned/untrusted
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

## 7. Open items (v0 → v1)

- Canonical-JSON definition for signing (field ordering, number formatting).
- Key rotation: multiple valid keys per store during overlap; revocation list.
- Whether to require a transparency-log inclusion proof for public stores.
- `oci://` artifact layout (reuse OCI referrers for signatures/SBOM).
- Versioning/upgrade semantics — see the enhancements research.
