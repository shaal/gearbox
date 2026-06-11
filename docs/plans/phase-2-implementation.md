# Phase 2 — Multi-store (alternative & private stores become usable)

**Status**: Draft / planning
**Date**: 2026-06-11
**Parent**: [ADR-0001 (Pluggable cog stores)](../adr/ADR-0001-pluggable-cog-stores.md) ·
[Phase 1 plan](phase-1-implementation.md) · [Cog Store Protocol](../protocol/cog-store-protocol.md)

> Phase 1 turned the single hardcoded bucket into a single **config-driven, signed** store
> — a seam. Phase 2 widens that seam into a real multi-store system: **N stores,
> namespacing, trust-on-first-use, and per-store auth**. This is the phase where alternative
> public stores (F-Droid-style) and private stores actually work end to end.

---

## 1. Goal & definition of done

A Seed can trust and install from **more than one** store. Concretely:

1. An operator hosts a store (catalog + artifacts + a published signing key); a user **adds
   it by URL**, sees its key **fingerprint**, confirms (TOFU), and installs a cog from it.
2. Two stores offering the same cog id (`doom`) resolve **unambiguously** via namespacing,
   priority, and pinning.
3. A **private** store with **bearer auth** works (token provisioned as a device secret).
4. The **official store keeps working unchanged** as the default.

## 2. Scope / non-goals

In scope: multiple `StoreDescriptor`s + resolution; namespacing (`store/cog`) + pinning;
multi-catalog load + per-store verify + merge; a **store-info document** + TOFU "Add store";
**bearer** auth; the dashboard/API surface for managing stores.

**Non-goals (Phase 3 — managed/enterprise):** policy lockdown / allow-deny, MDM-style
managed mode, **mTLS / cloud-IAM** auth, fleet provisioning profiles, air-gap (`file://`)
bundles, key **revocation** + transparency logs. Phase 2 makes multi-store *possible*;
Phase 3 makes it *governable*.

## 3. What Phase 1 already gave us (the seam)

Phase 1 was deliberately shaped so Phase 2 is additive, not a rewrite:

- `StoreDescriptor` is already **list-shaped** (B1) — Phase 2 lifts the "exactly one enabled
  store" invariant rather than changing the type.
- The `Fetcher` trait (B2) is scheme-keyed and ready for new backends/auth.
- Catalogs are already **signed + verified** (A4/B4); Phase 2 runs verify **per store**.
- The install namespace `<store-id>/<cog-id>` was reserved in the descriptor from day one.

## 4. New protocol surface: the store-info document

TOFU needs the store's **public key(s)** before any catalog can be trusted. So a store
publishes a small, well-known **store-info document** (`store.json` at the store root):

```jsonc
{
  "schema_version": 1,
  "store_id": "acme-internal",
  "name": "ACME Internal Cogs",
  "description": "Internal cogs for ACME devices.",
  "keys": [
    { "key_id": "acme-signing-2026", "alg": "ed25519", "pubkey_b64": "…" }
  ],
  "catalog_url": "https://cogs.acme.internal/app-registry.json"
}
```

Add-store flow: fetch `store.json` → show each key's **fingerprint** (SHA-256 of the raw
key, protocol §7.3) → user confirms → those keys are **pinned** as the store's trust anchor.
Thereafter the store's catalog must be signed by a pinned key (B4 unchanged). A later key
change → re-prompt (SSH-known-hosts model). This becomes **protocol §9** and likely a short
**ADR-0002**.

## 5. Workstreams

Mostly seed; small gearbox/cogs additions for the store-info doc + tooling.

### A — seed: multi-store resolution
Lift the single-store invariant; hold an ordered list; resolve a cog by (namespace ‖
priority ‖ pin). Removing/disabling a store drops its cogs from the active view.

### B — seed: namespacing + pinning
Install records keyed by `<store-id>/<cog-id>`. A **bare** `doom` resolves by store priority;
a user/admin can **pin** a cog to a store. Surface the namespaced id everywhere a cog id is
shown.

### C — seed: multi-catalog load + per-store verify + merge
Generalize B3 to fetch each enabled store's catalog, **verify each against that store's
trust** (B4), and merge into a namespaced catalog view. A failing store is skipped (logged),
not fatal to the others.

### D — seed: trust store + TOFU "Add store"
Generalize the single embedded key into a **per-store** trust set. Implement the add-store
flow over the store-info doc (§4): fetch → fingerprint → confirm → pin. Official store keeps
its built-in key (no prompt).

