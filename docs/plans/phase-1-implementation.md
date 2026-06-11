# Phase 1 — Config-driven single store (implementation plan)

**Status**: Ready for review
**Date**: 2026-06-10
**Parent**: [ADR-0001 (Pluggable cog stores)](../adr/ADR-0001-pluggable-cog-stores.md) ·
[Plan: Pluggable cog stores](pluggable-cog-stores.md) ·
[Cog Store Protocol](../protocol/cog-store-protocol.md)

> Phase 1 of the [4-phase rollout](pluggable-cog-stores.md#8-phased-rollout). It is a
> **pure refactor plus signing** with **zero user-visible change** — it replaces the
> single hardcoded store *origin* with a single *config-driven* store, lands the fetcher
> abstraction and the additive `path` manifest field, and signs the official catalog.
> Nothing about multi-store, namespacing, private auth, or UX is in scope here; Phase 1
> exists to de-risk all of that by making the base configurable and the catalog signed
> first.

---

## 1. Goal & definition of done

**Goal**: a Seed installs every existing cog exactly as today, except the official store
is now read from **config** (not a compiled-in constant), artifacts resolve through a
**scheme-keyed fetcher**, cog manifests may use a scheme-agnostic **`path`** (with
`gcs_path` as a back-compat alias), and the official catalog is **ed25519-signed and
verified**.

**Done when:**
1. A default-configured Seed installs all current cogs with **no observable difference**.
2. Pointing the official store's `artifact_base` at an `https://` mirror of the same bytes
   installs end-to-end — proving the base is genuinely config-driven, not hardcoded.
3. A manifest using `path = "..."` installs identically to one using `gcs_path = "..."`.
4. The official catalog is signed; a Seed **rejects** an unsigned/tampered catalog once
   the transition flag (§5, B6) is enabled.

## 2. Scope / non-goals

In scope: config-driven single store · fetcher trait (`gs://` + `https://`) · `path`
manifest field · catalog generator · catalog signing + single-key verification.

**Non-goals (Phase 2+):** multiple stores, catalog merge, namespacing (`store/cog`),
"Add store" UX + TOFU, private-store auth (bearer/mTLS/cloud-IAM), policy/managed-mode,
`s3://`/`oci://`/`file://` fetchers, revocation. The fetcher *trait* lands now; only two
schemes are implemented.

## 3. Workstreams

Three repos. Most work is in `seed`; `cogs` gets the additive manifest + publish changes;
`gearbox` owns the signing format the other two implement.

### A — `cognitum-one/cogs` (manifest + publish)

- **A1. Scheme-agnostic `path` in `[[assets]]`.** Accept `path` as the relative artifact
  location; keep `gcs_path` as a documented alias. Exactly one of the two must be present.
  Update `manifest-validate` CI accordingly and document `path` in the cog authoring guide.
- **A2. Catalog generator.** A tool that walks `src/cogs/*/cog.toml` and emits
  `app-registry.json` per the protocol (relative paths, sizes, sha256). Lives in
  **`gearbox`**, invoked from `cogs` CI (resolved §6).
- **A3. CI: catalog build + hash gates.** Extend the existing `asset-sha256` gate to cover
  catalog entries; add a check that the catalog generates and round-trips.
- **A4. Signing in publish.** After generating the catalog, sign it with the official
  ed25519 key (`STORE_SIGNING_KEY`, a CI secret in Phase 1; keyless/OIDC is a later
  hardening). Publish the signature alongside the catalog. Depends on **C1**.

### B — `cognitum-one/seed` (runtime)

- **B1. `StoreDescriptor` config + remove the constant.** Introduce the store-descriptor
  type; construct the one official store from config with defaults that preserve
  `gs://cognitum-apps`. Replace the hardcoded base everywhere it's referenced.
- **B2. Fetcher abstraction.** A `Fetcher` trait keyed by `artifact_base` scheme; implement
  `gs://` (today's behavior) and `https://`. Leave `s3`/`oci`/`file` as `unimplemented`
  stubs returning a clear error — the shape lands, the schemes don't.
- **B3. Catalog loader from config.** Load `app-registry.json` from the store's
  `catalog_url` instead of a bundled file. Single store, **no merge**.
- **B4. Catalog signature verification.** Embed the official public key as the sole trust
  anchor; verify the catalog signature before use; **fail closed**. Depends on **C1 + A4**.
- **B5. Install via resolved `path`.** Resolve each asset's `path` (or `gcs_path` alias)
  against the active store's `artifact_base`; sha256-verify against the **signed** manifest.
  Behavior is identical to today, now signature-gated.
- **B6. Transition safety.** Tolerate legacy `gcs_path` manifests and provide a
  `require_signed_catalog` flag, default **off** for one release so signing rolls out
  without a flag-day; flip to **on** once the official catalog is reliably signed.

### C — `gearbox` (spec + reference)

- **C1. Freeze the signing format (blocks A4 + B4).** Pin canonical-JSON serialization
  (field ordering, number/whitespace rules), the `key_id` format, and the signature
  envelope in [`cog-store-protocol.md`](../protocol/cog-store-protocol.md). This is the
  contract `cogs` (signer) and `seed` (verifier) must agree on byte-for-byte.
- **C2. (optional) Reference `gearbox catalog` + `gearbox sign`.** A minimal independent
  implementation so `cogs`/`seed` can cross-check against a third party. Nice-to-have in
  Phase 1; required by Phase 2.

## 4. Sequencing

```
C1 (signing format)  ──┬──────────────► A4 (sign) ──┐
                       │                             ├─ lockstep ► B4 (verify)
A1 (path field) ───────┴► A2 (catalog) ─► A3 (CI) ───┘
                                                     │
B1 (descriptor) ─► B2 (fetcher) ─► B3 (loader) ─► B5 (install) ─► B6 (transition)
```

Critical path: **C1 → A4/B4** (the signing handshake). A1→A2→A3 and B1→B2→B3→B5 proceed in
parallel; B5 needs A1's `path` to be flowing through the catalog. B6 lands last and is what
makes signing mandatory.

## 5. Risks & mitigations

| Risk | Mitigation |
|---|---|
| Signing flag-day breaks installs | `require_signed_catalog` defaults off for one release (B6) |
| Signer/verifier disagree on bytes | C1 freezes canonical-JSON first; C2 reference impl cross-checks |
| `path`/`gcs_path` ambiguity | CI requires exactly one (A1) |
| Official signing-key custody | Resolved §6: org secret manager → `STORE_SIGNING_KEY`; date-scoped `key_id` for additive rotation |
| Catalog generator ownership | Resolved §6: lives in `gearbox`, invoked from `cogs` CI |

## 6. Decisions (resolved 2026-06-10)

1. **Catalog generator home → `gearbox`.** Reusable by any store operator and kept next to
   the spec and its reference producer; `cogs` CI invokes it as a build step rather than
   vendoring its own copy. (Issue #3 is tagged `[gearbox]` accordingly.)
2. **Official signing-key custody.** The private ed25519 key lives in the org secret
   manager and is exposed to the `cogs` publish workflow only as the `STORE_SIGNING_KEY`
   CI secret — never committed, never inside a cog. Access is limited to release
   maintainers. The matching public key ships embedded in the Seed as the sole Phase 1
   trust anchor. The `key_id` is date-scoped (e.g. `cognitum-release-2026`) so rotation is
   additive: publish a new key, trust both during an overlap window, then retire the old
   one. Keyless/OIDC signing is a later hardening, not Phase 1.
3. **`https://` ships in Phase 1.** It's the cheap second fetcher that actually *proves*
   the base is config-driven — point `artifact_base` at an HTTPS mirror of the same bytes
   and installs still work. `s3://`/`oci://`/`file://` stay stubbed until Phase 2/3.

## 7. Ready-to-file issues

Each is independently shippable; `[repo]` tags the home. File under a shared
`epic: phase-1-config-driven-store` label.

1. **[gearbox] Freeze the catalog signing format (canonical-JSON + envelope)** — *(C1, blocks signer/verifier)*
   Define field ordering, number/whitespace rules, `key_id` format, and the signature
   envelope in the protocol spec. **AC:** a fixed sample catalog has one canonical byte
   representation; sign/verify round-trips against it.
2. **[cogs] Add scheme-agnostic `path` to `[[assets]]`; keep `gcs_path` as alias** — *(A1)*
   **AC:** manifest-validate accepts `path` xor `gcs_path`; a cog using `path` builds; docs updated.
3. **[gearbox] Catalog generator: `cog.toml` tree → signed `app-registry.json`** — *(A2)*
   **AC:** running it over `src/cogs/*` emits a spec-valid catalog with relative paths + sha256.
4. **[cogs] CI: catalog build + hash gate** — *(A3)*
   **AC:** PR fails if the catalog doesn't generate or an asset hash mismatches.
5. **[cogs] Sign the official catalog in publish (`STORE_SIGNING_KEY`)** — *(A4, needs #1)*
   **AC:** published catalog carries a valid ed25519 signature; CI verifies it before upload.
6. **[seed] Introduce `StoreDescriptor` config; remove the hardcoded base** — *(B1)*
   **AC:** default config reproduces `gs://cognitum-apps` behavior; no compiled-in bucket constant remains.
7. **[seed] `Fetcher` trait + `gs://` and `https://` implementations** — *(B2)*
   **AC:** install works via both schemes; `s3`/`oci`/`file` return a clear "unsupported in Phase 1" error.
8. **[seed] Load the catalog from `catalog_url` (single store, no merge)** — *(B3)*
   **AC:** Seed reads `app-registry.json` from config; the bundled-file path is removed.
9. **[seed] Verify the official catalog signature; fail closed** — *(B4, needs #1, #5)*
   **AC:** tampered/unsigned catalog is rejected when `require_signed_catalog` is on.
10. **[seed] Resolve asset `path` (alias `gcs_path`) against `artifact_base`; sha256 vs signed manifest** — *(B5, needs #2)*
    **AC:** `path` and `gcs_path` manifests install identically; hashes checked against the signed catalog.
11. **[seed] `require_signed_catalog` transition flag (default off → on)** — *(B6)*
    **AC:** one release ships with it off; flipping it on enforces signing with no other change.
12. **[gearbox] (optional) Reference `gearbox catalog` + `gearbox sign` CLI** — *(C2)*
    **AC:** independently produces a catalog + signature that `seed` accepts.

## 8. What "no user-visible change" buys us

After Phase 1, the only difference a user could detect is that the install source is now a
config value. That single seam is what every later phase plugs into: Phase 2 adds *more*
descriptors to the same list; Phase 3 adds auth/policy to each. Shipping this as a silent
refactor means the risky parts (trust, UX, multi-catalog) land on a foundation that's
already proven against the full existing cog catalog.
