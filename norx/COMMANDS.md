# Norx coreutils command contract

This is the deliberately small command contract shipped by the Norx
multicall ELF. It is a stable Norx interface, not a claim of GNU-compatible
behavior. Every command accepts byte strings without locale conversion. A
successful command returns 0; an operational error returns 1; a malformed
invocation or unsupported option returns 2; an unknown command returns 127.

| Command | Supported syntax and output contract |
| --- | --- |
| `cat` | `cat [FILE...]`; `-` and no files read stdin; `--` ends options; writes bytes unchanged. |
| `echo` | `echo [-n] [ARG...]`; joins arguments with one space and appends one newline unless `-n`. |
| `env` | `env [-i] [--] [NAME=VALUE...]`; prints the resulting environment, one entry per line; executing a command is unsupported and returns 2. |
| `ls` | `ls [-a] [-l] [--] [PATH...]`; names are bytewise sorted; long form is `type links size name`; dot names require `-a`. |
| `pwd` | `pwd`; prints the absolute `PWD` value or `/`; arguments are rejected. |
| `mkdir` | `mkdir [-p] PATH...`; creates mode `0755` directories; `-p` tolerates an existing directory. |
| `rmdir` | `rmdir PATH...`; removes empty directories only. |
| `cp` | `cp SOURCE DEST`; copies regular-file bytes; directories, symlinks, and same-inode copies are rejected. |
| `mv` | `mv SOURCE DEST`; renames within the mounted filesystem. |
| `rm` | `rm [-f] PATH...`; removes regular files or hard links; `-f` ignores missing paths. |
| `touch` | `touch PATH...`; creates files if missing; timestamps are intentionally not implemented. |
| `ln` | `ln SOURCE DEST`; creates a hard link; `ln -s` is rejected because the VFS has no symlink ABI. |
| `stat` | `stat PATH...`; prints path, size, type, mode, link count, and inode. |
| `find` | `find [--] [PATH] [-name GLOB] [-type f\|d]`; recursive depth is bounded at 32; glob supports `*` and `?`. |
| `grep` | `grep [-n] [-v] [--] PATTERN [FILE...]`; literal byte matching, one output line per matching input line; empty patterns are rejected. |
| `head` | `head [-n N\|-nN] [--] [FILE...]`; prints the first N lines, default 10. |
| `tail` | `tail [-n N\|-nN] [--] [FILE...]`; prints the last N lines, default 10. |
| `sort` | `sort [--] [FILE...]`; bytewise sorts each input separately; no locale, key, or numeric modes. |
| `wc` | `wc [-l] [-w] [-c] [--] [FILE...]`; prints selected line/word/byte counts per input; default is all three. |
| `true` | `true`; returns 0 and rejects arguments. |
| `false` | `false`; returns 1 and rejects arguments. |
| `sleep` | `sleep SECONDS`; converts seconds to the Norx scheduler quanta (`NORX_SCHEDULER_HZ = 100`). |

All file reads are retried for bounded `EINTR`/`EAGAIN` bursts and are capped
at 32 KiB for whole-input commands. The filesystem ABI accepts paths up to
256 bytes and returns only regular files and directories. The implementation
does not use host libc, host filesystem calls, dynamic linking, symlinks, or
unbounded allocation.
