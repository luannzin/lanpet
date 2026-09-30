//! Updates from GitHub Releases: a background thread looks for a newer release now and then and
//! downloads its installer; the player installs it from Home and LanPet restarts as the new version.
//! Trust is HTTPS to this repo's releases, the same as downloading the installer by hand.

use crate::game::Save;
use serde::Deserialize;
use std::error::Error;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

const REPO: &str = "luannzin/lanpet";
/// This platform's installer on every release (built by .github/workflows/release.yml).
#[cfg(target_os = "linux")]
const ASSET: &str = "lanpet_amd64.deb";
#[cfg(windows)]
const ASSET: &str = "LanPet-setup.exe";
#[cfg(target_os = "macos")]
const ASSET: &str = "LanPet.dmg";
const EVERY: Duration = Duration::from_secs(6 * 3600);

pub enum Msg {
    /// A newer version's installer is downloaded: (release tag, installer).
    Ready(String, PathBuf),
    /// Installed (on Windows: the installer is running and will start the new LanPet); time to quit.
    Installed,
    Failed(String),
}

pub struct Updater {
    tx: Sender<Msg>,
    pub rx: Receiver<Msg>,
    /// The newer version waiting to be installed: (release tag, installer).
    pub ready: Option<(String, PathBuf)>,
    pub installing: bool,
}

impl Updater {
    pub fn start() -> Updater {
        let (tx, rx) = channel();
        // dev builds (cargo run) aren't installed, so there's nothing to update
        if !cfg!(debug_assertions) {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let dir = Save::dir().join("update");
                loop {
                    match check(&dir) {
                        Ok(Some(ready)) => {
                            let _ = tx.send(ready);
                            return;
                        }
                        Ok(None) => {}
                        Err(e) => eprintln!("lanpet: update check failed: {e}"),
                    }
                    std::thread::sleep(EVERY);
                }
            });
        }
        Updater { tx, rx, ready: None, installing: false }
    }

    /// Installs the downloaded version off the UI thread (Linux asks for a password); `Installed` or `Failed` follows.
    pub fn install(&mut self) {
        let Some((_, file)) = self.ready.clone() else { return };
        if std::mem::replace(&mut self.installing, true) {
            return;
        }
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(match install(&file) {
                Ok(()) => Msg::Installed,
                Err(e) => Msg::Failed(e.to_string()),
            });
        });
    }
}

/// Downloads the latest release's installer into `dir` when that release is newer than us, else
/// clears out `dir` (installers of versions we already run).
fn check(dir: &Path) -> Result<Option<Msg>, Box<dyn Error>> {
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
    }
    let http: ureq::Agent = ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(600))).build().into();
    let body = http.get(format!("https://api.github.com/repos/{REPO}/releases/latest")).call()?.body_mut().read_to_string()?;
    let tag = serde_json::from_str::<Release>(&body)?.tag_name;
    if !newer(&tag, env!("CARGO_PKG_VERSION")) {
        let _ = std::fs::remove_dir_all(dir);
        return Ok(None);
    }
    let file = dir.join(format!("{tag}-{ASSET}"));
    if !file.exists() {
        std::fs::create_dir_all(dir)?;
        let part = file.with_extension(format!("part{}", std::process::id()));
        let mut res = http.get(format!("https://github.com/{REPO}/releases/download/{tag}/{ASSET}")).call()?;
        io::copy(&mut res.body_mut().as_reader(), &mut std::fs::File::create(&part)?)?;
        std::fs::rename(&part, &file)?;
    }
    Ok(Some(Msg::Ready(tag, file)))
}

/// Whether release tag `tag` ("v1.2.3") is a newer version than `current` ("1.2.0"). A tag that isn't
/// a plain version never is, which also keeps it safe to put in a file name.
fn newer(tag: &str, current: &str) -> bool {
    let parse = |v: &str| v.split('.').map(|n| n.parse::<u64>().ok()).collect::<Option<Vec<_>>>();
    match (tag.strip_prefix('v').and_then(parse), parse(current)) {
        (Some(new), Some(cur)) => new > cur,
        _ => false,
    }
}

#[cfg(unix)]
fn run(cmd: &mut Command) -> io::Result<()> {
    if cmd.status()?.success() { Ok(()) } else { Err(io::Error::other(format!("{} failed", cmd.get_program().to_string_lossy()))) }
}

#[cfg(target_os = "linux")]
fn install(deb: &Path) -> io::Result<()> {
    // polkit asks for the password; apt also brings in any new dependency
    run(Command::new("pkexec").args(["apt-get", "install", "-y"]).arg(deb))
}

#[cfg(target_os = "linux")]
pub fn relaunch() {
    let _ = Command::new("/usr/bin/lanpet").spawn();
}

#[cfg(windows)]
fn install(setup: &Path) -> io::Result<()> {
    // the installer closes us if we're still running, then starts the new LanPet
    Command::new(setup).args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS"]).spawn().map(drop)
}

#[cfg(windows)]
pub fn relaunch() {}

/// The LanPet.app we're running from.
#[cfg(target_os = "macos")]
fn bundle() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    exe.ancestors()
        .nth(3)
        .filter(|p| p.extension().is_some_and(|e| e == "app"))
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::other("LanPet isn't running from LanPet.app"))
}

#[cfg(target_os = "macos")]
fn install(dmg: &Path) -> io::Result<()> {
    let app = bundle()?;
    let (mnt, new) = (dmg.with_extension("mnt"), app.with_extension("new"));
    run(Command::new("hdiutil").args(["attach", "-nobrowse", "-readonly", "-mountpoint"]).arg(&mnt).arg(dmg))?;
    let _ = std::fs::remove_dir_all(&new);
    let copied = run(Command::new("ditto").arg(mnt.join("LanPet.app")).arg(&new));
    let _ = Command::new("hdiutil").args(["detach", "-quiet"]).arg(&mnt).status();
    copied?;
    // swap it in whole; the running old version keeps the files it already has open
    std::fs::remove_dir_all(&app)?;
    std::fs::rename(&new, &app)
}

#[cfg(target_os = "macos")]
pub fn relaunch() {
    if let Ok(app) = bundle() {
        let _ = Command::new("open").arg("-n").arg(app).spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::newer;

    #[test]
    fn only_plain_newer_versions_update() {
        assert!(newer("v0.1.5", "0.1.4"));
        assert!(newer("v0.2.0", "0.1.10"));
        assert!(newer("v0.1.10", "0.1.9"));
        assert!(!newer("v0.1.4", "0.1.4"));
        assert!(!newer("v0.1.3", "0.1.4"));
        assert!(!newer("v0.2.0-beta", "0.1.4"));
        assert!(!newer("v0.2/../../x", "0.1.4"));
        assert!(!newer("0.2.0", "0.1.4"));
    }
}
