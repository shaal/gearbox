# Research: Improving & Enhancing the Cog Store

**Status**: Research
**Date**: 2026-06-10
**Scope**: Forward-looking features for the Cognitum cog store, beyond the multi-store
foundation in [ADR-0001](../adr/ADR-0001-pluggable-cog-stores.md).

> This document researches *how to make the cog store better* — not just "more stores,"
> but a richer, safer, edge-aware distribution platform. It starts from an audit of what
> exists today, then proposes enhancements grouped into nine themes, prioritized by
> impact vs. effort, with a deliberate emphasis on the things that are **specific to
> constrained edge hardware** (Pi Zero 2 W class) rather than generic app-store features.

---

## 1. Where the store is today (audit)

A direct read of the repo (`cog.toml` manifests, `.github/workflows/ci.yml`, ADRs) shows
the store is a thin, declarative layer. What exists vs. what's missing:

| Area | Today | Gap |
|---|---|---|
| **Versioning / updates** | `version` in `[cog]`; independent per-cog versions (ADR-001) | No update, rollback, pinning, channels, or version ranges |
| **Discovery** | `name`, `description`, `category` | No icons, screenshots, tags, search, ratings, collections, i18n |
| **Dependencies** | None — ADR-001 deliberately rejects shared crates; cogs talk via RuVector | No capability/asset deps, no shared-asset dedup |
| **CI / validation** | `manifest-validate`, `asset-sha256`, `adr-required` gates | No signing, license, or permission audit |
| **Trust / permissions** | `[mesh] permissions`, `[api] auth` declared; sha256 only | No signing/provenance, no install-consent UX (ADR-0001 proposes signing) |
| **Compat / resources** | `hardware_requirement(s)`, `[resources] ram_mb/cpu_pct` | Single `-arm` artifact; no pre-flight fit check, no multi-arch select |
| **Telemetry / health** | None | No `/health` contract, crash reports, or operator metrics |
| **Licensing / pricing / i18n** | advisory `license` string | No SPDX validation, license policy, paid cogs, or localization |

The store is **correct and minimal**. The opportunity is to grow it into a platform
without violating ADR-001's core principle: **cogs stay independent — no lockstep
upgrades, no shared code crate.** Every enhancement below respects that (composition is
capability/asset-level, never code-level).

---

## 2. Enhancement themes

Each theme: the gap → the proposal → why it matters → where it lives
(M = cog manifest, C = catalog, S = Seed runtime, CI, G = Gearbox tooling).

### A. Lifecycle: updates, rollback, channels  ★ highest leverage

Today a cog has a version but no way to *move between* versions. Propose:

- **Update channels** per store: `stable` / `beta` / `edge`. A cog version is published
  to a channel; a device subscribes per-cog. (C, S)
- **Semver + version ranges + pinning**: a device can track `^0.1`, pin `=0.1.0`, or hold.
  Catalog exposes the full version list. (C, S)
- **Atomic update with auto-rollback**: install new version alongside old, flip a symlink,
  run a post-install **health probe**; on failure or crash-loop, roll back automatically.
  Critical on unattended edge devices with no operator present. (S)
- **Delta updates (bsdiff/zstd-dict)**: ship binary diffs between versions instead of full
  artifacts. A 2 MB cog patch instead of a 20 MB re-download matters enormously on
  metered/slow edge links. (C, S, G)
- **Yank / revoke**: mark a version withdrawn (`yanked`) or unsafe (`revoked`); Seeds stop
  offering it and warn/auto-update away from `revoked`. (C, S)
- **Changelogs**: per-version notes surfaced in the dashboard. (M/C)

### B. Discovery & merchandising

Beyond name/description/category:

- **Visual metadata**: `icon`, `screenshots[]`, `cover` — assets in the catalog, lazy-loaded. (M/C)
- **Tags + full-text search** over name/description/tags/category. (C, S)
- **Trust/quality signals** without a heavy review system: `verified` (signed by a trusted
  publisher), `editor's pick`/`featured` (curated collections), and **privacy-preserving
  popularity** (coarse, opt-in, aggregated install counts — never per-device). (C, S)
- **Collections / bundles**: "Home Safety pack" = a curated set installed together. (C)
- **"Works on your device" filter**: hide cogs the current Seed can't run (ties to E). (S)
- **Localized listings (i18n)**: `name`/`description` per locale. (M/C)
- **Related cogs / "because you installed…"** from category + capability adjacency (on-device,
  no cloud profiling). (S)

### C. Composition & dependencies (capability-level only)

ADR-001 forbids shared *code*. But cogs already cooperate via RuVector and capabilities.
Safe, non-lockstep composition:

- **Capability requirements**: a cog declares `requires_capability = ["llm.generate"]`; the
  Seed resolves it to *any* installed cog that *provides* that capability — interface, not
  implementation. (M, S)
