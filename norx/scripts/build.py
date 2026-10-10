#!/usr/bin/env python3
"""Build and stage the Norx coreutils multicall ELF for the native ARM64 target."""

from __future__ import annotations

import hashlib
import argparse
import os
from pathlib import Path
import subprocess
import sys

SCRIPT_ROOT = Path(__file__).resolve().parent
COREUTILS_ROOT = SCRIPT_ROOT.parents[1]
REPO_ROOT = COREUTILS_ROOT.parent
TOOLCHAIN_SCRIPTS = REPO_ROOT / "toolchain" / "scripts"
sys.path.insert(0, str(TOOLCHAIN_SCRIPTS))

from build import (  # noqa: E402
    cargo_command,
    checked_output,
    copy_checked,
    load_toml,
    run,
    sha256,
    target_emulation,
    tool_path,
    validate_elf,
)


def git_output(*arguments: str) -> str:
    return checked_output(["git", "-C", str(COREUTILS_ROOT), *arguments]).strip()


def upstream_commit(branch: str | None) -> tuple[str, str]:
    name = branch or "master"
    ref = f"upstream/{name}"
    return name, git_output("rev-parse", "--verify", ref)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--branch",
        help="optional upstream branch to record; defaults to the upstream default branch",
    )
    args = parser.parse_args()
    source_branch, source_commit = upstream_commit(args.branch)
    versions = load_toml(REPO_ROOT / "toolchain" / "versions.toml")
    abi = load_toml(REPO_ROOT / "toolchain" / "abi.toml")
    llvm_version = versions["llvm_version"]
    cargo = cargo_command(versions["rust_toolchain"])
    rustc_version = checked_output([*cargo[:-1], "rustc", "-Vv"])
    if versions["rustc_version"] not in rustc_version or versions["rustc_commit"] not in rustc_version:
        raise SystemExit(f"rustc does not match the pinned userspace toolchain:\n{rustc_version}")

    ld_lld = tool_path("ld.lld.exe" if os.name == "nt" else "ld.lld", "NORX_LD_LLD")
    readobj = tool_path(
        "llvm-readobj.exe" if os.name == "nt" else "llvm-readobj", "NORX_LLVM_READOBJ"
    )
    llvm_strip = tool_path(
        "llvm-strip.exe" if os.name == "nt" else "llvm-strip", "NORX_LLVM_STRIP"
    )
    if llvm_version not in checked_output([str(ld_lld), "--version"]):
        raise SystemExit(f"{ld_lld} is not pinned to LLVM {llvm_version}")

    manifest = COREUTILS_ROOT / "norx" / "Cargo.toml"
    build_root = COREUTILS_ROOT / "build"
    rootfs = Path(os.environ.get("NORX_ROOTFS", REPO_ROOT / "test-rootfs"))
    results: dict[str, str] = {}
    for target_name, target_info in (("aarch64", abi["targets"]["aarch64"]),):
        target_dir = build_root / target_name
        target_dir.mkdir(parents=True, exist_ok=True)
        linker_script = REPO_ROOT / "toolchain" / target_info["linker_script"]
        target_spec = REPO_ROOT / "toolchain" / "targets" / f"{target_name}-unknown-norx.json"
        triple = target_info["triple"]
        runtime_archive = REPO_ROOT / "toolchain" / "build" / "runtime" / target_name / "libnorxrt.a"
        if not runtime_archive.is_file():
            raise SystemExit(f"missing target runtime archive: {runtime_archive}; run build-runtime.py first")
        linker_revision = sha256(linker_script)[:16]
        config = [
            "--config",
            f'target."{triple}".linker = "{ld_lld.as_posix()}"',
            "--config",
            f'target."{triple}".rustflags = ["-C", "debuginfo=2", "-L", "native={runtime_archive.parent.as_posix()}", "-l", "static=norxrt", "-C", "link-arg=-T{linker_script.as_posix()}", "-C", "link-arg=-m{target_emulation(target_name)}", "-C", "link-arg=--defsym=__linker_revision=0x{linker_revision}"]',
        ]
        command = [
            *cargo,
            *config,
            "build",
            "-Z",
            "build-std=core,alloc",
            "-Z",
            "json-target-spec",
            "--offline",
            "--locked",
            "--manifest-path",
            str(manifest),
            "--target",
            str(target_spec),
            "--target-dir",
            str(target_dir / "cargo-target"),
            "--release",
            "--bin",
            "coreutils",
        ]
        run(command)
        candidates = [
            path
            for path in (target_dir / "cargo-target").rglob("coreutils*")
            if path.is_file() and path.parent.name == "release" and path.name in {"coreutils", "coreutils.exe"}
        ]
        if len(candidates) != 1:
            raise SystemExit(f"expected one coreutils ELF for {target_name}, found {candidates}")
        artifact = target_dir / "coreutils.elf"
        copy_checked(candidates[0], artifact)
        run([str(llvm_strip), "--strip-debug", str(artifact)])
        report = validate_elf(readobj, artifact)
        (target_dir / "readobj.txt").write_text(report, encoding="utf-8")
        destination = rootfs / "tests" / "coreutils" / triple / artifact.name
        copy_checked(artifact, destination)
        results[target_name] = sha256(artifact)

    manifest_dir = rootfs / "var" / "lib" / "coreutils"
    manifest_dir.mkdir(parents=True, exist_ok=True)
    lines = [
        'schema = "coreutils-build"',
        "version = 1",
        f'abi_version = {abi["version"]}',
        f'upstream_branch = "{source_branch}"',
        f'upstream_commit = "{source_commit}"',
        f'fork_commit = "{git_output("rev-parse", "HEAD")}"',
        f'upstream_document_sha256 = "{sha256(COREUTILS_ROOT / "UPSTREAM.md")}"',
        f'patch_series_sha256 = "{sha256(COREUTILS_ROOT / "patches" / "series")}"',
        f'dirty = {str(bool(git_output("status", "--porcelain"))).lower()}',
        f'rustc_version = "{versions["rustc_version"]}"',
        f'rustc_commit = "{versions["rustc_commit"]}"',
        'panic_policy = "abort"',
        'allocator = "64k_single_address_space_bump"',
        "",
    ]
    for target_name, digest in results.items():
        lines.extend(
            [
                f"[targets.{target_name}]",
                f'coreutils_sha256 = "{digest}"',
                "",
            ]
        )
    manifest_path = manifest_dir / "manifest.toml"
    manifest_path.write_text("\n".join(lines), encoding="utf-8")
    print(f"coreutils build passed; manifest: {manifest_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
