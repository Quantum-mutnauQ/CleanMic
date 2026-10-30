mod i18n;
mod UI;
mod Config;

use UI::on_activate;
use Config::build_config_path;
use gtk::prelude::*;
use gtk::{glib, Application,};
use gettextrs::{bind_textdomain_codeset, bindtextdomain, textdomain};

fn main() -> glib::ExitCode {
    #[cfg(flatpak)]
    let locale_dir = std::path::PathBuf::from("/app/share/locale");

    #[cfg(not(flatpak))]
    let locale_dir = {
        let exe_dir = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
        unsafe {
            std::env::set_var("GSETTINGS_SCHEMA_DIR", exe_dir.join("assets/glib-2.0/schemas"));
        }
        exe_dir.join("locale")
    };

    bindtextdomain("CleanMic", locale_dir).unwrap();
    bind_textdomain_codeset("CleanMic", "UTF-8").unwrap();
    textdomain("CleanMic").unwrap();

    build_config_path();

    let app = Application::builder()
        .application_id("io.github.quantum_mutnauq.CleanMic")
        .build();

    app.connect_activate(on_activate);

    app.run()
}
