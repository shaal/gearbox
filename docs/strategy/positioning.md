# Positioning & GTM critique (internal strategy)

**Status**: Internal strategy — not marketing copy
**Date**: 2026-06-11
**Companion**: [Enterprise-readiness gap matrix](enterprise-readiness-matrix.md)

> This memo deliberately **reframes** the initial "position Gearbox as cog stores for
> enterprise companies" framing. It's a critical review, written to keep the *claim* and the
> *product* in sync. It is internal: blunt about gaps, not a pitch.

---

## TL;DR (verdict)

Private/enterprise is the **strongest commercial wedge** in this initiative — the instinct
is right. But "cog stores **for enterprise**" as the headline **overclaims today** and hides
a sequencing trap. Position it precisely:

> **Sovereign, supply-chain-secure distribution for fleets of edge-AI devices** — a pillar
> of an *enterprise-deployable Seed platform*, not a standalone "enterprise cog store."

Lead with the security architecture we can **prove and demo**. Treat governance / identity /
compliance as the explicit, honest road to "enterprise-ready" (it's mostly unbuilt). And
recognize that the **control plane**, not the open protocol, is the thing enterprises pay for.

---

## 1. Why the wedge is real (and partly earned)

- **Regulated / edge / OT orgs structurally cannot use a public marketplace.** They will not
  let factory-floor, clinical, or field devices phone home to someone else's store. "Your
  infra, your keys, your artifacts, air-gappable, no phone-home" is a *requirement* that
  public app stores fail — not a nice-to-have.
- **We built the part most stores get wrong.** A signed, deterministically-verifiable,
  reproducible supply chain with trust-on-first-use and three cross-checked implementations
  ([ADR-0001](../adr/ADR-0001-pluggable-cog-stores.md), [ADR-0002](../adr/ADR-0002-store-info-and-tofu.md),
  [protocol §7–8](../protocol/cog-store-protocol.md), `crates/gearbox`). That aligns with
  where procurement is heading (SLSA, SBOM mandates, EO 14028) and it is **demoable today**
  (`examples/store-loop.sh`). Most projects bolt this on late; doing it first is a genuine
  differentiator.

## 2. Where the framing breaks under scrutiny

1. **A cog store is a deal-*enabler*, not a deal-*driver*.** Nobody buys a store before
   adopting Seeds + cogs at scale. Leading with "the store" sells plumbing for a platform the
   buyer hasn't committed to. The enterprise story is "*the Seed platform is
   enterprise-deployable*," with private distribution as one necessary-but-not-sufficient
   pillar.
2. **"Enterprise" is a maturity claim we can't yet back.** The capabilities enterprises
   actually *procure on* — SSO/RBAC, approval workflows, audit, managed policy, mTLS,
   revocation, SOC2 — are mostly unbuilt (see the [gap matrix](enterprise-readiness-matrix.md)).
   Leading with "enterprise" invites exactly the checklist we'd fail in a POC, eroding the
   credibility the *security* story earned.
3. **Category confusion.** "Cog store" is novel jargon. Buyers map to known categories:
   **private artifact registry** (Harbor / Artifactory / ECR) + **edge fleet / device
   management** (MDM) for AI devices. Use those analogies or the first meeting is a vocabulary
   lesson.
4. **Commoditization risk.** Artifact hosting + signing is increasingly solved (Harbor +
   Cosign/Sigstore). The moat is **not** "we have a registry" — it's the Seed-specific
   **manifest + capability/permission model + on-device install/verify/runtime integration**.
   Positioning as "a registry" invites a build-vs-buy comparison we lose.
5. **Monetization inversion.** The protocol + reference should stay **open** — that grows
   cogs and Seeds, which is the flywheel. Enterprises pay for the **governed control plane**
   (publish approval, fleet policy, audit, RBAC) + support + compliance certs. If "the store"
   is the product, we're trying to monetize the part that should be free.

## 3. Recommended positioning

- **One-liner:** *Sovereign, supply-chain-secure distribution for edge-AI fleets.*
- **Frame:** the Cognitum **platform** is enterprise-deployable; private/sovereign cog
  distribution (Gearbox) is a first-class pillar of that — not the headline product.
- **Lead with the proven, demoable security architecture:** signed, verifiable, reproducible,
  your-keys, air-gap-capable, no-phone-home. Show the TOFU → verify → install demo.
- **Do not lead with governance features that don't exist.** State the enterprise-readiness
  roadmap honestly (= the gap matrix), and sequence it deliberately.
- **Treat the control plane as the product.** Keep the protocol open; build/sell the governed
  management layer.

## 4. Sharpest ICP (and the honest caveat)

**Target:** regulated / sovereignty-conscious orgs running **fleets of edge-AI devices that
cannot phone home** — manufacturing & OT, healthcare, defense / public sector, critical
infrastructure.

**Caveat:** these are also the **slowest, most compliance-heavy** buyers, and the platform
is not ready for them yet (no air-gap export, no RBAC/SSO, no audit, no SOC2). So they are the
right *target* but it's the wrong *timing* to claim "ready." The near-term move is design
partners + proof points, not a procurement-ready pitch.

## 5. Messaging do / don't

| Do | Don't |
|---|---|
| "Sovereign / private distribution for edge-AI fleets" | "Enterprise cog store" (jargon + overclaim) |
| Analogize to private registry + edge MDM | Assume buyers know "cog" / "Seed" |
| Lead with signed + verifiable + air-gap-capable (demo it) | Lead with governance/RBAC/audit (unbuilt) |
| "Pillar of an enterprise-deployable platform" | "The enterprise product" (it's a layer) |
| Differentiate on Seed manifest + runtime trust integration | Compete as "a registry" (commoditized) |

## 6. Near-term proof points (move "enterprise" from claim → evidence)

These back the positioning faster than more cryptography, and buyers actually ask for them:

1. **Air-gap bundle export** — `gearbox export` produces a signed, offline-installable bundle
   (store.json + catalog + artifacts); install from USB/NAS with no internet. *Most-asked by
   the sharpest ICP; directly demoable on top of the existing signing core.*
2. **Audit / event log** — an append-only record of add-store / install / verify / policy
   events. *Table-stakes for any regulated buyer; cheap to start.*
3. **Managed-mode policy** — "allow only approved stores" + pinning enforced by device policy
   (the resolver already models enabled/priority/pins). *Turns the multi-store core into a
   governance story.*

Each is in-our-control, demoable, and seeds the (currently unwritten) Phase-3 plan.

## 7. What this memo deliberately does *not* recommend

- Branding Gearbox itself as "the enterprise cog store."
- Leading sales with governance/identity/compliance language before those exist.
- Monetizing the protocol/spec (keep it open).
- Selling to procurement-heavy regulated buyers as "ready" before the Tier-0/Tier-1 items in
  the [gap matrix](enterprise-readiness-matrix.md) exist.

See the [enterprise-readiness gap matrix](enterprise-readiness-matrix.md) for the
capability-by-capability status and the prioritized road to "ready."
