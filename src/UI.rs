use crate::i18n::tr;
use crate::config::{save_settings, load_settings};
use crate::effects::{available_effects, EffectId, Effect, ParamSender};
use crate::audio::AudioCommand;
use std::rc::Rc;
use std::cell::Cell;
use pipewire as pw;

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
        let app = app.clone();
        move |_, _| on_quit_activate(&app)
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

fn on_quit_activate(app: &gtk::Application) {
    app.quit()
}

fn on_about_activate() {
    let about_dialog = gtk::AboutDialog::new();

    about_dialog.set_program_name(Some("CleanMic"));
    about_dialog.set_version(Some("0.0.0"));
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
pub fn on_activate(app: &gtk::Application, auto_start: bool) {
    setup_icons();

    let Some(settings) = create_settings() else { return };

    let window = ApplicationWindow::builder()
        .application(app)
        .title("CleanMic")
        .default_width(420)
        .default_height(320)
        .resizable(true)
        .hide_on_close(true)
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

    let ui_box = gtk::Box::new(Orientation::Vertical, 0);

    let Some(app) = window.application() else { return };
    let menu_bar = create_menu_bar(&app, &window);
    ui_box.append(&menu_bar);

    let main_box = gtk::Box::new(Orientation::Vertical, 12);
    main_box.set_margin_top(7);
    main_box.set_margin_bottom(10);
    main_box.set_margin_start(10);
    main_box.set_margin_end(7);

    let input_label = gtk::Label::new(Some(&tr("Eingang")));
    input_label.set_halign(gtk::Align::Start);
    main_box.append(&input_label);

    let input_dropdown = gtk::DropDown::from_strings(&[
        &tr("Etwas tut nicht!!"),
    ]);
    input_dropdown.set_hexpand(true);
    main_box.append(&input_dropdown);

    let mic_settings_hbox = gtk::Box::new(Orientation::Horizontal, 12);
    main_box.append(&mic_settings_hbox);

    let mic_gain_label = gtk::Label::new(Some(&tr("Microphone Lautschärke")));
    mic_settings_hbox.append(&mic_gain_label);

    let mic_gain_adjustment = gtk::Adjustment::new(100.0, 0.0, 200.0, 1.0, 10.0, 0.0);
    let mic_gain_spinner = gtk::SpinButton::new(Some(&mic_gain_adjustment), 1.0, 0,);
    mic_settings_hbox.append(&mic_gain_spinner);

    let effects_active = gtk::CheckButton::with_label(&tr("Effeckte anwenden"));
    effects_active.set_halign(gtk::Align::End);
    effects_active.set_active(true);
    mic_settings_hbox.append(&effects_active);


    let effects_frame = gtk::Frame::new(None);
    effects_frame.set_hexpand(true);
    effects_frame.set_vexpand(true);
    main_box.append(&effects_frame);

    let effects_scraller = gtk::ScrolledWindow::new();
    effects_frame.set_child(Some(&effects_scraller));

    let effeckts_list = gtk::ListBox::new();
    effeckts_list.set_selection_mode(gtk::SelectionMode::None);
    effects_scraller.set_child(Some(&effeckts_list));



    let add_button = gtk::Button::with_label(&tr("➕ Effeckt hinzufügen"));
    add_button.set_hexpand(true);
    main_box.append(&add_button);

    let default_input = gtk::CheckButton::with_label(&tr("Als Standard-Eingabe verwenden"));
    default_input.set_active(true);
    main_box.append(&default_input);

    let autostart = gtk::CheckButton::with_label(&tr("Autostart in tray"));
    autostart.set_active(true);
    main_box.append(&autostart);


    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&main_box));

    let blocking_box = gtk::Box::new(Orientation::Vertical, 0);
    blocking_box.set_hexpand(true);
    blocking_box.set_vexpand(true);
    blocking_box.set_can_target(true);
    blocking_box.set_halign(gtk::Align::Fill);
    blocking_box.set_valign(gtk::Align::Fill);
    blocking_box.set_visible(false);


    let center_box = gtk::Box::new(Orientation::Vertical, 0);
    center_box.set_halign(gtk::Align::Center);
    center_box.set_valign(gtk::Align::Center);
    center_box.set_hexpand(true);
    center_box.set_vexpand(true);

    let error_label = gtk::Label::new(Some(&tr("Verbindung zu PipeWire fehlgeschlagen!")));
    error_label.add_css_class("title-3");
    center_box.append(&error_label);
    blocking_box.append(&center_box);

    overlay.add_overlay(&blocking_box);
    ui_box.append(&overlay);

    load_settings();

    app.connect_shutdown(|_| {
        save_settings();
    });
    window.set_child(Some(&ui_box));

    if let Some((rx, handle)) = crate::tray::start() {
         // UI -> Tray
         effects_active.connect_toggled({
             let handle = handle.clone();
             move |cb| {
                 let active = cb.is_active();
                 handle.update(move |t: &mut crate::tray::CleanMicTray| {
                     t.active = active;
                 });
             }
         });
         // Tray -> UI
         glib::spawn_future_local({
             let window = window.clone();
             let app = app.clone();
             let effects = effects_active.clone();
             async move {
                 let _keep_alive = handle; // Handle am Leben halten
                 while let Ok(event) = rx.recv().await {
                     match event {
                         crate::tray::TrayEvent::Show => window.present(),
                         crate::tray::TrayEvent::Quit => app.quit(),
                         crate::tray::TrayEvent::Active => effects.set_active(true),
                         crate::tray::TrayEvent::Deactive => effects.set_active(false),
                     }
                 }
             }
         });
     }

     let (audio_rx, cmd_tx) = crate::audio::start();
     let cmd_tx = std::rc::Rc::new(cmd_tx);
     let mics: std::rc::Rc<std::cell::RefCell<Vec<crate::audio::Microphone>>> = Default::default();
     let model = gtk::StringList::new(&[]);
     input_dropdown.set_model(Some(&model));

     let next_id = Rc::new(Cell::new(0u64));

     glib::spawn_future_local({
         let blocking_box = blocking_box.clone();
         let error_label = error_label.clone();
         let default_input = default_input.clone();
         let mics = mics.clone();
         let cmd_tx=cmd_tx.clone();
         let input_dropdown=input_dropdown.clone();
         async move {
             use crate::audio::AudioEvent::*;
             while let Ok(ev) = audio_rx.recv().await {
                 match ev {
                     Connected => blocking_box.set_visible(false),
                     ConnectionFailed(e) => {
                         error_label.set_text(&format!("{}\n{e}",tr("Verbindung zu PipeWire fehlgeschlagen!")));
                         blocking_box.set_visible(true);
                     }
                     MicAdded(m) => {
                         let first = mics.borrow().is_empty();
                         let desc = m.description.clone();
                         let name = m.name.clone();
                         mics.borrow_mut().push(m);
                         model.append(&desc);
                         if first {
                             input_dropdown.set_selected(0);
                             let _ = cmd_tx.send(AudioCommand::SelectMic(name));
                         }
                     }
                     MicRemoved(id) => {
                         let pos = mics.borrow().iter().position(|m| m.id == id);
                         if let Some(pos) = pos {
                             mics.borrow_mut().remove(pos);
                             model.remove(pos as u32);
                         }
                     }
                     VirtualMicReady => {
                         if default_input.is_active() {
                             crate::audio::set_default_source(crate::audio::VIRTUAL_MIC_NAME);
                         }
                     }
                 }
             }
         }
     });

     input_dropdown.connect_selected_notify({
         let mics = mics.clone();
         let cmd_tx = cmd_tx.clone();
         move |dd| {
             let idx = dd.selected() as usize;
             if let Some(m) = mics.borrow().get(idx) {
                 let _ = cmd_tx.send(AudioCommand::SelectMic(m.name.clone()));
             }
         }
     });

     mic_gain_spinner.connect_value_changed(|spinner| {
         let gain = spinner.value_as_int();
         println!("Neuer Mic Gain: {}", gain);
     });

     let window_transient = window.clone();
     add_button.connect_clicked({
         let cmd_tx = cmd_tx.clone();
         let next_id = next_id.clone();
         let effeckts_list = effeckts_list.clone();
             move |button| {
             let selection_window = ApplicationWindow::builder().application(&app).build();
             selection_window.set_title(Some(&tr("Effeckt hinzufügen")));
             selection_window.set_default_size(180,360);
             selection_window.set_modal(true);
             selection_window.set_transient_for(Some(&window_transient));
             selection_window.set_visible(true);
             selection_window.set_deletable(false);

             let vbox = gtk::Box::new(Orientation::Vertical, 12);
             vbox.set_margin_top(7);
             vbox.set_margin_bottom(10);
             vbox.set_margin_start(10);
             vbox.set_margin_end(7);
             selection_window.set_child(Some(&vbox));

             let title = gtk::Label::new(Some(&tr("Wähle einen Effekt aus:")));
             vbox.append(&title);

             let search_entry = gtk::SearchEntry::new();
             search_entry.set_placeholder_text(Some(&tr("Effekte suchen...")));
             vbox.append(&search_entry);

             let list_box = gtk::ListBox::new();
             list_box.set_selection_mode(gtk::SelectionMode::Single);



             for effect in available_effects() {
                 let row = gtk::ListBoxRow::new();

                 let label = gtk::Label::new(Some(&tr(effect.name)));
                 label.set_halign(gtk::Align::Start);
                 label.set_margin_top(8);
                 label.set_margin_bottom(8);
                 label.set_margin_start(10);

                 row.set_child(Some(&label));
                 list_box.append(&row);
             }

             let list_box_clone = list_box.clone();

             search_entry.connect_search_changed(move |entry| {
                 let search_text = entry.text().to_string().to_lowercase();

                 let mut child = list_box_clone.first_child();

                 while let Some(widget) = child {
                     let next = widget.next_sibling();

                     if let Some(row) = widget.downcast_ref::<gtk::ListBoxRow>() {
                         if let Some(label) = row.child() {
                             if let Some(label) = label.downcast_ref::<gtk::Label>() {
                                 let text = label.text().to_string().to_lowercase();

                                 row.set_visible(text.contains(&search_text));
                             }
                         }
                     }

                     child = next;
                 }
             });

             let scroll = gtk::ScrolledWindow::new();
             scroll.set_vexpand(true);
             scroll.set_child(Some(&list_box));

             vbox.append(&scroll);

             let add_bttn = gtk::Button::with_label(&tr("Hinzufügen"));
             add_bttn.set_hexpand(true);
             vbox.append(&add_bttn);

             let close_bttn = gtk::Button::with_label(&tr("Schlesen"));
             close_bttn.set_hexpand(true);
             vbox.append(&close_bttn);

             let selection_window_clone = selection_window.clone();
             close_bttn.connect_clicked(move |_| {
                 selection_window_clone.destroy();
             });

             let selection_window_clone = selection_window.clone();
             add_bttn.connect_clicked({
                 let cmd_tx = cmd_tx.clone();
                 let next_id = next_id.clone();
                 let effeckts_list = effeckts_list.clone();
                 move |_| {
                     if let Some(row) = list_box.selected_row() {
                         if let Some(label) = row.child() {
                             let idx = list_box.selected_row().map(|r| r.index() as usize);
                             let descs = available_effects();
                             if let Some(d) = idx.and_then(|i| descs.get(i)) {
                                 let id = next_id.replace(next_id.get() + 1);
                                 let effect = (d.constructor)();
                                 let name = d.name;
                                 let gui = effect.configGUI(make_sender(id, &cmd_tx));
                                 add_effect_row(&effeckts_list, id, name, gui, cmd_tx.clone());
                                 let _ = cmd_tx.send(AudioCommand::AddEffect { id, effect });
                             }
                         }
                     }
                 }
             });

            selection_window.present();
         }
     });

     if !(auto_start && autostart.is_active()) {
         window.present();
     }
}