### E — seed: per-store bearer auth
Wire `StoreDescriptor.auth = { type: "bearer", token_ref }` through the `Fetcher` (B2). The
token is a **device-managed secret** (never in a cog, never in `store.json`). mTLS/cloud-IAM
are Phase 3.

### F — seed: dashboard / API UX
List stores; **Add store** (URL → fingerprint confirm); remove/enable/reorder; browse
per-store catalogs; choose the source at install; show store badges + namespaced ids.

### G — gearbox / cogs: store-info doc + tooling
Add a `gearbox store-info` command (emit/sign `store.json` from a store's keys) to the Rust
CLI; document `store.json` in the protocol (§9). Cog manifests are **unchanged** —
namespacing is a store/Seed concern, not a cog concern.

## 6. Resolution & namespacing rules

| Input | Resolves to |
|---|---|
| `acme-internal/doom` | exactly that store's `doom` (explicit) |
| `doom` (bare) | the enabled store with the **lowest priority** number that offers `doom` |
| `doom` pinned to `acme-internal` | always `acme-internal/doom` until unpinned |
| `doom` in two stores, no pin | bare resolves by priority; the dashboard shows both, badged |

Namespacing (the npm-scope / Docker-ref model) is what prevents a second store from
**shadowing** or impersonating an official cog.

## 7. Security

- **TOFU + fingerprint** for public stores (D); **admin-provisioned** trust for private
  (no prompt). A key change re-prompts.
- **A store can only install cogs signed by its own pinned key** — adding a store grants it
  *no* authority over other stores' namespaces.
- **Store ≠ sandbox bypass** (unchanged): runtime permissions, loopback bind, bearer tokens,
  resource budgets still gate every installed cog regardless of origin.
- **Auth secrets** are device-managed; never logged, committed, or placed in `store.json`.
- Namespacing closes the shadowing/confusion vector; priority is explicit and inspectable.

## 8. Sequencing

```
G (store-info doc + tooling) ─┐
                              ├─► D (trust + TOFU) ─► C (multi-catalog verify+merge)
A (multi-store resolution) ───┘                          │
        └─► B (namespacing + pinning) ──────────────────┘─► E (auth) ─► F (UX)
```

Land **G + D** first (you can't trust a second store without its keys), then **A/B/C** (the
resolution + merge core), then **E** (private-store auth), then **F** (the UX that exposes
it). Each step keeps the official store working.

## 9. Open decisions

1. ~~**Store-info doc vs keys-in-catalog**~~ — **resolved** ([ADR-0002](../adr/ADR-0002-store-info-and-tofu.md)):
   a separate self-signed `store.json` + TOFU. Reference impl + vector exist in `crates/gearbox`.
2. **Namespaced id syntax** — `store/cog` vs `@store/cog`; reserved chars in store ids.
   (The resolver implements `store/cog` today, [crates/gearbox/src/resolve.rs](../../crates/gearbox/src/resolve.rs).)
3. **Installed-cog fate on store removal** — keep running vs flag as orphaned vs uninstall.
4. **Catalog refresh/caching** across N stores (TTL, manual refresh, offline).
5. ~~**ADR-0002?**~~ — **done**: [ADR-0002 (Store-info document & trust-on-first-use)](../adr/ADR-0002-store-info-and-tofu.md).

## 10. Acceptance criteria

- Add a second (public) store by URL → fingerprint shown → confirm → install a cog from it,
  namespaced. Removing the store removes its cogs from the view.
- Two stores offering `doom` resolve unambiguously (explicit namespace, priority, pin).
- A private store with bearer auth installs (token as a device secret); a wrong/absent token
  fails clearly.
- The official store behaves exactly as in Phase 1 with no config.
- Every catalog is verified against **its own** store's pinned trust; one bad store doesn't
  break the others.

## 11. Recommendation & note on implementation language

Adopt the workstreams in §5, sequenced by §8; land **G + D** first. The store-info doc is the
one genuinely new artifact — spec it in protocol §9 and (recommended) record it in ADR-0002.

**Implementation language.** Phase 1 shipped two parity-checked implementations (Python
`tools/`, Rust `crates/gearbox`). For Phase 2, treat the **Rust crate as canonical** —
publish-side steps (A3/A4) and any new tooling (`gearbox store-info`) should use the Rust
binary, and a Node consumer can bind the same core via **napi-rs** (already in the stack).
Keep the Python tools, if at all, only as an independent **cross-check oracle** (parity
catches canonicalization bugs); they are not required on the critical path.
