#!/usr/bin/env bash
# Builds dist/LanPet-x86_64.AppImage: one file friends download, chmod +x, and run.
#   tools/build-appimage.sh
# Needs appimagetool on PATH or at tools/appimagetool-x86_64.AppImage
# (https://github.com/AppImage/appimagetool/releases/tag/continuous).
set -euo pipefail
cd "$(dirname "$0")/.."

tool=$(command -v appimagetool || echo tools/appimagetool-x86_64.AppImage)
if [ ! -x "$tool" ]; then
    echo "appimagetool not found. Put appimagetool-x86_64.AppImage from" >&2
    echo "https://github.com/AppImage/appimagetool/releases/tag/continuous in tools/ and chmod +x it." >&2
    exit 1
fi

cargo build --release

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
app="$work/LanPet.AppDir"
install -Dm755 target/release/lanpet "$app/usr/bin/lanpet"
install -Dm644 assets/icon.png "$app/lanpet.png"
ln -s lanpet.png "$app/.DirIcon"
ln -s usr/bin/lanpet "$app/AppRun"
cat > "$app/lanpet.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=LanPet
Comment=A tiny pixel pet that lives on your desktop and hangs out with coworkers' pets over the LAN
Exec=lanpet
Icon=lanpet
Categories=Game;
Terminal=false
EOF

mkdir -p dist
ARCH=x86_64 "$tool" "$app" dist/LanPet-x86_64.AppImage
echo "Built dist/LanPet-x86_64.AppImage (needs glibc 2.34+, e.g. Ubuntu 22.04 or newer)"
