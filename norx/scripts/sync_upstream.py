#!/usr/bin/env python3
"""Read-only provenance check for the maintained GNU coreutils fork."""

from __future__ import annotations

import argparse
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]


def run(*args: str) -> str:
    return subprocess.check_output(["git", "-C", str(ROOT), *args], text=True).strip()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--branch", help="optional upstream branch; defaults to master")
    args = parser.parse_args()
    branch = args.branch or "master"
    upstream = run("rev-parse", f"upstream/{branch}")
    origin = run("remote", "get-url", "origin")
    if "NorxTeam/coreutils" not in origin:
        raise SystemExit(f"origin is not the maintained Norx fork: {origin}")
    status = run("status", "--porcelain", "--", "UPSTREAM.md", "patches/series", "norx")
    print(f"upstream/{branch}={upstream}")
    print(f"origin={origin}")
    print(f"Norx port changes={'dirty' if status else 'clean'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
