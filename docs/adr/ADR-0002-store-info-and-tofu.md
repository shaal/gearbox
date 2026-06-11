# ADR-0002: Store-info document & trust-on-first-use

**Status**: Proposed
**Date**: 2026-06-11
**Related**: [ADR-0001 (Pluggable cog stores)](ADR-0001-pluggable-cog-stores.md),
[Cog Store Protocol §8](../protocol/cog-store-protocol.md#8-store-info-document-storejson--trust-on-first-use),
[Phase 2 plan](../plans/phase-2-implementation.md),
[reference impl: `crates/gearbox` (`store-info` + `serve`)](../../crates/gearbox)

## Context

ADR-0001 chose a pluggable, signed multi-store model. Phase 1 shipped one **official** store
whose public key is embedded in the Seed — so the Seed can always verify the official
catalog. Phase 2 must let a Seed trust **additional** stores (alternative-public, private),
and that runs straight into the bootstrapping problem:

> To verify anything a store signs, the Seed needs that store's public key. For a *brand-new*
> store there is no pre-existing trust anchor — so how does the key get there, safely, the
> first time?

Two forces pull against each other:

- **Decentralization.** The whole point of ADR-0001 is that *anyone* can run a store without
  being blessed by Cognitum. A central authority that must sign or vouch for every store
  would defeat that.
- **Safety.** A Seed must not silently trust a key an attacker supplied. Whatever bootstraps
  trust has to put a human (or an admin policy) in the loop, with something they can verify.

We also need the key material to be **discoverable**: at "add store" time the Seed has only a
URL, and must obtain the store's identity + keys before (and independently of) fetching a
possibly-large catalog.

## Decision

Adopt a **store-info document + trust-on-first-use (TOFU)**.

1. **`store.json`** — each store publishes a small store-info document at a well-known path
   (protocol §8): `store_id`, `name`, `description`, `catalog_url`, the store's public
   `keys[]` (`key_id` + `alg` + `pubkey_b64`), and a **self-signature** by one of those keys
   (the same JCS + Ed25519 envelope as the catalog, §7).
2. **Add-store flow (TOFU).** The Seed fetches `store.json`, shows each key's **fingerprint**
   (SHA-256 of the raw key, §7.3), the user confirms, and the keys are **pinned** as that
   store's trust anchor. Thereafter every catalog (and any refreshed `store.json`) from that
   store MUST verify against a pinned key; a key change **re-prompts** (the SSH
   known-hosts / F-Droid-repo model).
3. **The self-signature is integrity, not authority.** It proves the document wasn't
   truncated/altered and binds the key set together — but the decision to *trust* comes from
   the human confirming the fingerprint, not from the signature. This distinction is the
   crux of TOFU and must be reflected in the UX copy.
4. **Trust establishment varies by store type:**
   - **Official** — key ships with the Seed (ADR-0001); no prompt.
   - **Private / enterprise** — key is **admin-provisioned** via fleet config; no end-user
     prompt (Phase 3 managed mode).
   - **Alternative public** — TOFU with fingerprint confirmation.

This resolves Phase 2 plan open decisions #1 (`store.json` over keys-in-catalog) and #5 (yes,
record it as an ADR). A working **reference implementation and executable test vector already
exist** — `gearbox store-info create/verify`, the `gearbox serve` reference store, and the
`examples/store-loop.sh` demo run the full fetch → fingerprint → verify → pin loop — so this
decision is prototyped and tested, not just proposed on paper.

## Consequences

- **Positive**: fully decentralized — anyone can run a store with no central CA and without
  Cognitum's blessing; a model users already understand (SSH, F-Droid); **self-contained
  discovery** (one URL yields identity + keys + a catalog pointer) that works before/without
  a catalog fetch; the self-signature adds integrity and a clean home for key rotation;
  reuses the catalog's signing envelope (no new crypto); already implemented + vector-pinned.
- **Negative**: TOFU's known weakness — the **first** fetch is unauthenticated, so a
  MITM at add-time could present a substituted key. Mitigations (out-of-band fingerprint
  publication by the operator, HTTPS transport, admin provisioning for managed fleets) reduce
  but do not eliminate it, and a user can still be socially engineered into confirming a
  hostile fingerprint. Key **rotation overlap** and **revocation** need more machinery
  (deferred; protocol §9 open items). Adds a new artifact + an "Add store" flow to the Seed.
- **Neutral**: store ≠ sandbox bypass (unchanged) — runtime permissions, loopback bind,
  bearer tokens, and resource budgets still gate every installed cog regardless of origin.
  Namespacing (Phase 2 §6) prevents a newly-trusted store from shadowing another store's cog.

## Alternatives considered

- **Keys embedded in the catalog (no `store.json`)** — rejected: you'd fetch the whole
  (potentially large) catalog just to display a fingerprint at add-time, and store identity /
  keys are a **store-level** concern, not a per-catalog one. A small dedicated document lets
  add-store work independently of (and before) catalog retrieval.
- **Central CA / PKI for stores** — rejected: centralizes trust, which is the opposite of
  enabling independent stores; it is operationally heavy and a single point of control and
  failure.
- **Ship every store's key with the Seed** — rejected: doesn't scale and requires Cognitum to
  bless every store, killing the alternative-store use case.
- **Unsigned `store.json` (pure TOFU, no self-signature)** — rejected: the self-signature is
  cheap (reuses the catalog envelope) and buys post-pin integrity + a rotation anchor; TOFU
  alone leaves the pinned document unauthenticated against later tampering.
- **Key-transparency log / web-of-trust** — deferred, not rejected: a transparency-log
  inclusion proof for public stores is a worthwhile **future hardening** that *complements*
  TOFU (protocol §9 open items); it is heavier than Phase 2 needs to ship the capability.
