# Norx coreutils port

This directory is the Norx-specific freestanding layer of the maintained GNU
coreutils fork. It is a no-std Rust multicall ELF: the kernel stages one image
and exposes it as the command paths listed below.

Supported commands:

`cat`, `echo`, `env`, `ls`, `pwd`, `mkdir`, `rmdir`, `cp`, `mv`, `rm`, `touch`,
`ln`, `stat`, `find`, `grep`, `head`, `tail`, `sort`, `wc`, `true`, `false`,
`sleep`.

The command layer owns argument parsing, bounded I/O, byte-preserving output,
and exit-status conventions. It does not own shell state, credentials,
process groups, mounts, or host filesystem access. The kernel interface is the
documented Norx syscall ABI in `userspace/include/norx/syscall.h`; the port
contains a small Rust binding so it can be built independently of another
workspace checkout.

The implementation is intentionally bounded by the current ABI: paths and
individual records are limited to 256 bytes, buffered transformations use a
64 KiB allocator, and directory traversal reports kernel-provided entries.
Failures are written as `coreutils: <command>: <reason>` to stderr and return
1; malformed command lines return 2. `sort` is bytewise and locale-free.

The exact supported syntax, output, status codes, limits, and intentional GNU
differences are recorded in [`COMMANDS.md`](COMMANDS.md). The host tests also
lock the ABI struct sizes and the complete 22-command dispatch set.

Run host tests with:

```text
cargo test --manifest-path norx/Cargo.toml
```

Build both target ELFs and stage the rootfs image with:

```text
python norx/scripts/build.py
```

The current toolchain produces static `no_std` ELFs. A dynamic build is
explicitly deferred until Norx has a stable loader and shared-runtime ABI;
the build must never fall back to host libc or host filesystem access.

The end-to-end command set is exercised through the shell with:

```text
python ../norx-kernel/scripts/coreutils-smoke.py x86_64
python ../norx-kernel/scripts/coreutils-smoke.py aarch64
```
