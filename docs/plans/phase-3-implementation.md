# Phase 3 (Tier 0) — Governable & deployable in an enterprise

**Status**: Draft / planning
**Date**: 2026-06-14
**Parent**: [ADR-0001 (Pluggable cog stores)](../adr/ADR-0001-pluggable-cog-stores.md) ·
[Phase 2 plan](phase-2-implementation.md) ·
[Enterprise-readiness matrix](../strategy/enterprise-readiness-matrix.md) ·
[Cog Store Protocol](../protocol/cog-store-protocol.md)

> Phase 2 made multi-store *possible* — N stores, namespacing, TOFU, per-store auth.
> Phase 3 starts making it *governable and deployable*. This doc plans the matrix's
> **Tier 0** — the three wins that are self-contained, demoable, and buildable **in this
> repo** with no Cognitum-run service: **air-gap bundle export**, an **audit / event log**,
> and **managed-mode policy**. Each is a procurement talking point on its own and a building
> block for the Tier-1 table-stakes that follow.

---

## 1. Goal & definition of done

Three independently shippable capabilities, each demoable end-to-end from this repo and each
pinned by a frozen test vector (the [contract](../protocol/testvectors/)):

1. **Air-gap export** — `gearbox export` produces a single self-contained, signed bundle
   (store-info + catalog + every artifact) that installs on a disconnected device from
   `file://` with the **same verification path** as an online store.
2. **Audit log** — every trust-affecting action (add-store, verify, install, policy decision)
   appends to a **tamper-evident** (hash-chained) local log that `gearbox audit verify` can
   validate offline.
3. **Managed mode** — a **signed policy document** the resolver enforces: allow-only-approved
   stores, disable the public store, force pins. On a managed device, an out-of-policy install
   is refused *and* the refusal is audited.

Definition of done: each has a Rust reference in `crates/gearbox`, a Python cross-check where
it touches signing/canonicalization, a frozen test vector, a `selftest.sh`/`examples/` demo,
and a protocol section (or ADR for the trust-model changes — managed mode needs **ADR-0003**).

## 2. Scope / non-goals

In scope (Tier 0 only): the bundle format + `export`/`import`; the audit log format +
`audit`; the managed-policy document + resolver enforcement + `policy` tooling.

**Non-goals (Tier 1+, later phases):** mTLS / cloud-IAM auth; RBAC + publish-approval
workflow; key **revocation** + transparency log; install-time **permission consent UX**;
SBOM / SLSA attestations; SSO/OIDC; any managed **control plane** or fleet-provisioning
service. Phase 3/Tier 0 deliberately ships **no server and no online identity** — everything
here is a file format + local enforcement, which is exactly why it fits this repo.

## 3. What earlier phases already gave us (the seams)

Tier 0 is additive, not a rewrite:

- **Signing + JCS + verify** (Phase 1, A4/B4) — export, audit, and policy all **reuse the
  existing `signing`/`jcs` core**; no new crypto.
- **`store.json` + TOFU** (Phase 2, [ADR-0002](../adr/ADR-0002-store-info-and-tofu.md)) — the
  bundle is just a store-info + catalog + artifacts laid out for `file://`; import reuses the
  TOFU fingerprint path.
- **`file://` already reserved** as an artifact scheme (Phase 2 §4, B2 fetcher) — export is
  the missing producer for a scheme the consumer already models.
- **The resolver** (`crates/gearbox/src/resolve.rs`) already models `enabled` + `priority` +
  pins. **Managed mode is a filter in front of it**, not a new resolver.

## 4. Workstream T0-A — Air-gap bundle export (`gearbox export`)

### 4.1 Format
A bundle is a directory (and a `.tar` of it) that is a **complete, verifiable store on a
filesystem**:

```
acme-bundle/
  store.json                      # store-info (self-signed; Phase 2 §4)
  app-registry.json               # signed catalog (protocol §3/§7)
  artifacts/cogs/<arch>/…         # every binary + asset the catalog references
  manifest.json                   # bundle manifest: schema_version, store_id, catalog sha256,
                                  #   generated_at, file list with per-file sha256
  manifest.sig                    # detached signature over JCS(manifest.json)
```

