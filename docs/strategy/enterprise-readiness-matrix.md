# Enterprise-readiness gap matrix

**Status**: Roadmap / capability status (working doc)
**Date**: 2026-06-11

> An honest, capability-by-capability status of what enterprise deployment requires vs. what
> exists today, with evidence — and a prioritized road to close the gaps. It seeds a Phase-3
> plan that does not yet exist (today Phase 3 lives only as "non-goals" in the
> [Phase 2 plan](../plans/phase-2-implementation.md) and a rollout bullet in the
> [Phase 1 plan](../plans/phase-1-implementation.md)).

Status legend: **Built** (implemented + tested) · **Partial** · **Specced** (design only) ·
**Not started**.

---

## 1. Supply-chain security — *our strength*

| Capability | Enterprise expectation | Status | Evidence |
|---|---|---|---|
| Artifact / catalog signing | Code signed before distribution | **Built** | protocol §7, `crates/gearbox` signing, testvectors, A4/B4 |
| Independent verification | Anyone can verify, no trust in transport | **Built** | Rust + Python + OpenSSL agree byte-for-byte |
| Reproducible / deterministic build | Re-run → identical artifact (auditable) | **Built** | Ed25519 determinism + passed-in `generated_at` |
| Integrity at install | Hash-checked against the *signed* manifest | **Built** (spec) | B5 spec; demo checks sha256 against verified catalog |
| Store identity + trust bootstrap | Provenance of *who* published | **Built** (ref) | ADR-0002, `store.json`, `store-info`, demo |
| SBOM per cog | Component inventory for vuln mgmt | **Not started** | enhancements §D |
| Build provenance / attestations (SLSA) | "How was this built" | **Not started** | enhancements §D (determinism is a partial enabler) |
| Vulnerability / advisory feed | Flag installed cogs with CVEs | **Not started** | enhancements §D |

**Read:** the cryptographic core is genuinely strong and ahead of most. The *attestation /
SBOM / vuln* layer that procurement increasingly checks is unbuilt.

## 2. Identity & access

| Capability | Expectation | Status | Notes |
|---|---|---|---|
| SSO / SAML / OIDC | Corporate identity for console/admin | **Not started** | — |
| RBAC (who can publish / approve / install) | Least-privilege roles | **Not started** | — |
| Private-store transport auth | Authenticated artifact access | **Partial** | **bearer only** (server + B2/E spec); mTLS / cloud-IAM are Phase 3 |
| Per-publisher signing keys / key custody | Separation of duties | **Partial** | key model exists (key_id, rotation-ready); custody policy set ([plan §6](../plans/phase-1-implementation.md)), no RBAC around it |

## 3. Governance & policy

| Capability | Expectation | Status | Notes |
|---|---|---|---|
| Managed mode / policy lockdown | Restrict devices to approved stores | **Specced** | Phase 3; resolver already models enabled/priority/pins |
| Allow / deny store lists | Block the public store on managed fleets | **Specced** | ADR-0001 §4.5, Phase 2 §7 |
| Publish approval workflow | Review before a cog ships | **Not started** | — |
| Capability/permission consent at install | "This cog wants mesh + camera" + diff on update | **Partial** | `[mesh]`/`[api]` declared in cog.toml (cogs); consent UX unbuilt (enhancements §D) |
| Namespacing (anti-shadowing) | A new store can't impersonate an official cog | **Built** | `resolve.rs` (store/cog, priority, pins) |

## 4. Auditability & compliance

| Capability | Expectation | Status | Notes |
|---|---|---|---|
| Audit / event log | Append-only trail of add/install/verify/policy | **Not started** | a Tier-0 proof point |
| Key rotation overlap + revocation | Rotate/kill a compromised key | **Partial / Specced** | date-scoped `key_id` shipped; overlap + revocation list deferred (protocol §9) |
| Transparency log (Rekor-style) | Tamper-evident publish record | **Not started** | protocol §9 open item |
| Compliance posture (SOC2 / FIPS / pen-test) | Procurement gate | **Not started** | N/A until there's a service to certify |

## 5. Connectivity & deployment

| Capability | Expectation | Status | Notes |
|---|---|---|---|
| On-prem / self-hosted store | No dependency on a Cognitum-run service | **Built** (ref) | `gearbox serve`; any HTTPS/S3/GCS/OCI host |
| Air-gap / offline bundles | Install with no internet (USB/NAS) | **Not started** | `file://` stubbed; export bundle is a Tier-0 proof point (enhancements §H) |
| Pluggable artifact backends | Reuse Harbor/Artifactory/ECR/S3 | **Partial** | scheme-keyed fetcher; `gs`/`https` built, `s3`/`oci`/`file` stubbed (B2) |
| LAN peer caching | Bandwidth for fleets | **Not started** | enhancements §H |

## 6. Operations & lifecycle

| Capability | Expectation | Status | Notes |
|---|---|---|---|
| Update channels / rollback / delta | Safe fleet updates | **Not started** | enhancements §A |
| Health / telemetry / crash signals | Know if a rollout is healthy | **Not started** | enhancements §F |
| Fleet provisioning / device management | Onboard/manage devices at scale | **Not started** | Phase 3 |
| Support SLA / lifecycle policy | Enterprise support | **Not started** | commercial, not technical |

---

## Road to enterprise-ready (priority)

This is effectively the **seed of a Phase-3 plan**. Ordered: in-our-control, demoable wins
first; then first-deployment table-stakes; then compliance.

**Tier 0 — demoable, in-our-control:**
1. Air-gap **bundle export** (`gearbox export`) — §5.
2. **Audit / event log** — §4.
3. **Managed-mode policy** (allow-only-approved-stores, enforced pins) — §3.

**Tier 1 — first-deployment table-stakes:**
4. **mTLS / cloud-IAM** private-store auth — §2.
5. **RBAC** for publish / approve / install + approval workflow — §2/§3.
6. **Key rotation overlap + revocation** — §4.
7. Install-time **permission consent + diff** — §3.

**Tier 2 — compliance:**
8. **SBOM + provenance attestations** (SLSA) — §1.
9. **Vulnerability / advisory feed** — §1.
10. **SSO/OIDC** for the console — §2.
11. **SOC2** path (once a managed control plane exists) — §4.

**Status read:** supply-chain security (§1) is the only fully-built domain today; everything
else is **roadmap**. "Enterprise-ready" is honestly a Tier-1-complete bar — not today's.
