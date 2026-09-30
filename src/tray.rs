//! System tray icon: the dino, with a badge while the window is hidden: red when the pet needs
//! you, gold when something happened.
//! Linux uses the pure-Rust StatusNotifierItem backend (no GTK); its hosts rarely say where the icon is.

use eframe::egui::{Context, Rect, pos2, vec2};
use std::sync::mpsc::{Receiver, channel};
use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub enum TrayMsg {
    /// Left click on the icon; where it is on screen (physical pixels) when the platform says.
    Click(Option<Rect>),
    Open,
    /// Let the pet out on the desktop, or call it back in.
    Out,
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    None,
    Done,
    Need,
}

pub struct Tray {
    icon: TrayIcon,
    /// Indexed by `Badge`.
    icons: [Icon; 3],
    badge: Badge,
    tip: String,
    pub rx: Receiver<TrayMsg>,
}

fn icon(png: &[u8]) -> Icon {
    let img = image::load_from_memory(png).expect("embedded tray icon is a valid png").to_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).expect("tray icon has a valid size")
}

impl Tray {
    /// None when the desktop has no tray (e.g. GNOME without the AppIndicator extension).
    pub fn start(ctx: &Context) -> Option<Tray> {
        let open = MenuItem::new("Open LanPet", true, None);
        let out = MenuItem::new("Pet on the desktop: on/off", true, None);
        let quit = MenuItem::new("Quit", true, None);
        let menu = Menu::new();
        menu.append_items(&[&open, &out, &quit]).ok()?;
        let (open_id, out_id, quit_id): (MenuId, MenuId, MenuId) = (open.id().clone(), out.id().clone(), quit.id().clone());

        let (tx, rx) = channel();
        let (tx2, wake, wake2) = (tx.clone(), ctx.clone(), ctx.clone());
        TrayIconEvent::set_event_handler(Some(move |e| {
            if let TrayIconEvent::Click { position, rect, button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                let at = if rect.size.width > 0 {
                    Some(Rect::from_min_size(pos2(rect.position.x as f32, rect.position.y as f32), vec2(rect.size.width as f32, rect.size.height as f32)))
                } else if position.x != 0.0 || position.y != 0.0 {
                    Some(Rect::from_min_size(pos2(position.x as f32, position.y as f32), vec2(0.0, 0.0)))
                } else {
                    None
                };
                let _ = tx.send(TrayMsg::Click(at));
                wake.request_repaint();
            }
        }));
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let msg = if e.id == open_id {
                TrayMsg::Open
            } else if e.id == out_id {
                TrayMsg::Out
            } else if e.id == quit_id {
                TrayMsg::Quit
            } else {
                return;
            };
            let _ = tx2.send(msg);
            wake2.request_repaint();
        }));

        let icons = [
            icon(include_bytes!("../assets/tray.png")),
            icon(include_bytes!("../assets/tray_done.png")),
            icon(include_bytes!("../assets/tray_need.png")),
        ];
        let built = TrayIconBuilder::new()
            .with_icon(icons[0].clone())
            .with_tooltip("LanPet")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build();
        match built {
            Ok(icon) => Some(Tray { icon, icons, badge: Badge::None, tip: String::new(), rx }),
            Err(e) => {
                eprintln!("lanpet: no system tray ({e}); the pet walks the desktop instead");
                None
            }
        }
    }

    pub fn badge(&mut self, badge: Badge) {
        if badge != self.badge {
            self.badge = badge;
            let _ = self.icon.set_icon(Some(self.icons[badge as usize].clone()));
        }
    }

    pub fn tooltip(&mut self, tip: String) {
        if tip != self.tip {
            let _ = self.icon.set_tooltip(Some(&tip));
            self.tip = tip;
        }
    }
}
