# ADR-0003: Managed-mode policy

**Status**: Proposed
**Date**: 2026-06-14
**Related**: [ADR-0001 (Pluggable cog stores)](ADR-0001-pluggable-cog-stores.md),
[ADR-0002 (Store-info & TOFU)](ADR-0002-store-info-and-tofu.md),
[Cog Store Protocol §12](../protocol/cog-store-protocol.md#12-managed-mode-policy-policyjson--admin-enforced),
[Phase 3 plan §6](../plans/phase-3-implementation.md),
[reference impl: `crates/gearbox` (`policy`)](../../crates/gearbox)

## Context

ADR-0001/0002 made multi-store *possible*: a Seed can add private and alternative-public
stores and pin their keys (TOFU). That is the right default for an individual owner, but it is
the **wrong** default for a fleet an organization is accountable for. An enterprise deploying
Seeds to employees or kiosks needs to assert, centrally and verifiably:

> only these stores may be enabled; the public store is off; `doom` always comes from
> `acme-internal`; and a user may not add their own store.

Two constraints shape the solution:

- **It must not weaken the existing trust model.** Phase 2's per-store signing keys and
  namespacing (§6) must still hold; a policy may not let one store sign for another's cogs.
- **It must be enforceable on a disconnected device.** Like the rest of Tier 0, there is no
  control plane — enforcement is a local check against a distributed artifact, not a server
  call.

A naive "config file the Seed reads" fails the threat model: if simply deleting a file
re-opens the device, the policy is theater. Whatever we choose has to be **fail-closed** and
**signature-gated**.

## Decision

Adopt a **signed `policy.json`, enforced as a pre-resolution projection**.

1. **`policy.json`** (protocol §12) — `schema_version`, `managed`, `allow_stores[]`,
   `deny_public`, `forced_pins{}`, `allow_user_add_store`, plus a **signature** by an **org
   policy key** in the **same JCS + Ed25519 envelope** as the catalog and store-info (§7) — no
   new crypto. The key is provisioned **out-of-band** (MDM, image bake, or inside an air-gap
   bundle), so its authority does not depend on TOFU.

2. **Enforcement is a projection in front of the resolver**, not a change to it. A small
   `policy` module computes `Policy::project(stores, pins) -> (stores, pins)`:
   - drop (force-disable) any store not in `allow_stores`;
   - force-disable the built-in official store if `deny_public`;
   - overlay `forced_pins` onto the user pins (the admin pin wins).
   The **unchanged** `resolve.rs` then runs its normal rules. An out-of-policy reference
   resolves to the *existing* typed error (`StoreDisabled` / `NotFound`), which the CLI/seed
   relabels a **policy denial** and records as a `policy_deny` audit event (ADR pending /
   protocol §11, T0-B). `allow_user_add_store == false` refuses the TOFU add-store flow.

3. **Fail-closed.** Authority is the signature, so verification *is* the gate: an unsigned,
   forged, wrong-key, or schema-invalid policy is **rejected**, and a device provisioned
   `managed:true` denies rather than reverting to the open default. There is deliberately no
   "policy missing → open" branch — that absence is the property. (`gearbox policy verify` and
   `policy check` both exit non-zero on any verification failure.)

4. **Policy only ever restricts.** It can deny stores and force pins; it can grant no new
   authority and cannot give a store rights over another store's namespace. Phase 2's
   per-store-key rule is untouched.

A working **reference implementation + executable vector already exist**:
`gearbox policy create/verify/check`, the frozen vector in
[`testvectors/policy/`](../protocol/testvectors/policy/), Rust↔Python signing parity, and
`examples/managed-mode.sh` run the full author → distribute → enforce → deny-and-audit loop —
so this decision is prototyped and tested, not proposed on paper.

## Consequences

- **Positive**: a procurement-grade managed story with **no server** — a signed file enforced
  locally; reuses the catalog signing envelope (no new crypto) and the resolver seam (no new
  resolution logic); fail-closed by construction; denials are auditable from day one via the
  T0-B `policy_deny` record; reversible and inspectable (`policy check` is an ops dry-run).
- **Negative**: key management is now load-bearing — losing/rotating the **org policy key**
  needs an out-of-band re-provision (overlap/rotation is the same deferred problem as store
  keys, protocol §9). A policy can brick a device's store access if mis-authored; `policy
  check` mitigates by making the outcome inspectable before rollout. The
  `allow_user_add_store` / add-store *enforcement* lives in the Seed (this repo ships the
  format + projection + dry-run, not the Seed UX).
- **Neutral**: identifying the public store relies on a known **`cognitum-official`** id
  (ADR-0001); a future multi-official-store world would generalize `deny_public` to a set.
  Managed mode is opt-in per device (provisioned), so unmanaged Seeds are unchanged.

## Alternatives considered

- **Unsigned policy file the Seed reads** — rejected: deleting or editing it bypasses the
  control; not fail-closed, not tamper-evident. The signature is the whole point.
- **Bake policy into firmware / the Seed binary** — rejected: not updatable without a
  re-flash, and not expressible per-fleet; a signed, distributable document is the MDM-native
  shape.
- **A managed control plane (policy server the device polls)** — deferred (Tier 1+): violates
  the Tier-0 "no server, works air-gapped" constraint; a signed document is strictly simpler
  and offline-first. A control plane can later *distribute* the same signed artifact.
- **Teaching `resolve.rs` about policy directly** — rejected: it would entangle pure
  resolution logic with org policy and duplicate the `enabled`/pins model it already has. A
  projection keeps the resolver pure and the policy independently testable.
- **A new "policy authority" that can grant cross-store rights** — rejected: expands the trust
  model. Tier 0 policy only *restricts*; granting authority would reopen the namespace-safety
  question ADR-0001/0002 closed.
