---
version: 1
slug: "src-main-rs"
primary_target: "src/main.rs"
related_targets: ["src/art.rs"]
---

# LanPet window: tray, popover, expanded

Scope: the whole app window (tray icon, popover, expanded view, first-run hatch). Mode: Experience; the room leads and the interface recedes.

Audience and job: office coworkers glance at the pet between tasks, care for it in short bursts, and meet coworkers' pets when they share a room.

Constraints: egui 0.36 custom painting; Linux runs on X11; tray via tray-icon (ksni backend on Linux, which may not report click position). Game logic must keep running while the window is hidden (App::logic).

Craft bar: Stardew Valley's menus; Rusty's Retirement (a desk-side idle game).

User pins: direction C, the category standard played straight. The expanded view's room cards must be fixed (legible, not a cramped tile strip). Text must use a more legible face than the pixel fonts shown in the proposal.

## Direction contract

THESIS: One room at a time inside a cozy pixel-game frame. Refuses the 700px dashboard: eight room tiles, stat grid, scrolling card list and always-open chat.

OWN-WORLD: Wood frame #7a4a26 with #3b2112 outline and #b0733c highlight; parchment panels #f3d9a4 shaded #d9b574; ink #3b2112; green primary button #5dbb4f over #2f7a2b; coin gold #f6c343. Square corners, stepped 3px outlines, bottom-inset button shade. Nunito (heavy for titles, bold for text). Icons drawn as pixel shapes, never glyphs.

STORY: Glance to see which room the pet is in and what it is doing. Act with ◀ ▶ to change room, by clicking furniture, or with at most three buttons. Coworkers' pets appear in the room; click one for battle, wave or gift.

FIRST VIEWPORT: Popover 440px wide anchored at the tray. Ribbon [◀][ROOM NAME + status][▶]; coin, expand and minimize row; 400×168 room scene at 2× in a wood frame; four vitals bars; up to three action buttons, the first actionable one green. Signature move: room change cuts to black for a beat, the ribbon bounces, the pet walks in from the side it travelled.

FORM: Category standard (canon), chosen by the user over the roll; seed 6595f083.

FINISH: unreviewed and undocumented is unfinished; this build ends with the finish review, the verdict, DESIGN.md, and every shipping raster carrying its provenance
