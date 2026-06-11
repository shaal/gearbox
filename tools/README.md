# tools — catalog generator (reference implementation)

The **catalog generator** (gearbox#2): walks a tree of `cog.toml` manifests and emits a
spec-valid, optionally signed `app-registry.json` (the cog-store catalog, protocol §3).

This is the **publish-side reference implementation**, written in Python so it runs as a
step in `cognitum-one/cogs` CI with no toolchain to build. The device-side **verifier**
lives in `cognitum-one/seed`; the native **`gearbox` CLI** is gearbox#3. All three speak
the same protocol and are pinned by the test vector in
[`../docs/protocol/testvectors/`](../docs/protocol/testvectors/).

## Layout

```
tools/
  catalog_gen.py      # CLI: read cog tree -> build -> (sign) -> validate -> write
  cogstore/
    jcs.py            # RFC 8785 canonicalization (matches the frozen test vector)
    signing.py        # Ed25519 sign/verify over JCS bytes (protocol §7)
    catalog.py        # build + validate app-registry.json (protocol §3)
  testdata/           # fixtures: two cog.toml manifests + stub binary artifacts
  selftest.sh         # conformance vs the frozen #1 vector + an end-to-end run
```

## Requirements

- Python ≥ 3.11 (`tomllib` is stdlib)
- [`cryptography`](https://pypi.org/project/cryptography/) (Ed25519)

## Usage

```bash
python3 catalog_gen.py \
    --cogs-dir ../../cogs/src/cogs \
    --artifacts-dir dist \                 # built binaries staged at cogs/<arch>/<binary>
    --store-id cognitum-official \
    --generated-at 2026-06-10T00:00:00Z \  # pass it in — keeps catalogs reproducible
    --out app-registry.json \
    --sign-seed-hex "$(cat "$STORE_SIGNING_KEY")" --key-id cognitum-release-2026
```

- **Asset** `sha256`/`size` come from each `cog.toml` (the source of truth that cogs CI's
  `asset-sha256` gate already enforces). **Binary** `sha256`/`size` are computed from the
  staged build under `--artifacts-dir`.
- Artifact paths resolve under `cogs/<arch>/…`, where `<arch>` is the trailing segment of
  the binary name (`cog-<name>-<arch>`). The generator accepts the new `path` field and
  falls back to `gcs_path` (the A1 forward-compat alias).
- Omit `--sign-seed-hex` to emit an **unsigned** catalog (e.g. a preview channel). The
  signing seed is never committed — in CI it's the `STORE_SIGNING_KEY` secret.

## Self-test

```bash
./selftest.sh
```

1. **Conformance** — feeds the frozen [`catalog.signed.json`](../docs/protocol/testvectors/catalog.signed.json)
   through `jcs` + `signing` and asserts the canonical bytes and signature match the
   committed vector exactly. If the canonicalization ever drifts, this fails.
2. **End-to-end** — generates a signed catalog from `testdata/`, validates it against the
   protocol, and verifies its signature.

## Scope (Phase 1)

Single version per cog, single arch per binary, single official store. Multi-version /
multi-arch / multi-store are later phases (see the
[Phase 1 plan](../docs/plans/phase-1-implementation.md) and
[ADR-0001](../docs/adr/ADR-0001-pluggable-cog-stores.md)).
