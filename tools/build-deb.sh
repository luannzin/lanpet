#!/usr/bin/env bash
# Builds dist/lanpet_amd64.deb for Ubuntu 22.04+ / Debian 12+ (glibc 2.34+):
#   tools/build-deb.sh
# Install with: sudo apt install ./dist/lanpet_amd64.deb
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cargo build --release

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
pkg="$work/lanpet"
install -Dm755 target/release/lanpet "$pkg/usr/bin/lanpet"
install -Dm644 assets/icon.png "$pkg/usr/share/icons/hicolor/512x512/apps/lanpet.png"
mkdir -p "$pkg/usr/share/applications" "$pkg/DEBIAN"
cat > "$pkg/usr/share/applications/lanpet.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=LanPet
Comment=A tiny pixel pet that lives on your desktop and hangs out with coworkers' pets over the LAN
Exec=lanpet
Icon=lanpet
StartupWMClass=lanpet
Categories=Game;
Terminal=false
EOF
# Depends: the libraries the binary loads at runtime (it links only libc; X11/GL are dlopen'd).
cat > "$pkg/DEBIAN/control" <<EOF
Package: lanpet
Version: $version
Architecture: amd64
Maintainer: luannzin <luandaniel966@gmail.com>
Depends: libc6 (>= 2.34), libgcc-s1, libegl1, libgl1, libx11-6, libx11-xcb1, libxcb1, libxcursor1, libxi6, libxkbcommon0, libxkbcommon-x11-0, libxrender1
Section: games
Priority: optional
Homepage: https://github.com/luannzin/lanpet
Description: tiny pixel pet for your desktop, with coworkers' pets over the LAN
 LanPet studies, lifts, runs, sleeps and explores while you work, then hangs
 out with your coworkers' pets on the same network: they chat, wave, trade
 gifts and battle.
EOF

mkdir -p dist
dpkg-deb --root-owner-group -Zxz --build "$pkg" dist/lanpet_amd64.deb
echo "Built dist/lanpet_amd64.deb ($version)"