fn make_sender(id: EffectId, tx: &Rc<pw::channel::Sender<AudioCommand>>) -> ParamSender {
    let tx = tx.clone();
    Rc::new(move |param, value| {
        let _ = tx.send(AudioCommand::SetParam { id, param, value });
    })
}

fn count_rows(list: &gtk::ListBox) -> i32 {
    let mut n = 0;
    let mut child = list.first_child();
    while let Some(w) = child {
        n += 1;
        child = w.next_sibling();
    }
    n
}

fn add_effect_row(list: &gtk::ListBox, id: EffectId, name: &str, config_gui: gtk::Box, tx: Rc<pw::channel::Sender<AudioCommand>>,) {
    let send: ParamSender = {
        let tx = tx.clone();
        Rc::new(move |param, value| {
            let _ = tx.send(AudioCommand::SetParam { id, param, value });
        })
    };

    let (settings, check_button, up, down, remove) = effectTemplateGUI(name.to_string(), &config_gui);

    let row = gtk::ListBoxRow::new();
    row.set_activatable(false);
    row.set_child(Some(&settings));
    list.append(&row);

    check_button.connect_toggled({
        let tx = tx.clone();
        move |cb| {
            let _ = tx.send(AudioCommand::SetEffectEnabled { id, enabled: cb.is_active() });
        }
    });

    up.connect_clicked(glib::clone!(
        #[weak] list, #[weak] row, #[strong] tx,
        move |_| move_row(&list, &row, row.index() - 1, id, &tx)
    ));
    down.connect_clicked(glib::clone!(
        #[weak] list, #[weak] row, #[strong] tx,
        move |_| move_row(&list, &row, row.index() + 1, id, &tx)
    ));

    remove.connect_clicked(glib::clone!(
        #[weak] list, #[weak] row, #[strong] tx,
        move |_| {
            list.remove(&row);
            let _ = tx.send(AudioCommand::RemoveEffect(id));
        }
    ));
}

