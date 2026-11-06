//! Import paczek modów w formacie `.mrpack` (Modrinth): nowa instancja + pobranie plików + `overrides`.

use crate::download::{self, Job, Reporter};
use crate::instance::{Instance, Loader};
use crate::jvm::JvmProfile;
use crate::util;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;

const ALLOWED_HOSTS: [&str; 4] = [
    "cdn.modrinth.com",
    "github.com",
    "raw.githubusercontent.com",
    "gitlab.com",
];
const MAX_INDEX: u64 = 8 * 1024 * 1024;
const MAX_TOTAL_OVERRIDES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_ENTRIES: usize = 50_000;

#[derive(Deserialize)]
struct Index {
    #[serde(rename = "formatVersion")]
    format_version: u32,
    game: String,
    name: String,
    #[serde(default)]
    files: Vec<PackFile>,
    dependencies: HashMap<String, String>,
}

#[derive(Deserialize)]
struct PackFile {
    path: String,
    #[serde(default)]
    hashes: HashMap<String, String>,
    #[serde(default)]
    env: Option<Env>,
    #[serde(default)]
    downloads: Vec<String>,
    #[serde(rename = "fileSize", default)]
    file_size: Option<u64>,
}

#[derive(Deserialize)]
struct Env {
    #[serde(default)]
    client: Option<String>,
}

#[derive(Serialize)]
pub struct ImportReport {
    pub instance: Instance,
    pub files: usize,
    pub optional: usize,
    pub skipped: usize,
}

/// Nazwa instancji z nazwy paczki: tylko dozwolone znaki, do 40 znaków, unikalna.
fn make_name(root: &Path, pack: &str) -> String {
    let cleaned: String = pack
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut base: String = cleaned.chars().take(36).collect();
    base = base.trim().to_string();
    if base.is_empty() {
        base = "Modpack".to_string();
    }
    let dir = root.join("instances");
    let mut name = base.clone();
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("{base} {n}");
        n += 1;
    }
    name
}

/// Ścieżka z paczki musi być względna, bez `..`, bez znaków specjalnych i zagnieżdżona w folderze.
fn safe_pack_path(p: &str) -> bool {
    !p.contains('\\')
        && !p.contains(':')
        && p.contains('/')
        && util::safe_rel(p)
}

fn host_allowed(url: &str) -> bool {
    let rest = match url.strip_prefix("https://") {
        Some(r) => r,
        None => return false,
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    !host.contains('@') && ALLOWED_HOSTS.contains(&host)
}

fn read_index(path: &Path) -> Result<Index, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("Nie można otworzyć pliku: {e}"))?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|_| "To nie jest poprawny plik .mrpack (archiwum ZIP).".to_string())?;
    let entry = zip
        .by_name("modrinth.index.json")
        .map_err(|_| "W paczce brakuje pliku modrinth.index.json.".to_string())?;
    let mut text = String::new();
    entry
        .take(MAX_INDEX)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| format!("Błędny modrinth.index.json: {e}"))
}

/// Wypakowuje `overrides/` i `client-overrides/` (w tej kolejności) do folderu instancji.
fn extract_overrides(pack: &Path, inst_dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(pack).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    if zip.len() > MAX_ENTRIES {
        return Err("Paczka zawiera zbyt wiele plików.".to_string());
    }
    let mut total: u64 = 0;
    for prefix in ["overrides/", "client-overrides/"] {
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
            let name = entry.name().to_string();
            let rel = match name.strip_prefix(prefix) {
                Some(r) if !r.is_empty() => r.to_string(),
                _ => continue,
            };
            if entry.is_dir() || rel.ends_with('/') {
                continue;
            }
            if rel.contains('\\') || rel.contains(':') || !util::safe_rel(&rel) {
                return Err(format!("Niebezpieczna ścieżka w paczce: {name}"));
            }
            // Pliki w głównym folderze (np. options.txt) są dozwolone, ale nie mogą nadpisać danych launchera.
            if matches!(rel.as_str(), "instance.json" | "launcher.log") {
                continue;
            }
            let dest = inst_dir.join(&rel);
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut out = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
            let remaining = MAX_TOTAL_OVERRIDES.saturating_sub(total);
            let written = std::io::copy(&mut (&mut entry).take(remaining + 1), &mut out)
                .map_err(|e| e.to_string())?;
            out.flush().map_err(|e| e.to_string())?;
            total += written;
            if total > MAX_TOTAL_OVERRIDES {
                return Err("Paczka jest zbyt duża po rozpakowaniu.".to_string());
            }
        }
    }
    Ok(())
}

