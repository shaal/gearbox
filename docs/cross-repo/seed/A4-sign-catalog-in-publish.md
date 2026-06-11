# A4 — Sign the official catalog in the publish pipeline

**Status**: Drafted (ready to apply as a `cognitum-one/seed` PR)
**Target repo**: `cognitum-one/seed` (publish/release workflow) — see note
**Workstream**: Phase 1 / A4 — **critical path** (producer half of the A4 ↔ B4 handshake)
**Depends on**: #1 signing format (done), #2 catalog generator (done)
**Pins**: [protocol §7](../../protocol/cog-store-protocol.md#7-signing) + [`testvectors/`](../../protocol/testvectors/)

> **Location note.** The workflow that builds the `-arm` binaries and uploads artifacts +
> `app-registry.json` to `gs://cognitum-apps` is **not in the cogs repo** — it lives in
> `cognitum-one/seed`. The catalog is generated + signed where the artifacts are staged, so
> A4 attaches there (plan §5.2 listed it under cogs).

## Goal

At publish time, generate the official `app-registry.json` with the Gearbox generator and
**sign** it with the official Ed25519 key, so a Seed (B4) verifies it. A4 and B4 are the two
halves of the signing handshake; both are pinned to the **same frozen test vector**, so a
signer built to A4 and a verifier built to B4 interoperate by construction.

## Pipeline steps

After the `-arm` binaries + assets are staged under `cogs/<arch>/…` (existing build):

1. **Obtain the generator** — check out Gearbox at a pinned tag/commit; ensure Python ≥ 3.11
   + `cryptography`.
2. **Generate + sign**:
   ```bash
   python3 gearbox/tools/catalog_gen.py \
     --cogs-dir cogs/src/cogs \
     --artifacts-dir "$STAGING" \
     --store-id cognitum-official \
     --generated-at "$(git -C cogs show -s --format=%cI HEAD)" \
     --out app-registry.json \
     --sign-seed-hex "$STORE_SIGNING_KEY" --key-id cognitum-release-2026
   ```
3. **Verify before upload** — verify `app-registry.json` against the embedded official
   public key; **fail the job on mismatch** (catches a key/format error before it ships).
4. **Upload** `app-registry.json` to the location the official store's `catalog_url` points
   at.

## Determinism (a property worth keeping)

Ed25519 is deterministic (RFC 8032) and `--generated-at` is passed in (the cogs commit
time), so the **entire signed catalog is reproducible**: identical inputs → byte-identical
`app-registry.json`. The catalog becomes an auditable, reproducible build artifact.

## Key custody (resolved, [plan §6](../../plans/phase-1-implementation.md#6-decisions-resolved-2026-06-10))

Private 32-byte Ed25519 seed lives in the org secret manager, exposed only as the
`STORE_SIGNING_KEY` CI secret (hex); never logged or committed. The public key is embedded
in the Seed (B4). `key_id` is date-scoped (`cognitum-release-2026`) for additive rotation —
to rotate, publish under a new `key_id` while Seeds trust both during the overlap window.

## CI step sketch (GitHub Actions)

```yaml
      - uses: actions/checkout@v4
        with: { repository: <org>/gearbox, ref: <pinned-tag>, path: gearbox }
      - run: pip install cryptography
      - name: Generate + sign catalog
        env: { STORE_SIGNING_KEY: ${{ secrets.STORE_SIGNING_KEY }} }
        run: |
          python3 gearbox/tools/catalog_gen.py \
            --cogs-dir cogs/src/cogs --artifacts-dir "$STAGING" \
            --store-id cognitum-official \
            --generated-at "$(git -C cogs show -s --format=%cI HEAD)" \
            --out app-registry.json \
            --sign-seed-hex "$STORE_SIGNING_KEY" --key-id cognitum-release-2026
      - name: Verify before upload
        run: python3 gearbox/tools/verify_catalog.py app-registry.json   # see note
      - name: Upload
        run: gsutil cp app-registry.json gs://cognitum-apps/cogs/app-registry.json
```

> `verify_catalog.py` is a small Phase-1 helper to add to Gearbox `tools/` — a thin wrapper
> over `cogstore.signing.verify_catalog` with the official public key. Tiny gearbox-native
> task.

## Failure handling

- `STORE_SIGNING_KEY` absent or not 32 bytes → **fail** (never publish an unsigned official
  catalog).
- Generator/validation error → fail (a broken manifest must not publish).
- Verify-before-upload mismatch → fail.

## Acceptance criteria

- The published catalog carries a valid signature under `cognitum-release-<year>`; a Seed
  (B4) accepts it.
- CI verifies the signature **before** upload; a signing/verify failure fails the publish.
- Re-running publish on identical inputs yields a **byte-identical** signed catalog.
- `STORE_SIGNING_KEY` absent → the publish fails (no unsigned official catalog escapes).
