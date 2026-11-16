# SPDX-License-Identifier: Apache-2.0
"""Freeze opaque native metadata inputs; Rust constructs every model descriptor.

Run through the verified same-user shell. The request is private evidence and
must stay in the authorized workspace. This helper never constructs Profiles,
calibrations, compiler policies, resource policies or download URLs.
"""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import subprocess


DEFAULT_SOURCE_FILES = (
    "Cargo.lock",
    "rust-toolchain.toml",
    "crates/airk-core/src/arbiter/mod.rs",
    "crates/airk-core/src/arbiter/context_action.rs",
    "crates/airk-core/src/arbiter/context_action/calibration.rs",
    "crates/airk-core/src/arbiter/context_action/calibration/terminal.rs",
    "crates/airk-core/src/arbiter/focus.rs",
    "crates/airk-core/src/native_focus.rs",
    "crates/airk-core/src/arbiter/resource_policy.rs",
    "crates/airk-retrieval/src/arbiter/context_action.rs",
    "crates/airk-retrieval/src/arbiter/compiler.rs",
    "crates/airk-retrieval/src/arbiter/compiler/qwen.rs",
    "crates/airk-retrieval/src/arbiter/compiler/focus.rs",
    "crates/airk-retrieval/src/arbiter/runtime/qualification.rs",
    "crates/airk-retrieval/src/arbiter/runtime/qualification/regeneration.rs",
)


def file_pin(path: Path) -> dict[str, str]:
    path = path.absolute()
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"A regular original metadata file is required: {path}")
    before = path.stat()
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1 << 20), b""):
            digest.update(block)
    after = path.stat()
    if (before.st_ino, before.st_size, before.st_mtime_ns) != (
        after.st_ino, after.st_size, after.st_mtime_ns
    ):
        raise ValueError(f"Metadata changed while freezing: {path}")
    return {"path": str(path), "sha256": digest.hexdigest()}


def workspace_output(path: Path, repository: Path) -> Path:
    resolved = path.resolve()
    if not resolved.is_relative_to(repository) or resolved == repository:
        raise ValueError("Private output must remain inside the authorized repository")
    if resolved.exists():
        raise ValueError(f"A fresh private output path is required: {resolved}")
    return resolved


def build_request(*, repository: Path, catalog: Path, acquisition_receipt: Path,
                  asset_census: Path, native_build_receipt: Path, asset_root: Path, output: Path,
                  request: Path, source_files: list[Path], source_commit: str) -> dict:
    repository = repository.resolve(strict=True)
    output = workspace_output(output, repository)
    request = workspace_output(request, repository)
    if output == request or request.is_relative_to(output):
        raise ValueError("The frozen request must precede and stay outside the fresh native output")
    if not request.parent.is_dir():
        raise ValueError("Create and verify the request parent under the same user first")
    if asset_root.is_symlink() or not asset_root.is_dir():
        raise ValueError("The original acquired asset root must be an existing ordinary directory")
    sources = [file_pin(path) for path in source_files]
    if not sources or len({row["path"] for row in sources}) != len(sources):
        raise ValueError("Distinct frozen source files are required")
    value = {
        "schema": "airk.private.context-metadata-regeneration/1",
        "repository_root": str(repository),
        "native_build_receipt": file_pin(native_build_receipt),
        "source_catalog": file_pin(catalog),
        "acquisition_receipt": file_pin(acquisition_receipt),
        "asset_census": file_pin(asset_census),
        "asset_root": str(asset_root.resolve(strict=True)),
        "output": str(output),
        "source_commit": source_commit,
        "source_files": sources,
    }
    with request.open("x", encoding="utf-8", newline="\n") as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    return {"request": file_pin(request), "output": str(output),
            "native_selector": "arbiter::runtime::qualification::regeneration::emit_current_policy_profiles_from_verified_original_assets",
            "NN_invocations": 0, "metadata_regenerated": False,
            "public_source_available": False}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository-root", type=Path,
                        default=Path(__file__).resolve().parents[2])
    parser.add_argument("--catalog", type=Path, required=True)
    parser.add_argument("--acquisition-receipt", type=Path, required=True)
    parser.add_argument("--asset-census", type=Path, required=True)
    parser.add_argument("--native-build-receipt", type=Path, required=True)
    parser.add_argument("--asset-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--request", type=Path, required=True)
    parser.add_argument("--source-file", type=Path, action="append")
    args = parser.parse_args()
    repository = args.repository_root.resolve(strict=True)
    source_files = args.source_file or [repository / path for path in DEFAULT_SOURCE_FILES]
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repository,
                                     text=True).strip()
    result = build_request(repository=repository, catalog=args.catalog,
        acquisition_receipt=args.acquisition_receipt, asset_census=args.asset_census, native_build_receipt=args.native_build_receipt,
        asset_root=args.asset_root, output=args.output, request=args.request,
        source_files=source_files, source_commit=commit)
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
