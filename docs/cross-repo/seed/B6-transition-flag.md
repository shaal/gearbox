# seed B6 — `require_signed_catalog` transition flag

**Status**: Outline
**Target repo**: `cognitum-one/seed`
**Workstream**: Phase 1 / B6 (lands last)
**Depends on**: B4 (verification)

## Goal

Avoid a flag-day: ship signature **verification present but not enforced** for one
release, then flip to enforced — so signing rolls out without bricking installs if a
catalog is briefly unsigned.

## Changes

- A config flag `require_signed_catalog`, **default `false`** for one release.
  - `false`: if a signature is present, verify it and **warn** on absent/invalid; do not
    block installs.
  - `true`: missing / invalid / untrusted signature → **reject**.
- Flip the default to `true` once the official catalog is reliably signed (after A4).

## Acceptance

- Release N: flag off — installs proceed with or without a signature; an invalid signature
  logs a loud warning.
- Flipping to on: enforcement is active with no other code change.
