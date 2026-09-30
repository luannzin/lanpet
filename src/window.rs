//! The window's states (hidden in the tray, the desktop pet, popover, expanded): showing, hiding,
//! resizing, placing it beside the tray icon, and walking it along the screen as the desktop pet.

use crate::game::Job;
use crate::tray::TrayMsg;
use crate::{App, View};
use eframe::egui::{self, Pos2, Rect, Vec2, ViewportCommand, WindowLevel, pos2};

pub const POPOVER: Vec2 = Vec2::new(440.0, 354.0);
pub const EXPANDED: Vec2 = Vec2::new(740.0, 438.0);
pub const HATCH: Vec2 = Vec2::new(440.0, 460.0);
/// The desktop pet: the sprite at 2x with headroom for a speech bubble.
pub const ROAM: Vec2 = Vec2::new(140.0, 132.0);
/// How long a needy pet stays in the tray after you close the window on it.
const SNOOZE: f64 = 15.0 * 60.0;
/// Where the popover opens when the tray can't say where its icon is: Windows' taskbar sits at the
/// bottom, macOS and most Linux desktops keep their bar at the top.
const BOTTOM_BAR: bool = cfg!(windows);

/// Top-left for a window of `size` beside the tray icon (points). Opens away from the screen edge
/// the icon is on; without an icon position it goes to the corner where the bar usually is.
fn place_near(icon: Option<Rect>, size: Vec2, screen: Vec2, bottom_bar: bool) -> Pos2 {
    const M: f32 = 8.0;
    let fit_x = |x: f32| x.clamp(M, (screen.x - size.x - M).max(M));
    let fit_y = |y: f32| y.clamp(M, (screen.y - size.y - M).max(M));
    match icon {
        Some(icon) => {
            let y = if icon.center().y > screen.y / 2.0 { icon.min.y - size.y - M } else { icon.max.y + M };
            pos2(fit_x(icon.center().x - size.x / 2.0), fit_y(y))
        }
        None => pos2(fit_x(screen.x - size.x - 12.0), fit_y(if bottom_bar { screen.y - size.y - 56.0 } else { 36.0 })),
    }
}

/// Top-left after resizing a window to `size`, keeping the corner that faces the nearest screen corner.
fn keep_corner(cur: Rect, size: Vec2, screen: Vec2) -> Pos2 {
    let x = if cur.center().x > screen.x / 2.0 { cur.max.x - size.x } else { cur.min.x };
    let y = if cur.center().y > screen.y / 2.0 { cur.max.y - size.y } else { cur.min.y };
    pos2(x.max(0.0), y.max(0.0))
}

impl App {
    pub(crate) fn size(&self) -> Vec2 {
        if self.save.pet.is_none() {
            HATCH
        } else {
            match self.view {
                View::Roam => ROAM,
                View::Expanded => EXPANDED,
                _ => POPOVER,
            }
        }
    }

