# LanPet

A tiny pixel pet that lives on your desktop. It studies, lifts, runs, sleeps and explores while you
work (and keeps going while the app is closed), then hangs out with your coworkers' pets over the LAN:
pets walk around the same little island town, see each other in the same place, chat, wave, trade
gifts and battle.

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

- Click the tray icon to open LanPet. Close it and the pet hides in the tray, or (paw button) walks
  along the bottom of your screen.
- The world is a top-down town. Click anywhere and your pet walks there; click a building to walk in,
  and the doormat to walk back out. Your coworkers' pets walk around it too.
- Places are where things happen: Home (sleep, food, water), Library (study), Gym (weights,
  treadmill), Portal (expeditions), Arena (battles), Shop. Click furniture to use it, or use the
  buttons. In town, the buttons walk your pet to any building. Hold the mouse down to steer, or
  click the minimap to head somewhere further off.
- Homes are in the apartment block: everyone online gets a front door, four to a floor. Take the
  elevator to a floor and walk in at a coworker's door to visit their home.
- Decorate your home: at home, press **Decorate**, click a piece of furniture to pick it up and the
  floor to set it down, or take more out of storage. The Shop sells furniture; visitors see your
  home as you left it.
- Click a coworker's pet to battle, wave or send a gift. Chat reaches the pets in the same place.
- The town keeps your clock: golden at sunset, dark at night with the lamps, windows and glowing
  furniture lit.
- The expanded view can be made bigger by dragging its bottom-right corner.

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
