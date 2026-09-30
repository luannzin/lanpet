#!/usr/bin/env bash
# Builds dist/LanPet.app (universal: Apple Silicon + Intel) and dist/LanPet.dmg, the disk image you
# drag LanPet into Applications from. Run on a Mac.
#   tools/build-macos.sh
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

app=dist/LanPet.app
rm -rf "$app" dist/LanPet.dmg
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create -output "$app/Contents/MacOS/lanpet" \
    target/aarch64-apple-darwin/release/lanpet target/x86_64-apple-darwin/release/lanpet

iconset=$(mktemp -d)/lanpet.iconset
mkdir -p "$iconset"
for s in 16 32 128 256 512; do
    sips -z $s $s assets/icon.png --out "$iconset/icon_${s}x${s}.png" >/dev/null
    sips -z $((s * 2)) $((s * 2)) assets/icon.png --out "$iconset/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/lanpet.icns"

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>LanPet</string>
    <key>CFBundleDisplayName</key><string>LanPet</string>
    <key>CFBundleIdentifier</key><string>io.github.luannzin.lanpet</string>
    <key>CFBundleExecutable</key><string>lanpet</string>
    <key>CFBundleIconFile</key><string>lanpet</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSLocalNetworkUsageDescription</key><string>LanPet finds your coworkers' pets on the local network so they can hang out, chat and battle.</string>
</dict>
</plist>
EOF

# Ad-hoc signature: required for Apple Silicon. Not notarized, so a browser-downloaded copy needs
# System Settings > Privacy & Security > Open Anyway once (or: xattr -dr com.apple.quarantine LanPet.app).
codesign --force --deep --sign - "$app"

stage=$(mktemp -d)
ditto "$app" "$stage/LanPet.app"
ln -s /Applications "$stage/Applications"
# hdiutil now and then fails with "Resource busy" on CI runners; a retry gets through
for try in 1 2 3; do
    hdiutil create -volname LanPet -srcfolder "$stage" -format UDZO -ov dist/LanPet.dmg && break
    [ "$try" = 3 ] && exit 1
    sleep 5
done
echo "Built $app and dist/LanPet.dmg"