    /// Show, hide or resize the window. `View::Tray` means "close it": the pet hides in the tray,
    /// or walks the desktop if that's where it lives. `near` is the tray icon (points), when known.
    pub(crate) fn set_view(&mut self, ctx: &egui::Context, v: View, near: Option<Rect>) {
        let v = if v == View::Tray && self.lives_out() { View::Roam } else { v };
        if v == View::Tray && self.tray.is_none() {
            return; // nothing would bring the window back
        }
        let from = self.view;
        let was_open = self.open();
        let t = self.now_t();
        self.view = v;
        if was_open && !self.open() {
            self.hidden_at = t;
        }
        if v == View::Tray {
            if from != View::Tray {
                ctx.send_viewport_cmd(ViewportCommand::Visible(false));
                self.snooze = t + SNOOZE;
            }
            return;
        }
        if self.open() && !was_open {
            self.room = self.pet_room().unwrap_or(self.room);
            self.attention = false;
            self.shown_at = t;
            self.had_focus = false;
        }
        if was_open != self.open() || from == View::Tray {
            // the room and the desktop pet don't share coordinates
            self.fx.clear();
            self.floaters.clear();
        }
        let size = self.size();
        let pos = if v == View::Roam {
            // hop out of the window where it stood, else carry on from where the pet last walked
            let x = if was_open { self.win_rect.center().x - size.x / 2.0 } else { self.roam.pos.x };
            let floor = self.save.floor.unwrap_or(self.screen.y - size.y - if BOTTOM_BAR { 48.0 } else { 0.0 });
            let pos = pos2(x.clamp(0.0, (self.screen.x - size.x).max(0.0)), floor.clamp(0.0, (self.screen.y - size.y).max(0.0)));
            self.roam.pos = pos;
            self.roam.target_x = pos.x;
            self.roam.wander_at = t + 2.0;
            self.roam.held = false;
            pos
        } else {
            // the popover opens beside the tray icon, or beside the desktop pet it was opened from
            match near.or((from == View::Roam).then_some(self.win_rect)) {
                Some(icon) => place_near(Some(icon), size, self.screen, BOTTOM_BAR),
                None if self.win_rect.is_positive() => keep_corner(self.win_rect, size, self.screen),
                None => place_near(None, size, self.screen, BOTTOM_BAR),
            }
        };
        self.win_rect = Rect::from_min_size(pos, size);
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
        if from == View::Tray {
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        }
        // X11 forgets "always on top" each time the window leaves the screen, so ask again once it's back
        ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
        if self.open() && !was_open {
            ctx.send_viewport_cmd(ViewportCommand::Focus); // the desktop pet never takes focus
        }
    }

