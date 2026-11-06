//! Pobieranie środowiska Javy od Mojang (to samo, którego używa oficjalny launcher).

use crate::download::{self, Job, Reporter};
use crate::util;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const ALL_URL: &str =
    "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

#[derive(Deserialize)]
struct Entry {
    manifest: ManifestRef,
}

#[derive(Deserialize)]
struct ManifestRef {
    url: String,
}

#[derive(Deserialize)]
struct RuntimeManifest {
    files: HashMap<String, RFile>,
}

#[derive(Deserialize)]
struct RFile {
    #[serde(rename = "type")]
    kind: String,
    downloads: Option<RDownloads>,
    #[serde(default)]
    executable: bool,
}

#[derive(Deserialize)]
struct RDownloads {
    raw: Option<DlInfo>,
}

#[derive(Deserialize)]
struct DlInfo {
    url: String,
    sha1: Option<String>,
    size: Option<u64>,
}

fn platform_key() -> &'static str {
    if cfg!(windows) {
        "windows-x64"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "mac-os-arm64"
        } else {
            "mac-os"
        }
    } else {
        "linux"
    }
}

fn java_exe(runtime_dir: &Path) -> PathBuf {
    if cfg!(windows) {
        runtime_dir.join("bin").join("java.exe")
    } else {
        runtime_dir.join("bin").join("java")
    }
}

/// Jednorazowo tworzy bazowe archiwum CDS Javy, jeśli środowisko go nie ma (potrzebne do przyspieszenia
/// ładowania klas przy kolejnych startach). Najlepsza próba: porażka nie przerywa uruchamiania gry.
pub async fn ensure_cds_base(java: &Path, major: u32, reporter: &Reporter) {
    if major < 19 {
        return;
    }
    let dir = match java.parent().and_then(|bin| bin.parent()) {
        Some(d) => d.to_path_buf(),
        None => return,
    };
    let marker = dir.join(".cds-checked");
    if marker.exists() {
        return;
    }
    let has_base = dir.join("bin").join("server").join("classes.jsa").exists()
        || dir.join("lib").join("server").join("classes.jsa").exists();
    if has_base {
        let _ = std::fs::write(&marker, "base-present");
        return;
    }

    download::emit(reporter, "Optymalizacja", 0, 1);
    let console_exe = if cfg!(windows) {
        dir.join("bin").join("java.exe")
    } else {
        dir.join("bin").join("java")
    };
    let exe = if console_exe.exists() {
        console_exe
    } else {
        java.to_path_buf()
    };
    let ok = tokio::task::spawn_blocking(move || {
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("-Xshare:dump")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // bez okna konsoli
        }
        cmd.status().map(|s| s.success()).unwrap_or(false)
    })
    .await
    .unwrap_or(false);
    download::emit(reporter, "Optymalizacja", 1, 1);
    let _ = std::fs::write(&marker, if ok { "dump-ok" } else { "dump-failed" });
}

/// Zwraca ścieżkę do pliku wykonywalnego Javy, pobierając środowisko, jeśli go jeszcze nie ma.
pub async fn ensure_runtime(
    client: &reqwest::Client,
    shared: &Path,
    component: &str,
    reporter: &Reporter,
) -> Result<PathBuf, String> {
    if !util::safe_token(component) {
        return Err(format!("Nieprawidłowa nazwa środowiska Javy: {component}"));
    }
    let dir = shared.join("runtime").join(component);
    let ready = dir.join(".ready");
    let exe = java_exe(&dir);
    if ready.exists() && exe.exists() {
        return Ok(exe);
    }

    let all_text = download::get_text(client, ALL_URL).await?;
    let all: HashMap<String, HashMap<String, Vec<Entry>>> = serde_json::from_str(&all_text)
        .map_err(|e| format!("Błędna lista środowisk Javy: {e}"))?;
    let entry = all
        .get(platform_key())
        .and_then(|components| components.get(component))
        .and_then(|versions| versions.first())
        .ok_or_else(|| {
            format!(
                "Mojang nie udostępnia środowiska Javy „{component}” dla tego systemu ({}).",
                platform_key()
            )
        })?;

    let manifest_text = download::get_text(client, &entry.manifest.url).await?;
    let manifest: RuntimeManifest = serde_json::from_str(&manifest_text)
        .map_err(|e| format!("Błędny manifest środowiska Javy: {e}"))?;

    let mut jobs: Vec<Job> = Vec::new();
    for (rel, f) in &manifest.files {
        if f.kind != "file" {
            continue;
        }
        let raw = match f.downloads.as_ref().and_then(|d| d.raw.as_ref()) {
            Some(r) => r,
            None => continue,
        };
        if !util::safe_rel(rel) {
            return Err(format!("Niebezpieczna ścieżka w środowisku Javy: {rel}"));
        }
        jobs.push(Job {
            url: raw.url.clone(),
            dest: dir.join(rel),
            sha1: raw.sha1.clone(),
            size: raw.size,
        });
    }

    download::download_all(client, "Java", jobs, reporter, 16).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for (rel, f) in &manifest.files {
            if f.executable {
                let p = dir.join(rel);
                if let Ok(meta) = std::fs::metadata(&p) {
                    let mut perms = meta.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&p, perms);
                }
            }
        }
    }

    std::fs::write(&ready, b"ok").map_err(|e| e.to_string())?;
    if !exe.exists() {
        return Err("Po pobraniu nie znaleziono pliku wykonywalnego Javy.".to_string());
    }
    Ok(exe)
}
