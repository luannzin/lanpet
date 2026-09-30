#!/usr/bin/env bash
# Builds dist/LanPet-setup.exe, the Windows installer. Run in Git Bash with Inno Setup 6 installed.
#   tools/build-windows.sh
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
iscc=$(command -v iscc || echo "/c/Program Files (x86)/Inno Setup 6/ISCC.exe")
cargo build --release
# no path conversion, or Git Bash turns /DVersion=... into a file path
MSYS_NO_PATHCONV=1 "$iscc" "/DVersion=$version" tools/lanpet.iss
echo "Built dist/LanPet-setup.exe ($version)"
