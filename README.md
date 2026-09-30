# LanPet

A tiny pixel pet that lives on your desktop. It studies, lifts, runs, sleeps and explores while you
work (and keeps going while the app is closed), then hangs out with your coworkers' pets over the LAN:
pets in the same room see each other, chat, wave, trade gifts and battle.

## Install

Grab the file for your OS from [Releases](https://github.com/luannzin/lanpet/releases).

| OS | File | Install |
|----|------|---------|
| Ubuntu 22.04+ / Debian 12+ | `lanpet_amd64.deb` | Double-click it, or `sudo apt install ./lanpet_amd64.deb`. Then open LanPet from your apps |
| macOS 11+ | `LanPet.dmg` | Open it and drag LanPet into Applications. First launch only: macOS blocks it, go to **System Settings → Privacy & Security → Open Anyway** |
| Windows 10+ | `LanPet-setup.exe` | Run it (no admin needed). First time only: **More info → Run anyway** |

Those one-time warnings appear because the app isn't signed with a paid Apple/Microsoft certificate.

Linux from the terminal:

```bash
curl -L https://github.com/luannzin/lanpet/releases/latest/download/lanpet_amd64.deb -o /tmp/lanpet.deb && sudo apt install /tmp/lanpet.deb
```

macOS without the warning (downloads from the terminal skip Gatekeeper's quarantine):

```bash
curl -L https://github.com/luannzin/lanpet/releases/latest/download/LanPet.dmg -o /tmp/LanPet.dmg && hdiutil attach -nobrowse -mountpoint /tmp/LanPet /tmp/LanPet.dmg && ditto /tmp/LanPet/LanPet.app /Applications/LanPet.app && hdiutil detach /tmp/LanPet && open /Applications/LanPet.app
```

Updates: LanPet checks for a new release on start and every few hours, downloads it in the
background, and your pet tells you. Go to **Home → Update LanPet**; it installs and restarts
(Linux asks for your password).

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
tools/build-deb.sh                         # Linux .deb
tools/build-macos.sh                       # macOS .app + .dmg (on a Mac)
tools/build-windows.sh                     # Windows installer (Git Bash + Inno Setup 6)
```

Releases: bump `version` in `Cargo.toml` and push to `main`. GitHub Actions sees there's no release
for that version yet, builds the three installers and publishes release `v<version>`; installed
copies pick it up as an update. No manual tagging.