- **Shared-asset dedup**: two cogs needing the same 135M GGUF model download it **once**
  via content-addressed storage (sha256-keyed). Big win for multi-cog AI devices. (C, S)
- **Bundles/suites** (see B) as the user-facing form of "install these together." (C)
- **Soft hints**: "pairs well with" links, advisory only — no hard dependency graph to
  resolve, preserving independence. (M/C)

### D. Trust, security & supply chain  ★ (extends ADR-0001)

ADR-0001 adds catalog signing. Build the rest of the supply chain on it:

- **Build provenance / attestations** (SLSA-style): catalog records *how* a binary was
  built (source commit, builder, toolchain). (C, CI, G)
- **SBOM per cog** (CycloneDX/SPDX): what's inside, for vuln scanning. (CI, G)
- **Permission/consent UX**: at install, show the cog's requested powers —
  `[mesh] permissions`, `[api]` ports, sensor access — and require consent. On **update**,
  **diff** the permissions and re-prompt only if they *grow* ("now also wants the camera").
  This is the single biggest user-facing safety upgrade. (S)
- **License policy gates**: validate `license` as SPDX; let a managed fleet *block*
  categories (e.g. "no GPL on this fleet" — directly relevant to the DOOM/GPL case). (CI, S)
- **Vulnerability/advisory feed**: a store can publish advisories that flag installed cogs;
  Seeds surface "update available (security)". (C, S)
- **Key rotation + revocation + transparency log**: overlap windows for keys, a revocation
  list, and optional Rekor-style inclusion proofs for public stores. (S, G)
- **Reproducible builds**: independent rebuild → identical hash, so the signature attests a
  *verifiable* artifact. (CI, G)

### E. Compatibility & resource gating  ★ edge-critical

Today: one `-arm` binary + a coarse RAM/CPU hint. Propose a real compatibility contract:

- **Structured compatibility matrix**: `arch` (armv6/armv7/arm64/riscv64/wasm32),
  `seed_min_version`, required peripherals (`camera`, `mic`, `i2c:0x76`). (M/C)
- **Multi-arch artifacts + auto-select**: catalog lists per-arch binaries; the Seed picks
  the right one. Unblocks non-Pi Seeds. (C, S)
- **Pre-flight resource check**: before download, verify the cog *fits* remaining RAM/flash;
  refuse with a clear reason instead of OOM-killing at runtime. (S)
- **WASM-first portable cogs**: a `wasm32` artifact runs on *any* Seed arch — a universal
  fallback target and a strong story for "write once, run on every Seed." (M, S)
- **Graceful "why incompatible"**: explain (missing sensor / too little RAM / arch), don't
  just hide. (S)

### F. Reliability & observability

The store should know whether what it ships actually works:

- **Standard `/health` + `/metrics` contract** for API cogs (ADR-095 cogs already bind HTTP).
  A cog that doesn't pass health post-install triggers rollback (A). (M, S)
- **Opt-in, privacy-preserving crash reports & telemetry**: aggregated on-device first,
  coarse counts only, operator-controlled. Powers "this version crashes" signals. (S)
- **Operator health dashboard**: install/uninstall success, crash rate, version spread
  across a fleet. (S, store-side)
- **Crash-rate auto-halt**: a staged rollout (H) automatically pauses if the new version's
  crash rate spikes. (S)

### G. Developer experience & publishing

Lower the cost of making and shipping a cog:

- **`gearbox` CLI**: `new` (scaffold), `validate` (run the CI gates locally), `build`
  (multi-arch cross-compile), `sign`, `publish`, `catalog` (generate `app-registry.json`). (G)
- **Local dev store**: `gearbox serve ./dist` → a `file://`/`http://` store you can point a
  test Seed at, for end-to-end testing without touching production. (G)
- **JSON Schema for `cog.toml`**: editor autocomplete + validation; the same schema the CI
  `manifest-validate` gate uses. (G, CI)
- **Keyless CI signing (OIDC)**: publish from CI signing with a workload identity instead of
  a long-lived key. (CI, G)
- **Cog templates** per archetype (sensor→DSP, API/MCP cog, game) — the doom/cognitive-pipeline
  patterns as cookiecutters. (G)
- **Staging/preview stores**: publish to a preview channel, get a shareable install link
  before promoting to `stable`. (C, G)

### H. Distribution & operations  ★ edge-critical

Where bytes actually come from, optimized for fleets and bad networks:

- **Pluggable backends** (`gs`/`s3`/`oci`/`https`/`file`) — the ADR-0001 fetcher abstraction.
  `oci://` lets enterprises reuse Harbor/Artifactory/ECR. (S)
- **LAN peer caching**: in a fleet, one Seed downloads a cog and **peers pull from it over
  the LAN** instead of each re-fetching from the cloud. Dramatic bandwidth savings for
  a room/site full of devices. (S)
