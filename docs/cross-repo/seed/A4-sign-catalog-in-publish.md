# A4 — Sign the official catalog in the publish pipeline

**Status**: Outline
**Target repo**: `cognitum-one/seed` (publish/release workflow) — see note
**Workstream**: Phase 1 / A4
**Depends on**: #1 signing format (done), #2 catalog generator (done)

> **Location note.** Plan §5.2 listed A4 under `cogs`, but the workflow that builds the
> `-arm` binaries and uploads artifacts + `app-registry.json` to `gs://cognitum-apps` is
> **not in the cogs repo** — it lives in `cognitum-one/seed` (companion to past cog PRs).
> The catalog is generated + signed where the artifacts are staged, so A4 attaches there.

## Goal

At publish time, generate the official `app-registry.json` with the Gearbox generator and
**sign** it with the official Ed25519 key, so a Seed can verify it (B4).

## Approach

In the publish workflow, after the `-arm` binaries + assets are staged under
`cogs/<arch>/…`, run:

```bash
python3 catalog_gen.py \
    --cogs-dir <cogs checkout>/src/cogs \
    --artifacts-dir <staged artifacts> \
    --store-id cognitum-official \
    --generated-at "$(git -C <cogs> show -s --format=%cI HEAD)" \
    --out app-registry.json \
    --sign-seed-hex "$STORE_SIGNING_KEY" --key-id "cognitum-release-2026"
```

Then **verify before upload** (run the bundled verifier against the embedded public key;
fail the job if it doesn't verify), and upload `app-registry.json` next to the artifacts.

## Key custody (resolved, [plan §6](../../plans/phase-1-implementation.md#6-decisions-resolved-2026-06-10))

Private Ed25519 seed lives in the org secret manager, exposed only as the
`STORE_SIGNING_KEY` CI secret (32-byte seed, hex); never logged or committed. The public
key is embedded in the Seed (B4). `key_id` is date-scoped for additive rotation.

## Acceptance

- The published catalog carries a valid signature under `cognitum-release-<year>`.
- CI verifies the signature before upload; a signing/verify failure fails the publish.
- A Seed (B4) accepts the published catalog.