The bundle manifest is signed with the **same key** that signs the catalog, so import has one
trust anchor to check. Every file is hashed in `manifest.json`; nothing is trusted by path.

### 4.2 CLI
- `gearbox export --catalog app-registry.json --store-info store.json --artifacts-dir DIR \
    --out acme-bundle [--sign-seed-hex HEX --key-id ID]` — copy referenced artifacts, compute
  hashes, write + sign `manifest.json`, optionally `tar`.
- `gearbox import <bundle|bundle.tar>` — verify `manifest.sig` against the store's pinned key
  (TOFU on first import), check **every** artifact hash against the signed catalog, then hand
  the verified catalog to the normal install path with `artifact_base = file://…`.

### 4.3 Verification path (must match online)
Import runs the *identical* `verify_catalog` + per-artifact `sha256` checks as an online
fetch. The only difference is the fetcher scheme (`file://`). An air-gapped install is
therefore **no less trusted** than an online one — the demoable claim procurement wants.

## 5. Workstream T0-B — Audit / event log

### 5.1 Format
An append-only JSONL log; each line is a record **hash-chained** to the previous one
(Merkle-style), so any edit/truncation is detectable offline:

```jsonc
{
  "seq": 42,
  "ts": "2026-06-14T15:00:00Z",
  "event": "install",              // add_store | verify_catalog | install | policy_deny | key_change
  "subject": "acme-internal/doom@1.2.0",
  "detail": { "sha256": "…", "key_id": "acme-signing-2026", "result": "ok" },
  "prev": "<sha256 of the previous record's canonical bytes, or 64 zeros for seq 0>",
  "self": "<sha256 of JCS(this record without `self`)>"
}
```

`prev`/`self` reuse `jcs` + `sha2` — no new primitives. (Optional, later: periodically sign
the head `self` to make the log not just tamper-**evident** but tamper-**proof**; deferred to
keep Tier 0 keyless and local.)

### 5.2 CLI / integration
- `gearbox audit append --log a.jsonl --event … --subject … [--detail k=v]` — append one
  chained record.
- `gearbox audit verify --log a.jsonl` — recompute the chain; exit non-zero on any break,
  printing the first bad `seq`.
- The seed runtime calls the append path at each trust-affecting moment (B-series hooks);
  `gearbox` ships the reference format + verifier so the log is auditable with no Cognitum
  service.

## 6. Workstream T0-C — Managed-mode policy

### 6.1 The policy document (`policy.json`, → ADR-0003)
A **signed** document an admin distributes to managed devices (MDM, image bake, or bundle):

```jsonc
{
  "schema_version": 1,
  "managed": true,
  "allow_stores": ["acme-internal"],   // only these store ids may be enabled
  "deny_public": true,                  // the built-in official store is disabled
  "forced_pins": { "doom": "acme-internal" },  // admin pins; users cannot override
  "allow_user_add_store": false        // can a user add their own store via TOFU?
}
```

Signed (JCS + Ed25519) by an **org policy key** provisioned out-of-band (image/MDM). On a
managed device, an unsigned or wrong-key policy is ignored (fail-closed to the unmanaged
default is **not** acceptable when `managed:true` is already trusted — see §8).

### 6.2 Enforcement — a filter in front of the resolver
The resolver already models `enabled`/`priority`/pins, so policy is applied as a
**pre-resolution projection**, keeping `resolve.rs` pure:

1. Drop stores not in `allow_stores`; force-disable the public store if `deny_public`.
2. Inject `forced_pins` as pins the user layer cannot override.
3. If `allow_user_add_store == false`, the TOFU add-store flow is refused (and audited).

A managed `resolve()` of an out-of-policy reference returns the existing typed error
(`StoreDisabled` / `NotFound`); the CLI/seed surfaces it as a **policy denial** and writes a
`policy_deny` audit record (§5). Net new code is a small `policy` module + a `Policy::project(
stores, pins) -> (stores, pins)` step; the resolution rules themselves are unchanged.

