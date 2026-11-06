//! Składanie polecenia uruchamiającego grę.

use crate::auth::Session;
use crate::install::Ready;
use crate::meta;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn build_command(
    ready: &Ready,
    shared: &Path,
    instance_dir: &Path,
    session: &Session,
    profile_args: Vec<String>,
    prep_ms: u128,
) -> Result<Command, String> {
    let natives = instance_dir.join("natives");
    std::fs::create_dir_all(&natives).map_err(|e| e.to_string())?;

    let sep = if cfg!(windows) { ";" } else { ":" };
    let mut cp: Vec<String> = ready
        .libraries
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    cp.push(ready.client_jar.to_string_lossy().to_string());
    let classpath = cp.join(sep);

    let v = &ready.version;
    let asset_id = v
        .asset_index
        .as_ref()
        .map(|a| a.id.clone())
        .or_else(|| v.assets.clone())
        .unwrap_or_default();

    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("natives_directory", natives.to_string_lossy().to_string());
    vars.insert("launcher_name", "Voidlight".to_string());
    vars.insert("launcher_version", "0.1.0".to_string());
    vars.insert("classpath", classpath.clone());
    vars.insert("classpath_separator", sep.to_string());
    vars.insert(
        "library_directory",
        shared.join("libraries").to_string_lossy().to_string(),
    );
    vars.insert("auth_player_name", session.name.clone());
    vars.insert("auth_uuid", session.uuid.clone());
    vars.insert("auth_access_token", session.access_token.clone());
    vars.insert("clientid", String::new());
    vars.insert("auth_xuid", String::new());
    vars.insert("user_type", session.user_type.clone());
    vars.insert("user_properties", "{}".to_string());
    vars.insert("version_name", v.id.clone());
    vars.insert(
        "version_type",
        v.kind.clone().unwrap_or_else(|| "release".to_string()),
    );
    vars.insert("game_directory", instance_dir.to_string_lossy().to_string());
    vars.insert("assets_root", ready.assets_root.to_string_lossy().to_string());
    vars.insert("assets_index_name", asset_id);

    let args = v
        .arguments
        .as_ref()
        .ok_or_else(|| "Brak argumentów w JSON-ie wersji.".to_string())?;
    let main_class = v
        .main_class
        .clone()
        .ok_or_else(|| "Brak klasy głównej w JSON-ie wersji.".to_string())?;

    let mut jvm = profile_args;

    // Archiwum CDS (Java 19+): przy kolejnych startach JVM wczytuje klasy gotowe z pliku zamiast
    // parsować je od nowa. Pierwszy start tworzy archiwum przy zamknięciu gry, a gdy zmienią się
    // mody lub wersja, JVM sam je odświeża.
    let java_major = v.java_version.as_ref().map(|j| j.major_version).unwrap_or(8);
    if java_major >= 19 {
        let cds_dir = instance_dir.join("cds");
        std::fs::create_dir_all(&cds_dir).map_err(|e| e.to_string())?;
        jvm.push("-XX:+AutoCreateSharedArchive".to_string());
        jvm.push(format!(
            "-XX:SharedArchiveFile={}",
            cds_dir.join("game.jsa").to_string_lossy()
        ));
    }

    jvm.extend(meta::expand_args(&args.jvm, &vars));
    if !jvm.iter().any(|a| a == "-cp" || a == "-classpath") {
        jvm.push("-cp".to_string());
        jvm.push(classpath);
    }
    let game = meta::expand_args(&args.game, &vars);

    let mut log = File::create(instance_dir.join("launcher.log")).map_err(|e| e.to_string())?;
    let _ = writeln!(
        log,
        "[Voidlight] przygotowanie przed startem: {} ms",
        prep_ms
    );
    let shown: Vec<String> = jvm
        .iter()
        .chain(std::iter::once(&main_class))
        .chain(game.iter())
        .map(|a| {
            if !session.access_token.is_empty() && *a == session.access_token {
                "<token>".to_string()
            } else if a.len() > 300 {
                format!("{}…", a.chars().take(120).collect::<String>())
            } else {
                a.clone()
            }
        })
        .collect();
    let _ = writeln!(
        log,
        "[Voidlight] java: {} (istnieje: {})\n[Voidlight] argumenty: {}",
        ready.java.display(),
        ready.java.exists(),
        shown.join(" ")
    );
    let log_err = log.try_clone().map_err(|e| e.to_string())?;

    let mut cmd = Command::new(&ready.java);
    cmd.current_dir(instance_dir)
        .args(&jvm)
        .arg(main_class)
        .args(&game)
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // bez okna konsoli, a komunikaty Javy trafiają do launcher.log
    }
    Ok(cmd)
}
