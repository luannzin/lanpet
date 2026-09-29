# LanPet

A tiny pixel pet that lives on your desktop. It studies, lifts, runs, sleeps and explores while you
work (and keeps going while the app is closed), then hangs out with your coworkers' pets over the LAN:
pets in the same room see each other, chat, wave, trade gifts and battle.

## Install

Grab the file for your OS from [Releases](https://github.com/luannzin/lanpet/releases).

| OS | File | Run |
|----|------|-----|
| Linux (Ubuntu 22.04+) | `LanPet-x86_64.AppImage` | `chmod +x LanPet-x86_64.AppImage && ./LanPet-x86_64.AppImage` |
| macOS 11+ | `LanPet-macos.zip` | Unzip and open LanPet.app. First time only: macOS blocks it, go to **System Settings → Privacy & Security → Open Anyway** |
| Windows 10+ | `lanpet-windows-x86_64.zip` | Unzip, run `lanpet.exe`. First time only: **More info → Run anyway** |

Those one-time warnings appear because the app isn't signed with a paid Apple/Microsoft certificate.

macOS without the warning (downloads from the terminal skip Gatekeeper's quarantine):

```bash
curl -L https://github.com/luannzin/lanpet/releases/latest/download/LanPet-macos.zip -o /tmp/LanPet.zip && ditto -x -k /tmp/LanPet.zip /Applications && open /Applications/LanPet.app
```

LAN play: be on the same Wi-Fi/network and click **Allow** on the first-run network prompt
(Windows firewall, macOS "find devices on local network"). Nothing else to configure.

## Playing

- Shrink the window (🗕) and the pet sits in a small widget above your taskbar. Drag it anywhere,
  double-click to open, right-click to quit.
- Rooms are where things happen: Library (study), Gym (weights, treadmill), Bedroom (sleep),
  Kitchen (feed), Portal (expeditions), Arena (battles), Shop.
- Click a coworker's pet to battle, wave or send a gift. Room chat reaches pets in the same room.

The save lives in `~/.local/share/lanpet/` (Linux), `~/Library/Application Support/lanpet/` (macOS)
or `%APPDATA%\lanpet\` (Windows).

## Development

```bash
cargo run                                  # run
LANPET_SAVE=/tmp/pet2.json cargo run       # a second pet on the same machine, to test LAN
cargo test
python3 tools/gen_assets.py                # regenerate the pixel art in assets/ (needs Pillow)
tools/build-appimage.sh                    # Linux AppImage (needs appimagetool)
tools/build-macos.sh                       # macOS .app (on a Mac)
```

Releases: push a tag (`git tag v0.1.0 && git push origin v0.1.0`) and GitHub Actions builds all three
platforms and publishes the release.
