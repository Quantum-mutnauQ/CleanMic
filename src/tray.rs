use crate::i18n::tr;
use ksni::{blocking::TrayMethods, menu::StandardItem, MenuItem, Tray, menu::CheckmarkItem};

#[derive(Debug)]
pub enum TrayEvent {
    Show,
    Active,
    Deactive,
    Quit,
}

pub struct CleanMicTray {
    tx: async_channel::Sender<TrayEvent>,
    active: bool,
}

impl Tray for CleanMicTray {
    fn id(&self) -> String {
        "io.github.quantum_mutnauq.CleanMic".into()
    }

    fn title(&self) -> String {
        "CleanMic".into()
    }

    #[cfg(flatpak)]
    fn icon_name(&self) -> String {
        "io.github.quantum_mutnauq.CleanMic".into()
    }
    #[cfg(not(flatpak))]
    fn icon_name(&self) -> String {
        "cleanmic".into()
    }

    #[cfg(not(flatpak))]
    fn icon_theme_path(&self) -> String {
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("assets")))
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send_blocking(TrayEvent::Show);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        vec![
            StandardItem {
                label: tr("Öffnen").into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send_blocking(TrayEvent::Show);
                }),
                ..Default::default()
            }
            .into(),
            CheckmarkItem {
                label: tr("Aktiv").into(),
                checked: self.active,
                activate: Box::new(|t: &mut Self|{
                    t.active = !t.active;

                    if t.active {
                        let _ = t.tx.send_blocking(TrayEvent::Active);
                    }else{
                        let _ = t.tx.send_blocking(TrayEvent::Deactive);
                    }
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: tr("Beenden").into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send_blocking(TrayEvent::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub fn start() -> async_channel::Receiver<TrayEvent> {
    let (tx, rx) = async_channel::unbounded();

    #[cfg(flatpak)]
    let disable_name = true;
    #[cfg(not(flatpak))]
    let disable_name = false;

    match (CleanMicTray { tx , active: true }).disable_dbus_name(disable_name).spawn() {
        Ok(handle) => {
            std::mem::forget(handle);
        }
        Err(e) => eprintln!("Tray konnte nicht gestartet werden: {e}"),
    }

    rx
}
