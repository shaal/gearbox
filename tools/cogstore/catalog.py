"""Build + validate an app-registry.json from a tree of cog.toml manifests (protocol §3).

Artifact path model (Phase 1): a cog's binary and assets live under `cogs/<arch>/...`
in the store, where `<arch>` is the trailing segment of the binary name
(`cog-<name>-<arch>`). Asset sha256/size come from the manifest (the source of truth that
cogs CI's asset-sha256 gate enforces); the binary is hashed from the staged build.

Two modes:
- normal: the binary is hashed from `--artifacts-dir` (publish).
- `manifests_only=True`: no built binary exists yet (cogs PR-time gate, A3) — the binary
  entry is `{path, pending: true}` and only manifest-derived fields + asset hashes are
  validated.

Asset entries are self-contained for install (B5): they carry `filename` and any
`required_when` from the manifest, not just `{id, path, sha256, size}`.
"""
import datetime
import hashlib
import pathlib
import tomllib

SHA256_HEX_LEN = 64


def arch_of(binary: str) -> str:
    """`cog-<name>-<arch>` -> `<arch>` (e.g. cog-doom-arm -> arm)."""
    parts = binary.split("-")
    if len(parts) < 3:
        raise ValueError(f"binary {binary!r} is not of the form cog-<name>-<arch>")
    return parts[-1]


def _jsonable(v):
    """Normalize tomllib output to JSON-serializable types (TOML dates -> ISO strings)."""
    if isinstance(v, (datetime.datetime, datetime.date, datetime.time)):
        return v.isoformat()
    if isinstance(v, dict):
        return {k: _jsonable(x) for k, x in v.items()}
    if isinstance(v, list):
        return [_jsonable(x) for x in v]
    return v


def _sha256_file(p: pathlib.Path):
    h = hashlib.sha256()
    with p.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 16), b""):
            h.update(chunk)
    return h.hexdigest(), p.stat().st_size


def asset_entry(a: dict, arch: str) -> dict:
    """Catalog artifact entry for a manifest `[[assets]]` block — self-contained for install.

    Carries `filename` (the local destination, B5) and any `required_when` (conditional
    install, B5), so the Seed needn't cross-reference the embedded manifest by id.
    """
    rel = a.get("path") or a["gcs_path"]          # A1 forward-compat: prefer `path`, else `gcs_path`
    entry = {
        "id": a["id"],
        "path": f"cogs/{arch}/{rel}",
        "filename": a["filename"],                # local destination (B5)
        "sha256": a["sha256"],                    # manifest = source of truth (CI-gated)
        "size": int(a["size_bytes"]),
    }
    if "required_when" in a:
        entry["required_when"] = a["required_when"]
    return entry


def build_cog_version(cog_dir: pathlib.Path, artifacts_dir, *, manifests_only: bool = False) -> dict:
    manifest = _jsonable(tomllib.loads((cog_dir / "cog.toml").read_text()))
    cog = manifest["cog"]
    binary = cog["binary"]
    arch = arch_of(binary)
    bin_rel = f"cogs/{arch}/{binary}"

    if manifests_only:
        binary_art = {"path": bin_rel, "pending": True}     # built + hashed later (A4 publish)
    else:
        bin_file = pathlib.Path(artifacts_dir) / bin_rel
        if not bin_file.is_file():
            raise FileNotFoundError(
                f"binary artifact missing: {bin_file} "
                f"(stage the built {binary} under --artifacts-dir, or use --manifests-only)")
        bsha, bsize = _sha256_file(bin_file)
        binary_art = {"path": bin_rel, "sha256": bsha, "size": bsize}

    assets = [asset_entry(a, arch) for a in manifest.get("assets", [])]
    return {
        "version": cog["version"],
        "manifest": manifest,
        "artifacts": {"binary": binary_art, "assets": assets},
    }


def build_catalog(cogs_dir, artifacts_dir, *, store_id: str, generated_at: str,
                  manifests_only: bool = False) -> dict:
    cogs_dir = pathlib.Path(cogs_dir)
    cogs = []
    for cog_dir in sorted(p for p in cogs_dir.iterdir() if (p / "cog.toml").is_file()):
        ver = build_cog_version(cog_dir, artifacts_dir, manifests_only=manifests_only)
        cogs.append({"id": ver["manifest"]["cog"]["id"], "versions": [ver]})

    catalog = {
        "schema_version": 1,
        "store_id": store_id,
        "generated_at": generated_at,
        "cogs": cogs,
    }
    validate(catalog)
    return catalog


def validate(catalog: dict) -> None:
    """Raise ValueError if `catalog` is not a spec-valid app-registry.json."""
    def req(cond, msg):
        if not cond:
            raise ValueError(f"invalid catalog: {msg}")

    req(catalog.get("schema_version") == 1, "schema_version must be 1")
    req(isinstance(catalog.get("store_id"), str) and catalog["store_id"], "store_id missing")
    req(isinstance(catalog.get("generated_at"), str), "generated_at must be a string")
    req(isinstance(catalog.get("cogs"), list), "cogs must be a list")

    seen = set()
    for c in catalog["cogs"]:
        cid = c.get("id")
        req(isinstance(cid, str) and cid, "cog.id missing")
        req(cid not in seen, f"duplicate cog id {cid!r}")
        seen.add(cid)
        req(isinstance(c.get("versions"), list) and c["versions"], f"{cid}.versions empty")
        for v in c["versions"]:
            req(isinstance(v.get("version"), str), f"{cid}.version missing")
            req(isinstance(v.get("manifest"), dict), f"{cid}.manifest missing")
            arts = v.get("artifacts") or {}
            _check_artifact(arts.get("binary"), f"{cid} binary", allow_pending=True)
            req(isinstance(arts.get("assets"), list), f"{cid} assets must be a list")
            for a in arts["assets"]:
                _check_artifact(a, f"{cid} asset {a.get('id')!r}")
                req(isinstance(a.get("filename"), str) and a["filename"],
                    f"{cid} asset {a.get('id')!r}: filename missing")
                rw = a.get("required_when")
                req(rw is None or isinstance(rw, str),
                    f"{cid} asset {a.get('id')!r}: required_when must be a string")


def _check_artifact(a, where, allow_pending=False):
    def req(cond, msg):
        if not cond:
            raise ValueError(f"invalid catalog: {where}: {msg}")

    req(isinstance(a, dict), "missing")
    p = a.get("path")
    req(isinstance(p, str) and p, "path missing")
    req("://" not in p and not p.startswith("/"), f"path must be relative, got {p!r}")
    if allow_pending and a.get("pending") is True:
        return                                              # binary not yet built/hashed
    s = a.get("sha256")
    req(isinstance(s, str) and len(s) == SHA256_HEX_LEN
        and all(ch in "0123456789abcdef" for ch in s),
        f"sha256 must be 64 lowercase hex, got {s!r}")
    req(isinstance(a.get("size"), int) and not isinstance(a.get("size"), bool)
        and a["size"] >= 0, "size must be a non-negative integer")
