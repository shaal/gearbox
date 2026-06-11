# seed B6 — `require_signed_catalog` transition flag

**Status**: Drafted (ready to apply as a `cognitum-one/seed` PR)
**Target repo**: `cognitum-one/seed`
**Workstream**: Phase 1 / B6 — lands last; the enforcement toggle around B4
**Depends on**: B4 (verification)

## Goal

Avoid a flag-day: ship signature **verification present but not enforced** for one release,
then flip to enforced — so signing can roll out across a fleet without bricking installs if
a catalog is briefly unsigned or a key is mid-rollout.

## Why

B4 can verify, but turning verification into a hard gate on day one is risky: any gap (an
unsigned catalog, a key not yet distributed) would block all installs. B6 is the **policy
around B4**, not B4 itself — it decides what a verification *failure* means, and gives
operators a window to confirm the rollout is healthy before enforcing.

## Behavior

A config boolean `require_signed_catalog`, **default `false`** for the transition release:

| `require_signed_catalog` | signature valid | signature absent / invalid / untrusted |
|---|---|---|
| `false` (transition) | proceed | **warn loudly + metric**, proceed |
| `true` (enforced) | proceed | **reject — fail closed** |

- `false` is strictly advisory: it never blocks on a signature problem, but it **emits a
  structured warning + metric** for every unsigned/invalid catalog — so operators can watch
  the rollout reach "100% signed" before flipping.
- Flip the **default to `true`** in the next release, once A4 reliably signs the official
  catalog. Flipping requires no code change.
- A **managed-fleet policy** may force `true` regardless of local config (managed-mode,
  Phase 3) — noted, not built here.

## Where it sits

B6 wraps the B4 result at the call site (B3 load → **B4 verify → B6 gate** → typed parse →
B5 install). It does not change B4's logic; it interprets B4's `Result`.

## Reference sketch (Rust)

```rust
#[derive(Debug, thiserror::Error)]
pub enum InstallRefused {
    #[error("catalog not verified and signatures are required: {0}")]
    Unverified(#[from] VerifyError),
}

/// Apply the transition policy to a B4 verification result.
pub fn gate_catalog(
    result: Result<String, VerifyError>, require_signed: bool,
) -> Result<(), InstallRefused> {
    match (result, require_signed) {
        (Ok(key_id), _) => { tracing::info!(%key_id, "catalog signature verified"); Ok(()) }
        (Err(e), false) => {                       // transition: advisory only
            tracing::warn!(error = %e,
                "catalog signature problem; proceeding (require_signed_catalog=false)");
            metrics::counter!("catalog.unverified", 1);
            Ok(())
        }
        (Err(e), true) => Err(InstallRefused::Unverified(e)),   // enforced: fail closed
    }
}
```

## Acceptance criteria

- **Release N** (`require_signed_catalog = false`): installs proceed whether the catalog is
  signed or not; an absent/invalid/untrusted signature logs a loud warning and increments a
  metric, but does not block.
- Flipping to `true`: enforcement is active — unsigned/invalid/untrusted catalogs are
  rejected, valid ones proceed — with **no other code change**.
- A valid signature is logged with its `key_id` in both modes.
- (Forward-looking) a managed policy can force `true` over local config.
