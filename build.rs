use std::{env, fs, path::{Path, PathBuf}, process::Command};

const APP_ID: &str = "io.github.quantum_mutnauq.CleanMic";
const PACKAGE: &str = "CleanMic";
const LANGS: &[&str] = &["de", "en"];

fn run(cmd: &mut Command) {
    let status = cmd.status().unwrap_or_else(|e| panic!("{cmd:?} konnte nicht starten: {e}"));
    assert!(status.success(), "{cmd:?} fehlgeschlagen");
}

fn copy(from: &Path, to: &Path) {
    fs::create_dir_all(to.parent().unwrap()).unwrap();
    fs::copy(from, to).unwrap_or_else(|e| panic!("{from:?} -> {to:?}: {e}"));
}

fn main() {
    println!("cargo:rerun-if-changed=src/assets");
    println!("cargo:rerun-if-changed=src/locale");
    println!("cargo:rerun-if-env-changed=FLATPAK_BUILD");
    println!("cargo:rerun-if-env-changed=FLATPAK_ID");
    println!("cargo:rustc-check-cfg=cfg(flatpak)");

    let flatpak = env::var("FLATPAK_BUILD").is_ok_and(|v| v == "1" || v == "ON");
    let flatpak_id = env::var("FLATPAK_ID").unwrap_or_default();
    if flatpak && flatpak_id.is_empty() {
        panic!("FLATPAK_ID muss gesetzt sein!");
    }
    if flatpak {
        println!("cargo:rustc-cfg=flatpak");
    }

    let src = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let assets = src.join("src/assets");

    // Zielverzeichnisse (entspricht deinen CMake-Variablen)
    let base: PathBuf = if flatpak {
        PathBuf::from("/app/share")
    } else {
        PathBuf::from(env::var("OUT_DIR").unwrap())
            .ancestors().nth(3).unwrap().to_path_buf() // target/<profile>
    };
    let (locale_dir, icon_dir, schema_dir, metainfo_dir, desktop_dir) = if flatpak {
        (
            base.join("locale"),
            base.join("icons/hicolor/scalable/apps"),
            base.join("glib-2.0/schemas"),
            base.join("metainfo"),
            base.join("applications"),
        )
    } else {
        (
            base.join("locale"),
            base.join("assets/hicolor/scalable/apps"),
            base.join("assets/glib-2.0/schemas"),
            base.clone(), // unbenutzt
            base.clone(),
        )
    };

    // 1. .po -> .mo
    for lang in LANGS {
        let po = src.join(format!("src/locale/{lang}/LC_MESSAGES/{PACKAGE}.po"));
        let mo = locale_dir.join(format!("{lang}/LC_MESSAGES/{PACKAGE}.mo"));
        fs::create_dir_all(mo.parent().unwrap()).unwrap();
        run(Command::new("msgfmt").arg(&po).arg("-o").arg(&mo));
    }

    // 2. Icon
    let icon_name = if flatpak { flatpak_id.clone() } else { "cleanmic".into() };
    let icon = icon_dir.join(format!("{icon_name}.svg"));
    copy(&assets.join("CleanMic.svg"), &icon);

    // 3. GSettings-Schema
    let schema_file = format!("{APP_ID}.gschema.xml");
    copy(&assets.join(&schema_file), &schema_dir.join(&schema_file));
    run(Command::new("glib-compile-schemas").arg(&schema_dir));

    // 4. Metainfo (nur Flatpak)
    if flatpak {
        copy(
            &assets.join("metainfo.xml"),
            &metainfo_dir.join(format!("{flatpak_id}.metainfo.xml")),
        );
    }

    // 5. Desktop-Datei
    let template = fs::read_to_string(assets.join("CleanMic.desktop.in")).unwrap();
    let (exec, icon_value, desktop_path) = if flatpak {
        (
            "cleanmic".to_string(),
            flatpak_id.clone(),
            desktop_dir.join(format!("{flatpak_id}.desktop")),
        )
    } else {
        (
            format!("\"{}\"", base.join(PACKAGE).display()),
            icon.display().to_string(),
            base.join("cleanmic.desktop"),
        )
    };
    let desktop = template
        .replace("@EXEC_COMMAND@", &exec)
        .replace("@ICON_NAME@", &icon_value);
    fs::create_dir_all(desktop_path.parent().unwrap()).unwrap();
    fs::write(&desktop_path, desktop).unwrap();
}