fn move_row(list: &gtk::ListBox, row: &gtk::ListBoxRow, new_index: i32, id: EffectId, tx: &pw::channel::Sender<AudioCommand>) {
    let max = count_rows(list) - 1;
    let new_index = new_index.clamp(0, max);
    if new_index == row.index() { return; }
    list.remove(row);
    list.insert(row, new_index);
    let _ = tx.send(AudioCommand::MoveEffect { id, new_index: new_index as usize });
}

pub fn effectTemplateGUI(title: String, setting: &gtk::Box) -> (gtk::Box, gtk::CheckButton, gtk::Button, gtk::Button, gtk::Button,){
    let vbox = gtk::Box::new(Orientation::Vertical, 0);

    let titleCheck = gtk::CheckButton::with_label(&title);
    titleCheck.set_active(true);
    vbox.append(&titleCheck);

    let settingsExpander = gtk::Expander::new(Some(&tr("Einstellungen")));
    settingsExpander.set_child(Some(setting));
    vbox.append(&settingsExpander);


    let hbox = gtk::Box::new(Orientation::Horizontal, 0);
    vbox.append(&hbox);

    let up = gtk::Button::with_label("▲");
    hbox.append(&up);

    let down = gtk::Button::with_label("▼");
    hbox.append(&down);

    let remove = gtk::Button::with_label(&tr("Löschen"));
    remove.set_hexpand(true);
    hbox.append(&remove);


    (vbox, titleCheck , up, down, remove)
}
