use crate::i18n::tr;
use std::{fs::DirBuilder, os::unix::fs::DirBuilderExt, path::PathBuf, sync::OnceLock};
use gtk::glib;

static CONFIG_FILE_PATH: OnceLock<PathBuf> = OnceLock::new();

pub fn build_config_path() {
    let dir = glib::user_config_dir().join("CleanMic");

    if let Err(e) = DirBuilder::new().recursive(true).mode(0o700).create(&dir) {
        eprintln!("{} {e}", tr("Fehler beim Erstellen des Verzeichnisses CleanMic."));
        return;
    }

    let _ = CONFIG_FILE_PATH.set(dir.join("config.cfg"));
}



pub fn save_settings(){
    println!("save config");

}

pub fn load_settings(){
    println!("load confing");

}
