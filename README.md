# LanPet

A tiny pixel pet that lives on your desktop. It studies, lifts, runs, sleeps and explores while you
work (and keeps going while the app is closed), then hangs out with your coworkers' pets over the LAN:
pets in the same room see each other, chat, wave, trade gifts and battle.

## Install

Grab the file for your OS from [Releases](https://github.com/luannzin/lanpet/releases).

| OS | File | Run |
|----|------|-----|
| Linux (Ubuntu 22.04+) | `LanPet-x86_64.AppImage` | `chmod +x LanPet-x86_64.AppImage && ./LanPet-x86_64.AppImage` |
| macOS 11+ | `LanPet-macos.zip` | Unzip, then **right-click LanPet.app → Open** the first time (the app isn't notarized) |
| Windows 10+ | `lanpet-windows-x86_64.zip` | Unzip, run `lanpet.exe` (SmartScreen: *More info → Run anyway*) |

LAN play needs everyone on the same network with UDP port **47474** allowed. Say yes when the
Windows firewall or macOS "local network" prompt appears.

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