### 6.3 CLI
- `gearbox policy create … --sign-seed-hex … --key-id …` / `gearbox policy verify policy.json`
  — author/verify the signed document (reuses `signing`).
- `gearbox policy check --policy policy.json --stores stores.json --ref doom` — show what a
  managed device would resolve (and why), for ops dry-runs.

## 7. Cross-cutting: contract, parity, CI

Every new signed artifact (`manifest.json`, `policy.json`) and the audit chain gets a **frozen
test vector** under `docs/protocol/testvectors/`, and where it touches signing it gets a
Python cross-check so the existing **parity** CI job covers it byte-for-byte. New CLI paths
get `selftest.sh` cases and an `examples/` demo (mirroring `examples/store-loop.sh`). No Tier-0
feature merges without: vector + (Rust, and Python if it signs) + selftest + docs.

## 8. Security

- **Air-gap import is not a trust downgrade** — same `verify_catalog` + per-artifact hash
  checks as online; only the transport (`file://`) differs. The bundle manifest is signed so a
  swapped artifact fails verification, not just a hash mismatch on one file.
- **Audit log is tamper-evident, offline** — the hash chain detects any edit/truncation with
  no server. It is **evidence**, not an access control; it never holds secrets (no tokens, no
  seeds), only key ids and hashes.
- **Managed policy is fail-closed** — once a device is provisioned `managed:true` with a
  policy key, a missing/invalid policy must **deny**, not silently revert to the open default;
  otherwise stripping the policy file would be a trivial bypass. Policy changes are
  signature-gated by the org key and audited.
- **No new authority** — policy only ever **restricts** (deny stores, force pins). It cannot
  grant a store rights over another store's namespace; Phase 2's per-store-key rule stands.
- **No new crypto / no new network surface** — Tier 0 reuses `jcs`/`signing`/`sha2` and adds
  no listening service. (`gearbox serve` is unchanged and out of scope here.)

## 9. Sequencing

```
T0-A export ──┐ (independent; reuses signing — ship first as the cleanest demo)
T0-B audit  ──┼─► all three are independent; B underpins C's policy_deny record
T0-C policy ──┘     so land B before (or with) C
```

All three are independent enough to parallelize, with one soft edge: land **T0-B (audit)**
before or with **T0-C (managed mode)** so policy denials are recordable from day one.
Recommended order by demo value + isolation: **A → B → C**.

## 10. Acceptance criteria

- **Export/import:** `gearbox export` produces a bundle; on a network-isolated run,
  `gearbox import` verifies it and an install succeeds via `file://`; a single tampered
  artifact byte makes import **fail**. Frozen bundle-manifest vector; Rust↔Python parity.
- **Audit:** a scripted add-store → verify → install sequence yields a chained log;
  `gearbox audit verify` passes; editing any record (or its order) makes it **fail** at the
  right `seq`.
- **Managed mode:** with a signed `deny_public` + `allow_stores:[acme]` + forced-pin policy,
  resolving `doom` yields the ACME cog; resolving a public/unlisted store is **denied** and a
  `policy_deny` record is written; an unsigned/forged policy is rejected (fail-closed).

## 11. Recommendation & implementation language

Build all three in the **canonical Rust crate** (`crates/gearbox`), reusing the existing
`jcs`/`signing`/`store`/`resolve` modules; keep the **Python tools as the cross-check oracle**
for anything that signs (export manifest, policy) so the parity gate keeps both honest. Start
with **T0-A (export)** — it's the highest-signal enterprise demo ("install with no internet,
fully verified") and reuses the most existing code. Record the managed-mode trust-model change
as **ADR-0003** before T0-C lands. Each Tier-0 item completed flips a matrix row from
*Not started* → *Built*; together they take §3 (governance), §4 (audit), and §5 (air-gap) off
the "0% built" list and set up the Tier-1 work (mTLS, RBAC, revocation, consent).