    /// The desktop pet: walks the window along the screen, idles, trains or sleeps where it
    /// stands, and can be picked up and put down somewhere else (which becomes its floor).
    pub(crate) fn roam_step(&mut self, ctx: &egui::Context, job: Option<Job>, dt: f32, t: f64) {
        if self.roam.held {
            // The OS is dragging the window and doesn't say when it lets go; still for a moment means put down.
            if self.win_rect.min != self.roam.pos {
                self.roam.pos = self.win_rect.min;
                self.roam.still_at = t;
            } else if t - self.roam.still_at > 0.35 {
                self.roam.held = false;
                self.roam.target_x = self.roam.pos.x;
                self.roam.wander_at = t + 2.0;
                self.save.floor = Some(self.roam.pos.y);
                self.dirty = true;
                self.m.sv += 6.0;
            }
            self.m.moving = false;
            return;
        }
        const LAP: f32 = 300.0;
        let max_x = (self.screen.x - ROAM.x).max(0.0);
        let x = self.roam.pos.x;
        match job {
            // laps: turn around at the end of each, or at the edge of the screen
            Some(Job::Run) if (self.roam.target_x - x).abs() <= 1.0 => {
                let right = if x + LAP > max_x { false } else { x < LAP || self.m.flip };
                self.roam.target_x = if right { x + LAP } else { x - LAP };
            }
            None if t > self.roam.wander_at => {
                self.roam.wander_at = t + 4.0 + self.rng.f32() as f64 * 8.0;
                if self.rng.chance(70) {
                    self.roam.target_x = x + (self.rng.f32() - 0.5) * 700.0;
                }
            }
            _ => {}
        }
        self.roam.target_x = self.roam.target_x.clamp(0.0, max_x);
        let dx = self.roam.target_x - x;
        self.m.moving = dx.abs() > 1.0 && t >= self.m.react_until && (job.is_none() || job == Some(Job::Run));
        if self.m.moving {
            let speed = if job == Some(Job::Run) { 160.0 } else { 50.0 };
            let before = self.roam.pos.round();
            self.roam.pos.x += dx.signum() * (speed * dt).min(dx.abs());
            self.m.flip = dx < 0.0;
            if self.roam.pos.round() != before {
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(self.roam.pos.round()));
            }
        }
    }

    pub(crate) fn handle_tray(&mut self, ctx: &egui::Context, t: f64) {
        let msgs: Vec<TrayMsg> = self.tray.as_ref().map(|tr| tr.rx.try_iter().collect()).unwrap_or_default();
        for m in msgs {
            match m {
                TrayMsg::Click(at) => {
                    let near = at.map(|r| Rect::from_min_max((r.min.to_vec2() / self.ppp).to_pos2(), (r.max.to_vec2() / self.ppp).to_pos2()));
                    if self.open() {
                        self.set_view(ctx, View::Tray, None);
                    } else if t - self.hidden_at > 0.35 {
                        // A click right after the popover hid itself (it lost focus to that very click) means "close".
                        self.set_view(ctx, View::Popover, near);
                    }
                }
                TrayMsg::Open => {
                    if self.open() {
                        ctx.send_viewport_cmd(ViewportCommand::Focus);
                    } else {
                        self.set_view(ctx, View::Popover, None);
                    }
                }
                TrayMsg::Out => {
                    self.save.out = !self.save.out;
                    self.dirty = true;
                    if !self.open() {
                        self.set_view(ctx, View::Tray, None);
                    }
                }
                TrayMsg::Quit => self.quit(ctx),
            }
        }
    }

    /// Follows the window: screen size, scale, where it is, first placement, and the popover's
    /// habit of closing when you click somewhere else.
    pub(crate) fn track_window(&mut self, ctx: &egui::Context, t: f64) {
        let (outer, monitor, ppp, focused) = ctx.input(|i| {
            let v = i.viewport();
            (v.outer_rect, v.monitor_size, v.native_pixels_per_point, v.focused)
        });
        if let Some(m) = monitor.filter(|m| m.x > 0.0) {
            self.screen = m;
        }
        if let Some(p) = ppp {
            self.ppp = p;
        }
        if let Some(r) = outer {
            self.win_rect = r;
        }
        if ctx.cumulative_frame_nr() == 1 {
            // eframe shows the window after its first frame; X11 ignored "always on top" while it was still hidden
            ctx.send_viewport_cmd(ViewportCommand::WindowLevel(WindowLevel::AlwaysOnTop));
        }
        if !self.placed && monitor.is_some() {
            self.placed = true;
            let size = self.size();
            let pos = place_near(None, size, self.screen, BOTTOM_BAR);
            self.win_rect = Rect::from_min_size(pos, size);
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
        }
        if self.view == View::Popover && self.tray.is_some() && self.save.pet.is_some() {
            match focused {
                Some(true) => self.had_focus = true,
                Some(false) if self.had_focus && t - self.shown_at > 0.4 => {
                    self.had_focus = false;
                    self.set_view(ctx, View::Tray, None);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn quit(&mut self, ctx: &egui::Context) {
        if self.save.pet.is_some() {
            self.persist();
        }
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::vec2;

    #[test]
    fn windows_open_beside_the_tray_and_keep_their_corner() {
        let screen = vec2(1920.0, 1080.0);
        // bottom-right taskbar icon: open above it, pulled in from the right edge
        let icon = Rect::from_min_size(pos2(1850.0, 1050.0), vec2(24.0, 24.0));
        assert_eq!(place_near(Some(icon), POPOVER, screen, true), pos2(1920.0 - POPOVER.x - 8.0, 1050.0 - POPOVER.y - 8.0));
        // top menu-bar icon: open below it
        let icon = Rect::from_min_size(pos2(900.0, 4.0), vec2(22.0, 22.0));
        assert_eq!(place_near(Some(icon), POPOVER, screen, false), pos2(911.0 - POPOVER.x / 2.0, 34.0));
        // unknown icon: the bar's corner
        assert_eq!(place_near(None, POPOVER, screen, false).y, 36.0);
        assert_eq!(place_near(None, POPOVER, screen, true).y, 1080.0 - POPOVER.y - 56.0);
        // growing a bottom-right popover keeps its bottom-right corner; a top-left one keeps its top-left
        let cur = Rect::from_min_size(pos2(1400.0, 600.0), POPOVER);
        assert_eq!(keep_corner(cur, EXPANDED, screen) + EXPANDED, cur.max);
        let cur = Rect::from_min_size(pos2(40.0, 40.0), POPOVER);
        assert_eq!(keep_corner(cur, EXPANDED, screen), cur.min);
    }
}
