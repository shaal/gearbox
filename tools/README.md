# tools — catalog generator (reference implementation)

The **catalog generator** (gearbox#2): walks a tree of `cog.toml` manifests and emits a
spec-valid, optionally signed `app-registry.json` (the cog-store catalog, protocol §3).

This is the **publish-side reference implementation**, written in Python so it runs as a
step in `cognitum-one/cogs` CI and the `cognitum-one/seed` publish workflow with no
toolchain to build. The device-side **verifier** lives in `cognitum-one/seed`; the native
**`gearbox` CLI** is gearbox#3. All implement the same protocol and are pinned by the test
vector in [`../docs/protocol/testvectors/`](../docs/protocol/testvectors/).

## Layout

```
tools/
  catalog_gen.py      # CLI: read cog tree -> build -> (sign) -> validate -> write
  verify_catalog.py   # CLI: verify a signed catalog against a trusted key (A4 pre-upload)
  cogstore/
    jcs.py            # RFC 8785 canonicalization (matches the frozen test vector)
    signing.py        # Ed25519 sign/verify over JCS bytes (protocol §7)
    catalog.py        # build + validate app-registry.json (protocol §3)
  testdata/           # fixtures: two cog.toml manifests + stub binary artifacts
  selftest.sh         # 5 checks (conformance, e2e, manifests-only, asset_entry, verify)
```

## Requirements

- Python ≥ 3.11 (`tomllib` is stdlib)
- [`cryptography`](https://pypi.org/project/cryptography/) (Ed25519)

## Generate a catalog

```bash
# publish: binary hashed from the staged build
python3 catalog_gen.py \
    --cogs-dir ../../cogs/src/cogs --artifacts-dir dist \
    --store-id cognitum-official --generated-at 2026-06-10T00:00:00Z \
    --out app-registry.json \
    --sign-seed-hex "$(cat "$STORE_SIGNING_KEY")" --key-id cognitum-release-2026

# manifests-only: no built binary yet (cogs PR-time gate, A3) -> binary entry is {pending}
python3 catalog_gen.py --cogs-dir ../../cogs/src/cogs --manifests-only \
    --store-id cognitum-official --generated-at 2026-06-10T00:00:00Z --out /tmp/app-registry.json
```

- **Asset** `sha256`/`size`/`filename`/`required_when` come from each `cog.toml`. Asset
  entries are **self-contained for install** (B5): they carry `filename` (the on-device
  destination) and any `required_when`, not just `{id, path, sha256, size}`.
- **Binary** `sha256`/`size` are computed from the staged build (`--artifacts-dir`); under
  `--manifests-only` the binary entry is `{path, pending: true}` and is hashed later by the
  publish step (A4).
- Artifact paths resolve under `cogs/<arch>/…` (arch = trailing segment of
  `cog-<name>-<arch>`). The generator accepts the new `path` field and falls back to
  `gcs_path` (A1 alias).
- Omit `--sign-seed-hex` for an **unsigned** catalog (preview channel). The signing seed is
  never committed — in CI it's the `STORE_SIGNING_KEY` secret.

## Verify a catalog

```bash
python3 verify_catalog.py app-registry.json --key-id cognitum-release-2026 --pubkey-b64 <b64>
# or: --trust-file trust.json   ({"key_id": "<b64>"})
```

The seed publish step (A4) runs this against the official public key **before upload** —
exit 0 + prints the signing `key_id`, non-zero on any failure.

## Self-test

```bash
./selftest.sh
```

1. **Conformance** — `jcs` + `signing` reproduce the frozen
   [`catalog.signed.json`](../docs/protocol/testvectors/catalog.signed.json) (canonical
   bytes + signature) byte-for-byte. A canonicalization/signing drift fails here.
2. **End-to-end** — generate a signed catalog from `testdata/`, validate, verify.
3. **Manifests-only** — generate with no built binary; validates with `binary.pending`.
4. **`asset_entry`** — `filename` + `required_when` flow into asset entries; `path` wins
   over `gcs_path`.
5. **`verify_catalog.py`** — verifies the vector and rejects a tampered copy.

## Scope (Phase 1)

Single version per cog, single arch per binary, single official store. Multi-version /
multi-arch / multi-store are later phases (see the
[Phase 1 plan](../docs/plans/phase-1-implementation.md) and
[ADR-0001](../docs/adr/ADR-0001-pluggable-cog-stores.md)).
