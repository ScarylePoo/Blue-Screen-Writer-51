#!/bin/bash
# Run Windows cargo (x86_64-pc-windows-gnu, self-contained linker) from WSL,
# in the repository (or worktree) this script lives in.
cd "$(dirname "$(readlink -f "$0")")/.." || exit 1
exec "/mnt/c/Users/Forboding Angel/.cargo/bin/cargo.exe" "$@" 2>&1
