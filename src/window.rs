//! The window's three states (hidden in the tray, popover, expanded): showing, hiding, resizing,
//! and placing it beside the tray icon.

use crate::tray::TrayMsg;
use crate::{App, View};
use eframe::egui::{self, Pos2, Rect, Vec2, ViewportCommand, pos2};

pub const POPOVER: Vec2 = Vec2::new(440.0, 354.0);
pub const EXPANDED: Vec2 = Vec2::new(740.0, 438.0);
pub const HATCH: Vec2 = Vec2::new(440.0, 460.0);
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
        } else if self.view == View::Expanded {
            EXPANDED
        } else {
            POPOVER
        }
    }

    /// Show, hide or resize the window. `near` is the tray icon (points), when known.
    pub(crate) fn set_view(&mut self, ctx: &egui::Context, v: View, near: Option<Rect>) {
        if v == View::Tray && (self.tray.is_none() || self.save.pet.is_none()) {
            return; // nothing would bring the window back
        }
        let from = self.view;
        let t = self.now_t();
        self.view = v;
        if v == View::Tray {
            if from != View::Tray {
                ctx.send_viewport_cmd(ViewportCommand::Visible(false));
                self.hidden_at = t;
            }
            return;
        }
        if from == View::Tray {
            self.room = self.pet_room().unwrap_or(self.room);
            self.attention = false;
            self.shown_at = t;
            self.had_focus = false;
            self.fx.clear();
            self.floaters.clear();
        }
        let size = self.size();
        let pos = match near {
            Some(icon) => place_near(Some(icon), size, self.screen, BOTTOM_BAR),
            None if self.win_rect.is_positive() => keep_corner(self.win_rect, size, self.screen),
            None => place_near(None, size, self.screen, BOTTOM_BAR),
        };
        self.win_rect = Rect::from_min_size(pos, size);
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
        if from == View::Tray {
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
    }

    pub(crate) fn handle_tray(&mut self, ctx: &egui::Context, t: f64) {
        let msgs: Vec<TrayMsg> = self.tray.as_ref().map(|tr| tr.rx.try_iter().collect()).unwrap_or_default();
        for m in msgs {
            match m {
                TrayMsg::Click(at) => {
                    let near = at.map(|r| Rect::from_min_max((r.min.to_vec2() / self.ppp).to_pos2(), (r.max.to_vec2() / self.ppp).to_pos2()));
                    if self.view != View::Tray {
                        self.set_view(ctx, View::Tray, None);
                    } else if t - self.hidden_at > 0.35 {
                        // A click right after the popover hid itself (it lost focus to that very click) means "close".
                        self.set_view(ctx, View::Popover, near);
                    }
                }
                TrayMsg::Open => {
                    if self.view == View::Tray {
                        self.set_view(ctx, View::Popover, None);
                    } else {
                        ctx.send_viewport_cmd(ViewportCommand::Focus);
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
