<!-- Thanks for contributing to gearbox. Keep the protocol contract intact. -->

## What & why

<!-- What does this change and why? Link any issue or ADR. -->

## Protocol impact

- [ ] No change to signing, canonicalization (JCS), or the catalog/store schema.
- [ ] Changes the protocol — an ADR is added/updated under `docs/adr/` and the test
      vectors in `docs/protocol/testvectors/` are regenerated intentionally.

## Checks

- [ ] `cargo test --manifest-path crates/gearbox/Cargo.toml` passes.
- [ ] `tools/selftest.sh` passes.
- [ ] The Rust and Python implementations still agree (CI `parity` job).
- [ ] Docs updated if behaviour or interfaces changed.

## Security

- [ ] This change has no security implications, **or** they are described above.