pub async fn import(
    root: &Path,
    pack: &Path,
    reporter: &Reporter,
) -> Result<ImportReport, String> {
    let ext_ok = pack
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("mrpack"))
        .unwrap_or(false);
    if !ext_ok {
        return Err("Wybierz plik z rozszerzeniem .mrpack.".to_string());
    }

    let pack_path = pack.to_path_buf();
    let index = {
        let p = pack_path.clone();
        tokio::task::spawn_blocking(move || read_index(&p))
            .await
            .map_err(|e| e.to_string())??
    };

    if index.format_version != 1 || index.game != "minecraft" {
        return Err("Nieobsługiwany format paczki (wymagany Minecraft, formatVersion 1).".to_string());
    }
    let mc = index
        .dependencies
        .get("minecraft")
        .cloned()
        .ok_or_else(|| "Paczka nie podaje wersji Minecrafta.".to_string())?;
    if !util::safe_token(&mc) {
        return Err(format!("Nieprawidłowa wersja Minecrafta: {mc}"));
    }
    for other in ["forge", "neoforge", "quilt-loader"] {
        if index.dependencies.contains_key(other) {
            return Err(format!(
                "Ta paczka wymaga loadera „{other}”, a launcher obsługuje na razie tylko Fabric i Vanilla."
            ));
        }
    }
    let (loader, loader_version) = match index.dependencies.get("fabric-loader") {
        Some(v) if util::safe_token(v) => (Loader::Fabric, Some(v.clone())),
        Some(v) => return Err(format!("Nieprawidłowa wersja Fabric Loadera: {v}")),
        None => (Loader::Vanilla, None),
    };

    // Wybór plików: pomijamy te, które nie są potrzebne na kliencie.
    let mut jobs: Vec<(String, Job)> = vec![];
    let mut skipped = 0usize;
    let mut optional = 0usize;
    let name = make_name(root, &index.name);
    let inst_dir = root.join("instances").join(&name);
    for f in &index.files {
        let client_env = f.env.as_ref().and_then(|e| e.client.as_deref()).unwrap_or("required");
        if client_env == "unsupported" {
            skipped += 1;
            continue;
        }
        if client_env == "optional" {
            optional += 1;
        }
        if !safe_pack_path(&f.path) {
            return Err(format!("Niebezpieczna ścieżka pliku w paczce: {}", f.path));
        }
        let url = f
            .downloads
            .iter()
            .find(|u| host_allowed(u))
            .ok_or_else(|| format!("Brak dozwolonego źródła pobierania dla {}.", f.path))?;
        let sha1 = f
            .hashes
            .get("sha1")
            .filter(|h| h.len() == 40 && h.chars().all(|c| c.is_ascii_hexdigit()))
            .cloned()
            .ok_or_else(|| format!("Brak sumy SHA-1 dla {}.", f.path))?;
        jobs.push((
            f.path.clone(),
            Job {
                url: url.clone(),
                dest: inst_dir.join(&f.path),
                sha1: Some(sha1),
                size: f.file_size,
            },
        ));
    }

    let instance = Instance {
        name: name.clone(),
        mc_version: mc,
        loader,
        loader_version,
        ram_mb: 4096,
        jvm_profile: JvmProfile::Balanced,
    };

    let result = async {
        instance.save(root).map_err(|e| e.to_string())?;
        let client = download::http_client()?;
        let list: Vec<Job> = jobs.iter().map(|(_, j)| j.clone()).collect();
        download::download_all(&client, "Modpack", list, reporter, 8).await?;
        download::emit(reporter, "Pliki paczki", 0, 1);
        let (p, d) = (pack_path.clone(), inst_dir.clone());
        tokio::task::spawn_blocking(move || extract_overrides(&p, &d))
            .await
            .map_err(|e| e.to_string())??;
        download::emit(reporter, "Pliki paczki", 1, 1);
        Ok::<(), String>(())
    }
    .await;

    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&inst_dir);
        return Err(e);
    }

    Ok(ImportReport {
        instance,
        files: jobs.len(),
        optional,
        skipped,
    })
}
