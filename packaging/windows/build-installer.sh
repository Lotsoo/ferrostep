#!/usr/bin/env bash
# Builds the Windows installer from Fedora:
#   1. cross-compiles ferrostep.exe (MinGW, icon embedded by build.rs)
#   2. wraps it in dist/Ferrostep-Setup-<version>.exe with NSIS.
# Needs: rustup target x86_64-pc-windows-gnu, mingw64-gcc, mingw32-nsis.
set -euo pipefail
cd "$(dirname "$0")/../.."
version=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
cargo build --release --target x86_64-pc-windows-gnu
mkdir -p dist
makensis -V2 -DVERSION="$version" packaging/windows/ferrostep.nsi
echo "Installer siap: dist/Ferrostep-Setup-$version.exe"