- **Air-gap bundles**: `gearbox export` produces a single signed bundle (catalog + artifacts)
  that installs from USB/NAS with no internet — first-class, not a hack. (G, S)
- **Content-addressed storage + dedup**: artifacts keyed by sha256, so shared assets and
  unchanged layers between versions are stored/transferred once (pairs with A's deltas and
  C's shared assets). (C, S, G)
- **Resumable / range downloads**: survive flaky links on a 28 MB IWAD. (S)
- **CDN + mirrors**: a store can list mirror bases; the Seed picks the nearest/healthy one. (C, S)

### I. Governance & monetization (forward-looking)

- **Paid / licensed cogs**: entitlement tokens / license keys checked at install; out of
  scope near-term but the signing + auth primitives already support it. (S)
- **Org-scoped private listings**: a cog visible only to members of an org/store. (C, S)
- **Store federation**: search across several configured stores in one dashboard view, with
  per-store badges. (S)
- **Moderation / deprecation workflow** for public stores: report, review, deprecate. (store-side)

---

## 3. Priority

Impact vs. effort, biased toward edge-specific differentiation. "Impact" = user/operator
value; "Effort" = rough build cost across cogs+Seed+Gearbox.

| Enhancement | Theme | Impact | Effort | Notes |
|---|---|---|---|---|
| Atomic update + auto-rollback + health probe | A,F | ★★★★★ | M | Table-stakes for unattended edge; nothing else matters if updates brick devices |
| Install/update **permission consent + diff** | D | ★★★★★ | M | Biggest safety win; depends on ADR-0001 signing |
| **Delta updates** + content-addressed dedup | A,H | ★★★★☆ | M | Edge bandwidth; compounding wins with shared assets |
| Update channels + version ranges/pinning | A | ★★★★☆ | M | Foundation for staged rollout |
| Multi-arch + **WASM-first** + pre-flight fit check | E | ★★★★☆ | M | Unblocks non-Pi Seeds; clean "won't fit" UX |
| **LAN peer caching** | H | ★★★★☆ | L | Standout fleet feature; networking-heavy |
| `gearbox` CLI + local dev store + JSON Schema | G | ★★★★☆ | M | Unlocks contributors; multiplies everything |
| Visual metadata + tags + search | B | ★★★☆☆ | S | High polish, low risk |
| Shared-asset dedup + capability deps | C | ★★★☆☆ | M | Big for multi-cog AI devices; respect ADR-001 |
| Provenance + SBOM + reproducible builds | D | ★★★☆☆ | L | Enterprise trust; CI-heavy |
| Air-gap export bundles | H | ★★★☆☆ | M | Critical for a subset (defense/industrial) |
| Telemetry/crash + operator dashboard | F | ★★★☆☆ | M | Powers auto-halt rollout; privacy design needed |
| Ratings/curation/collections | B | ★★☆☆☆ | M | Needs catalog scale to be meaningful |
| Paid cogs / federation / moderation | I | ★★☆☆☆ | L | Defer until multi-store + discovery land |

## 4. Sequencing (suggested)

1. **Foundation (with ADR-0001):** signing + multi-store + the fetcher abstraction.
2. **Safe lifecycle:** atomic update + auto-rollback + health probe; channels + pinning;
   permission consent/diff. *(The "never brick a device, never silently gain power" tier.)*
3. **Edge efficiency:** delta updates + content-addressed dedup + LAN peer caching; multi-arch
   + WASM + pre-flight fit.
4. **Platform polish:** discovery (visual metadata, search, curation); `gearbox` CLI + dev
   store; provenance/SBOM; air-gap bundles.
5. **Ecosystem:** telemetry/operator dashboards; governance/monetization.

## 5. Design tensions to respect

- **Independence (ADR-001):** never introduce shared *code* deps or lockstep upgrades.
  Composition stays at the capability/asset interface.
- **Privacy:** discovery popularity and telemetry must be opt-in, aggregated, on-device-first —
  edge users chose local-first for a reason.
- **Constrained hardware:** every feature pays a RAM/flash/CPU cost on the device. Prefer
  designs that move work to publish-time (Gearbox/CI) over install-time (Seed).
- **Trust boundary ≠ sandbox:** richer discovery never relaxes runtime containment. A cog from
  a fancy listing is still gated by permissions, ports, and resource budgets.

## 6. Open questions

- Where does delta/CAS logic live — Seed, Gearbox, or a shared crate? (Probably a small shared
  crate consumed by both, *not* shared by cogs.)
- Telemetry: is *any* network callback acceptable by default, or strictly operator-enabled?
- WASM cogs: performance ceiling vs. native `-arm` for the sensor→DSP hot path — measure first.
- Curation authority: who marks `featured`/`verified` for the official store, and how is that
  exposed to alternative stores without implying Cognitum endorsement?
