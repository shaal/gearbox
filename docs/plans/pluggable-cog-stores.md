# Plan: Pluggable cog stores (private & alternative)

**Status**: Draft / research
**Date**: 2026-06-10
**Owner**: gearbox maintainers + cognitum-one/seed + cognitum-one/cogs (cross-repo)
**Related**: [ADR-0001 (Pluggable cog stores)](../adr/ADR-0001-pluggable-cog-stores.md),
[Cog Store Protocol](../protocol/cog-store-protocol.md),
[cognitum-one/cogs ADR-001 (Cogs as plugins)](https://github.com/cognitum-one/cogs/blob/main/docs/adrs/ADR-001-cogs-as-plugins-architecture.md),
[cognitum-one/seed ADR-095 (Cogs as API providers)](https://github.com/cognitum-one/seed/blob/main/docs/seed/ADR-095-cogs-as-api-providers.md)

> This is a research + design document, not an implementation. It defines what an
> "alternative cog store" means, decomposes today's implicit single store into the
> parts that must become pluggable, surveys how other ecosystems solved the same
> problem, and proposes a phased path. The durable decision is captured in ADR-0001;
> this plan is the long-form backing.

---

## 1. The ask

While building the `doom` cog we hit a recurring tension: not everyone wants to
install cogs from one central, Cognitum-operated store.

- **Enterprises** want a **private store**: their own internal cogs (and a vetted
  subset of public ones) hosted on infrastructure they control, behind their own
  auth, never touching the public internet — and often the ability to *forbid* the
  public store entirely on managed devices.
- **Other communities/projects** want **alternative public stores**: a competing or
  complementary catalog they operate, like F-Droid alongside Google Play, or Open
  VSX alongside the VS Code Marketplace.
- **Single-cog self-hosters** want to ship *one* cog from their own URL — the
  `doom` ADR already names this: a GPLv2 cog the maintainers may not want inside
  the MIT repo could instead live in "a separate repository referenced from the
  registry" ([cogs ADR-019](https://github.com/cognitum-one/cogs/blob/main/docs/adrs/ADR-019-doom.md)). Alternative stores are exactly
  that mechanism.

The questions this plan answers:

1. **What does "an alternative store" actually mean** in this architecture?
2. **How can it be done** — what becomes pluggable, and what's the contract?
3. **Do we need to change Cognitum Seed itself?** (Short answer: yes, most of the
   work is Seed-side; the `cogs` repo and this spec/tooling repo carry the rest.)
4. **How would people use it** — enterprise admin, community operator, hobbyist?

---

## 2. How the cog store works today

Decomposing the current system is the whole game: once we name the parts, "an
alternative store" is just "make these parts point somewhere else, safely."

### 2.1 The pieces

A "store" today is not one component — it's five concerns that currently all
resolve to a single hardcoded origin:

| Concern | What it is | Today |
|---|---|---|
| **Catalog / index** | The list of available cogs + versions + metadata that the dashboard browses | `app-registry.json` in the **seed** repo (companion to cog PRs, e.g. seed#154) |
| **Artifact storage** | Where binaries and assets physically live | One GCS bucket: `gs://cognitum-apps` |
| **Trust / provenance** | How the Seed knows an artifact is authentic and untampered | `sha256` per asset in `cog.toml`; implicit trust that the one bucket is official |
| **Transport / auth** | How the Seed reaches the store | Public GCS; effectively unauthenticated reads |
| **Resolution policy** | Given a cog id, which source serves it | N/A — there is exactly one source |

### 2.2 The cog manifest (`cog.toml`)

Every cog is a directory under `src/cogs/<id>/` with a declarative `cog.toml`.
Relevant sections (from the live `doom`, `cognitive-pipeline`, and `tailscale`
manifests):

- `[cog]` — `id`, `name`, `version`, `category`, `description`, `binary`,
  `hardware_requirement(s)`.
- `[config.*]` — per-parameter UI/CLI config (`type`, `label`, `default`,
  `min`/`max`, `cli_arg`, `secret`, `advanced`, `allowed`).
- `[resources]` — `ram_mb`, `cpu_pct`.
- `[api]` — `bind_port`, `bind_loopback_only`, `endpoints[]` (ADR-095 API cogs).
- `[mcp]`, `[mesh]` — MCP tools, mesh capability permissions.
- `[[assets]]` — **the part that matters most here.** Each downloadable asset:

```toml
# src/cogs/doom/cog.toml
[[assets]]
id          = "freedoom1-wad"
filename    = "freedoom1.wad"
size_bytes  = 28_795_076
sha256      = "7323bcc168c5a45ff10749b339960e98314740a734c30d4b9f3337001f9e703d"
gcs_path    = "wads/freedoom1.wad"     # <-- RELATIVE path, not a full URL
source_url  = "https://github.com/freedoom/freedoom/releases/download/v0.13.0/freedoom-0.13.0.zip"
license     = "BSD-3-Clause (FreeDoom)"
```

### 2.3 The install + provisioning flow

1. The Seed's install handler reads the catalog and the cog's `cog.toml`.
2. It downloads the binary from `gs://cognitum-apps/cogs/<arch>/<binary>` and each
   asset from `gs://cognitum-apps/cogs/<arch>/<gcs_path>`. For the arm-only `doom`
   cog that resolves to `gs://cognitum-apps/cogs/arm/wads/freedoom1.wad`.
3. It **sha256-verifies** every artifact against the manifest.
4. It places them at `<COG_DATA_DIR>` (`/var/lib/cognitum/apps/<cog-id>/`) and
   injects `COGNITUM_COG_TOKEN` + `COGNITUM_COG_DATA_DIR` at start.

CI in the cogs repo enforces the asset hashes (the `asset-sha256` gate) and the
`cog-<dirname>` binary naming.

### 2.4 The one load-bearing observation

**`gcs_path` is already relative.** The cog manifest never names the bucket — only
the path *within* it. The bucket base (`gs://cognitum-apps`) is a constant baked
into the **Seed**, not into any cog. That means the cog format is *already* almost
store-agnostic; the thing that's hardcoded to a single store lives in exactly one
place: the Seed runtime.

The corollary, and the reason this is non-trivial: **`sha256` protects the
download, not the decision to download.** Whoever controls the catalog chooses the
sha256, so the moment a Seed reads a catalog from a *second* origin, the hash
guarantees nothing about authenticity — only that the bytes match what that
(possibly hostile) catalog claimed. Crossing a trust boundary requires
**signatures**, not just hashes. This is the central security finding of the plan.

---

## 3. Prior art (so we don't reinvent the failure modes)

Every mature package ecosystem has faced "let people use a registry other than
ours." The patterns converge:

| Ecosystem | Add a store via | Namespacing | Trust model | Private/enterprise story |
|---|---|---|---|---|
| **npm** | `.npmrc` `registry=` + per-scope `@scope:registry=` | Scopes (`@acme/pkg`) | TLS + optional package signing/provenance | Verdaccio / Artifactory / GitHub Packages mirror |
| **Docker/OCI** | `docker pull host/repo:tag` (registry in the ref) | Registry host in image ref | Content digests + Cosign/Notary signatures | Harbor / ECR / Artifactory, often air-gapped |
| **Cargo** | `[registries]` in `.cargo/config.toml` | `pkg@registry` | Index + checksum | Private alt-registry (Cloudsmith, Artifactory) |
| **APT** | `/etc/apt/sources.list.d/*` | Per-repo | **GPG-signed `Release`** + per-key trust | Internal mirrors; pinning/priorities |
| **F-Droid** | "Add repository" by URL + **fingerprint** | Per-repo | Repo signed; **TOFU on first add** | Self-hosted repos are first-class |
| **VS Code / Open VSX** | Change the `serviceUrl` of the gallery | Publisher namespaces | Publisher verification | Open VSX is the canonical "alternative marketplace" |

Takeaways we adopt:

1. **A store is a named source = catalog URL + artifact base + trust config.**
   (APT `sources.list`, F-Droid repos.)
2. **Namespacing prevents id collisions** when two stores both ship `doom`.
   (npm scopes, Docker registry-in-ref.)
3. **Cross-boundary trust = signatures + a per-source trust anchor**, not hashes.
   (APT GPG-signed `Release`, Cosign.)
4. **Public stores: trust-on-first-use with a visible fingerprint.** Private
   stores: pre-provisioned by an admin (no end-user prompt). (F-Droid vs MDM.)
5. **The store is a discovery/distribution boundary, not a sandbox boundary.** A
   third-party cog is still gated at *runtime* by the existing permission model
   (loopback bind, bearer tokens, `[mesh] permissions`, capability grants). Adding
   a store must never be a way to bypass those.

---

## 4. Proposed design

### 4.1 Core concept: a **store descriptor**

The Seed stops holding one hardcoded base and instead holds a list of store
descriptors (the APT `sources.list` / F-Droid "repos" pattern):

```toml
# Seed-side config (illustrative)
[[store]]
id           = "cognitum-official"
name         = "Cognitum Official"
priority     = 0                                           # lower = higher precedence
catalog_url  = "https://registry.cognitum.dev/app-registry.json"
artifact_base = "gs://cognitum-apps/cogs"
trust        = ["cognitum-release-2026"]                   # signing keys this store must use
enabled      = true

[[store]]
id           = "acme-private"
name         = "ACME Internal"
priority     = 10
catalog_url  = "https://cogs.acme.internal/app-registry.json"
artifact_base = "https://cogs.acme.internal/artifacts"
auth         = { type = "bearer", token_ref = "secret://acme-store-token" }
trust        = ["acme-signing-2026"]
enabled      = true
```

### 4.2 Generalize artifact paths (small, back-compatible cog-side change)

In `cog.toml [[assets]]`, introduce a scheme-agnostic relative `path`, resolved
against the active store's `artifact_base`. Keep `gcs_path` as a deprecated alias
that means "resolve relative to the official GCS base," so **every existing
manifest keeps working unchanged**. The Seed grows a **fetcher abstraction** keyed
by the `artifact_base` scheme:

- `gs://` — GCS (today)
- `https://` — any static host / CDN / GitHub Pages (lowest barrier for community + single-cog hosting)
- `s3://` — S3-compatible object stores
- `oci://` — OCI registry (reuse enterprise Harbor/Artifactory/ECR)
- `file://` — LAN/NAS/USB mirror for **air-gapped** sites

The cog manifest stays **store-agnostic** — a cog never names which store ships it.

### 4.3 The catalog: a portable, signed `app-registry.json`

Promote `app-registry.json` from a seed-repo file to a **specified, portable,
signed artifact** any operator can generate and host. Per the schema sketch:

- `schema_version`, `store_id`
- `cogs[]`: `id`, `versions[]`, each version carrying the cog's manifest (or a
  pointer to its per-version `cog.toml`), the binary path + digest, and each
  asset's relative `path` + `sha256` + `size`.
- A **signature** over the catalog (or per-entry signatures) by the store's
  publisher key.

This is the contract both the Seed and any store operator implement. It lives as a
versioned **spec doc in this repo** ([`docs/protocol/cog-store-protocol.md`](../protocol/cog-store-protocol.md))
and is referenced by the Seed.

### 4.4 Trust store + verification (the non-negotiable part)

The Seed gains a **trust store**: a set of trusted ed25519 public keys
(`ed25519-dalek` is already a workspace dependency), each scoped to a store id (and
optionally a cog-id namespace). Install becomes:

1. Fetch catalog from the store → **verify catalog signature** against a key in
   `trust` for that store. Reject if unsigned/untrusted.
2. For each artifact, fetch → **sha256-verify** against the *signed* manifest.
3. Install only if both pass.

So a store can only ever install cogs signed by a key the Seed trusts for *that*
store. Official store → Cognitum release key (shipped with the Seed). Enterprise
store → the enterprise's own key (provisioned by their admin). Alternative public
store → ships its key + fingerprint; the user adds it via **TOFU** (fingerprint
shown and confirmed, like adding an SSH host key or an F-Droid repo).

### 4.5 Namespacing & resolution

Two stores may both offer `doom`. We resolve with:

- **Namespaced ids**: `cognitum-official/doom` vs `acme-private/doom` (npm-scope /
  Docker-ref model). The bare `doom` resolves by **store priority**.
- **Pinning**: a user/admin can pin a cog to a specific store.
- **Policy / allow-deny**: an enterprise can set `enabled = false` on the public
  store, or restrict the device to an allowlist of store ids ("managed mode").

### 4.6 Private-store auth & secrets

Private stores need transport auth: bearer token, mTLS client cert, or
cloud-provider IAM (GCS/S3 signed access). Auth secrets are **Seed device config**,
not `cog.toml` fields — they belong to the *store*, not the cog. (The cog manifest
already has `secret = true` for its *own* config params; store credentials are a
separate, Seed-managed secret store.)

---

## 5. What has to change, by component

### 5.1 Cognitum Seed (`cognitum-one/seed`) — the majority of the work

Yes — **the Seed must change.** The single hardcoded base is the whole reason
alternative stores don't exist today, and the install handler, trust, and UX all
live there.

1. Replace the hardcoded `gs://cognitum-apps` base with a **store registry** (list
   of descriptors) + config to add/remove/enable/prioritize stores.
2. **Multi-catalog loader**: fetch + merge catalogs across stores with namespacing
   and priority.
3. **Multi-scheme fetcher** (`gs`/`https`/`s3`/`oci`/`file`).
4. **Signature verification + trust store** (per-store trusted keys; revocation).
5. **Store auth** (bearer/mTLS/cloud-IAM) with secret handling.
6. **Resolution/policy engine**: priority, pinning, allow/deny, managed-mode
   lockdown for fleets.
7. **Dashboard/API**: list stores; "Add store" with fingerprint confirmation (TOFU);
   browse per-store catalogs; choose the source at install; managed-mode that hides
   "Add store" entirely.

### 5.2 `cognitum-one/cogs` — moderate

1. **Manifest**: add scheme-agnostic `path` to `[[assets]]`; keep `gcs_path` as a
   back-compat alias. No behavior change for existing cogs.
2. **Catalog generator**: a tool that walks `src/cogs/*/cog.toml`, emits a **signed**
   `app-registry.json`, and uploads artifacts to a **configurable** base. Today this
   is implicit/seed-side; make it explicit and ownable here.
3. **Publish pipeline / CI**: parameterize the artifact base, catalog destination,
   and signing key (`STORE_ARTIFACT_BASE`, `STORE_CATALOG_URL`, `STORE_SIGNING_KEY`)
   so the **same repo + pipeline** can publish to the official store *or* a fork can
   publish to *their own* store by changing config + secrets — **without forking the
   Seed.** This is what makes "run an alternative public store" a config exercise,
   not an engineering project.
4. **Docs/governance**: a "host your own cog store" runbook + store-operator
   responsibilities (key custody, asset hosting, vetting, revocation).

### 5.3 Shared spec — small but load-bearing

A versioned **store & catalog spec** — this repo's [`docs/protocol/cog-store-protocol.md`](../protocol/cog-store-protocol.md):
catalog JSON schema, store-descriptor schema, signing format, and the
fetch→verify→install algorithm. Both the Seed and any operator code to this
contract. It lives in **gearbox** (not `cogs` or `seed`) so any store operator can
implement it without cloning the cog monorepo or the device runtime.

---

## 6. How people use it

### Enterprise admin (private store)
1. Stand up a catalog + artifact host (S3/GCS/Harbor/HTTPS) and generate a signing
   keypair.
2. Publish internal cogs with the cogs repo's catalog generator pointed at the company
   base (`STORE_ARTIFACT_BASE=...`, `STORE_SIGNING_KEY=...`).
3. Provision the fleet with a config profile that **adds** `acme-private` (URL +
   auth token + trusted public key) and optionally **disables** the public store
   (managed-mode lockdown). No end-user TOFU prompt — the admin already trusts it.
4. Employees' dashboards show only ACME-approved cogs.
5. **Air-gapped variant**: `artifact_base = "file:///mnt/cog-mirror"`, catalog served
   from a LAN box; sync via USB/NAS.

### Alternative public-store operator (community)
1. Host artifacts on your own bucket/CDN + an `app-registry.json`; publish your
   signing **public key + fingerprint**.
2. Users **Add store** by URL in their dashboard, see the fingerprint, confirm
   (TOFU), and browse/install. Cogs appear namespaced by store. (The F-Droid model.)

### Hobbyist / single-cog self-hoster (the GPL-`doom` case)
1. Host one cog as a tiny static store — a signed `app-registry.json` + files on
   GitHub Pages or any HTTPS host.
2. Share the URL; anyone adds it and installs that one cog — without it ever
   entering the official MIT repo.

### End user
"Add store → paste URL → confirm fingerprint → install." Can pin/prefer a store or
remove it. On a managed device, "Add store" may be disabled by policy.

---

## 7. Security considerations

- **Hashes are not authenticity across stores** → require catalog signing; the
  Seed installs only signatures it trusts for that store (§4.4).
- **TOFU + visible fingerprint** for adding public stores; **admin-provisioned**
  trust (no prompt) for private/managed.
- **Store ≠ sandbox bypass.** Runtime containment (loopback bind, bearer tokens,
  `[mesh] permissions`, capability grants, resource budgets) still gates every
  installed cog regardless of origin. A third-party cog requesting `mesh.*` or
  binding a port is surfaced/gated exactly as an official one is.
- **Managed-mode lockdown**: device policy can forbid adding stores or restrict to
  an allowlist (MDM-style) for enterprise fleets.
- **Revocation**: signing-key revocation + catalog `yanked`/`revoked` version flags;
  the Seed must honor both.
- **Private-store credential hygiene**: store auth tokens are Seed-managed secrets,
  never committed and never in `cog.toml`.

---

## 8. Phased rollout

| Phase | Scope | User-visible change |
|---|---|---|
| **0 — Spec** | Write the store/catalog spec, store-descriptor schema, signing format (this plan + ADR-0001). | None |
| **1 — Single store, generalized** | Seed reads the official store from config (not a constant); add fetcher abstraction + scheme-agnostic `path`; keep `gcs_path` alias; sign the official catalog. | None (base is now config-driven) |
| **2 — Multi-store, additive** | N stores, namespacing, "Add store" + TOFU, trust store, per-store auth. | Public alternative stores become possible |
| **3 — Enterprise / managed** | Policy engine (lockdown, allowlist, pinning), mTLS/cloud-IAM auth, fleet provisioning profiles, air-gap `file://`, revocation. | Private stores + managed fleets |

Phase 1 is a pure refactor with zero behavior change — it's the safe foundation and
should land first.

---

## 9. Open questions

1. **Catalog shape**: one merged catalog vs per-store catalogs; confirm namespacing
   as the collision strategy.
2. **Key management**: where the official key lives, rotation cadence, how a fork
   generates its own; whether to support multiple valid keys per store during
   rotation.
3. **Spec ownership**: the store/catalog spec now lives in **gearbox**; confirm the
   catalog generator's home (gearbox vs the `cogs` repo) during review.
4. **OCI backend**: is `oci://` worth Phase-2 priority for enterprises already on
   Harbor/Artifactory/ECR? (Likely yes — strong enterprise pull.)
5. **Versioning/compat**: `cog.toml schema_version` + per-store Seed-min-version
   gating.
6. **Out of scope (flag only)**: paid/commercial third-party stores, licensing, and
   monetization.

---

## 10. Recommendation

Adopt the pluggable, signed, multi-store model in §4, sequenced by §8. Land
**Phase 1 (config-driven single store)** first as a no-behavior-change refactor;
it de-risks everything after it. The cog manifest change is minimal and
back-compatible (`path` alongside `gcs_path`); the heavy lifting — multi-catalog
resolution, trust store, auth, policy, and UX — is **Seed-side** and is where the
real design review should focus. The decision and trade-offs are recorded in
[ADR-0001](../adr/ADR-0001-pluggable-cog-stores.md).
