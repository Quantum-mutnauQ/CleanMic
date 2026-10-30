use crate::i18n::tr;
use crate::Config::{save_settings, load_settings};

use gtk::prelude::*;
use gtk::*;


const APP_ID: &str = "io.github.quantum_mutnauq.CleanMic";

#[cfg(not(flatpak))]
fn create_settings() -> Option<gio::Settings> {
    let Some(exe) = std::env::current_exe().ok() else {
        eprintln!("Executable konnte nicht gefunden werden!");
        return None;
    };

    let Some(parent) = exe.parent() else {
        eprintln!("Executable-Verzeichnis konnte nicht gefunden werden!");
        return None;
    };

    let schema_dir = parent.join("assets/glib-2.0/schemas");

    let source = match gio::SettingsSchemaSource::from_directory(&schema_dir, None, true) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Fehler beim Laden der Schema-Quelle: {}", e.message());
            return None;
        }
    };

    let Some(schema) = source.lookup(&format!("{}.State", APP_ID), true) else {
        eprintln!("Schema nicht gefunden!");
        return None;
    };

    Some(gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None))
}

#[cfg(flatpak)]
fn create_settings() -> Option<gio::Settings> {
    Some(gio::Settings::new(APP_ID))
}



#[cfg(not(flatpak))]
fn setup_icons() {
    gtk::Window::set_default_icon_name("cleanmic");

    if let Some(display) = gtk::gdk::Display::default() {
        if let Some(exe_dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) {
            gtk::IconTheme::for_display(&display).add_search_path(exe_dir.join("assets"));
        }
    }
}

#[cfg(flatpak)]
fn setup_icons() {
    gtk::Window::set_default_icon_name(APP_ID);
}

pub fn create_menu_bar(app: &gtk::Application, window: &gtk::ApplicationWindow) -> gtk::PopoverMenuBar {
    let quit_action = gio::SimpleAction::new("quit", None);
    quit_action.connect_activate({
        let window = window.clone();
        move |_, _| on_quit_activate(&window)
    });
    app.add_action(&quit_action);

    let about_action = gio::SimpleAction::new("about", None);
    about_action.connect_activate(|_, _| on_about_activate());
    app.add_action(&about_action);

    let toggle_fullscreen_action = gio::SimpleAction::new("toggle-fullscreen", None);
    toggle_fullscreen_action.connect_activate({
        let window = window.clone();
        move |_, _| toggle_fullscreen(&window)
    });
    app.add_action(&toggle_fullscreen_action);

    app.set_accels_for_action("app.toggle-fullscreen", &["F11"]);
    app.set_accels_for_action("app.quit", &["<Primary>q"]);

    let menu_model = gio::Menu::new();

    let file_menu = gio::Menu::new();
    file_menu.append(Some(&tr("Schließen")), Some("app.quit"));
    menu_model.append_submenu(Some(&tr("Datei")), &file_menu);

    let view_menu = gio::Menu::new();
    view_menu.append(Some(&tr("Vollbild")), Some("app.toggle-fullscreen"));
    menu_model.append_submenu(Some(&tr("Ansicht")), &view_menu);

    let help_menu = gio::Menu::new();
    help_menu.append(Some(&tr("Über")), Some("app.about"));
    menu_model.append_submenu(Some(&tr("Hilfe")), &help_menu);

    gtk::PopoverMenuBar::from_model(Some(&menu_model))
}

fn on_quit_activate(window: &gtk::ApplicationWindow) {
    window.close();
}

fn on_about_activate() {
    let about_dialog = gtk::AboutDialog::new();

    about_dialog.set_program_name(Some("CleanMic"));
    about_dialog.set_version(Some("4"));
    about_dialog.set_comments(Some(&tr("Clean the microphone audio")));
    about_dialog.set_website(Some("https://github.com/Quantum-mutnauQ/CleanMic"));
    about_dialog.set_license_type(gtk::License::Gpl20);

    #[cfg(flatpak)]
    about_dialog.set_logo_icon_name(APP_ID);
    #[cfg(not(flatpak))]
    about_dialog.set_logo_icon_name(Some("cleanmic"));

    about_dialog.set_authors(&[
        "Quantum-mutnauQ",
    ]);

    about_dialog.present();
}

fn toggle_fullscreen(window: &gtk::ApplicationWindow) {
    window.set_fullscreened(!window.is_fullscreen());
}

fn on_close_request(_window: &gtk::ApplicationWindow) -> glib::Propagation {
    save_settings();
    glib::Propagation::Proceed
}

pub fn on_activate(app: &gtk::Application) {
    setup_icons();

    let Some(settings) = create_settings() else { return };

    let window = ApplicationWindow::builder()
        .application(app)
        .title("CleanMic")
        .default_width(420)
        .default_height(320)
        .resizable(true)
        .build();

    settings
        .bind("width", &window, "default-width")
        .flags(gio::SettingsBindFlags::DEFAULT)
        .build();
    settings
        .bind("height", &window, "default-height")
        .flags(gio::SettingsBindFlags::DEFAULT)
        .build();
    settings
        .bind("is-maximized", &window, "maximized")
        .flags(gio::SettingsBindFlags::DEFAULT)
        .build();
    settings
        .bind("is-fullscreen", &window, "fullscreened")
        .flags(gio::SettingsBindFlags::DEFAULT)
        .build();

    #[cfg(flatpak)]
    dw::StyleManager::default().set_color_scheme(adw::ColorScheme::Default);

    let main_box = gtk::Box::new(Orientation::Vertical, 0);

    let Some(app) = window.application() else { return };
    let menu_bar = create_menu_bar(&app, &window);
    main_box.append(&menu_bar);

    load_settings();

    window.connect_close_request(on_close_request);

    window.set_child(Some(&main_box));
    window.present();
}
