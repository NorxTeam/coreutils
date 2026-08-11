# Norx coreutils upstream record

This repository is the maintained Norx fork of GNU coreutils. The upstream
tree remains the source of the portable GNU implementation; Norx-specific
freestanding userspace lives under `norx/` and is kept separate from the
upstream build system.

## Provenance

- Upstream: `https://github.com/coreutils/coreutils.git`
- Maintained fork: `https://github.com/NorxTeam/coreutils.git`
- Git remotes: `upstream` and `origin`
- Baseline branch: `master`
- Baseline commit: `f5e1fd312683e039985c733037f81e6c1378ac2a`
- Upstream release marker: `.prev-version` (`9.11`)
- License: GNU GPL v3 or later; see `COPYING`
- Norx port status: active, non-release

The `master` history is kept as an upstream-compatible baseline. Norx changes
are applied on the `norx/coreutils` branch and are listed in
`patches/series`. A sync must fetch `upstream/master`, preserve the fork
metadata, and report conflicts before changing the Norx branch.

## Norx port

The first Norx port is a small Rust/no-std multicall implementation in
`norx/`. It consumes only the published Norx syscall ABI and does not link to
host libc, host filesystem APIs, or GNU coreutils build outputs. Its binaries
are staged as separate `/bin/<command>` paths while sharing one ELF image.

The initial scope is deliberately explicit:

- commands: `cat`, `echo`, `env`, `ls`, `pwd`, `mkdir`, `rmdir`, `cp`, `mv`,
  `rm`, `touch`, `ln`, `stat`, `find`, `grep`, `head`, `tail`, `sort`, `wc`,
  `true`, `false`, and `sleep`;
- shell state (`cd`, variables, jobs, process groups) remains in `nsh`;
- credentials, privilege transitions, service control, mount/network/device
  operations, and process inspection remain outside this repository;
- UTF-8 is accepted for output and path bytes, but locale-dependent collation
  is not implemented; `sort` uses bytewise UTF-8 ordering;
- command failures write a bounded `coreutils: <command>: <reason>` line to
  stderr and return status 1, while usage errors return status 2;
- SIGINT and short writes are reported through the Norx syscall result; the
  command never depends on a host signal or filesystem implementation.

The initial VFS ABI supports regular files and directories. `ln` implements
hard links; `ln -s` is rejected with a deterministic unsupported error until
the kernel publishes symlink node semantics. `stat` reports the ABI's stable
mode, type, size, and link-count fields rather than host metadata.

## Build and artifact policy

`norx/scripts/build.py` is the single cross-build entry point. It uses the
pinned Rust toolchain and `build-std=core,alloc`, links the staged Norx runtime,
validates the ELF for x86_64 and aarch64, and writes the one multicall image to
`test-rootfs/tests/coreutils/<triple>/coreutils.elf`. The rootfs assembler maps
that image to each command path without copying host binaries.

The current supported artifact is a statically linked `no_std` ELF. Dynamic
linking is a documented follow-up gated on a stable Norx loader and shared
runtime ABI; no build mode may introduce host libc or host filesystem access.
The full command set is exercised through nsh by
`norx-kernel/scripts/coreutils-smoke.py` for both target architectures.

The generated `test-rootfs/var/lib/coreutils/manifest.toml` records the
toolchain, ABI version, source/fork commit, patch-series hash, target hashes,
and clean/dirty source state. No artifact is considered release-ready until
the host tests, malformed-input tests, pipeline tests, and `nsh` rootfs smoke
pass for both targets.

## Sync procedure

```text
git fetch upstream --prune
git diff --exit-code -- UPSTREAM.md patches/series norx
python norx/scripts/sync_upstream.py --check
```

The sync check is read-only. Applying an upstream update is an explicit
maintainer action because it may require revalidating the Norx port and its
artifact manifest.
