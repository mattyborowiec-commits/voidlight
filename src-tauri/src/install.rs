//! Przygotowanie gry: JSON wersji, plik klienta, biblioteki, zasoby i Java.
//! Wszystko trafia do wspólnego folderu `shared`, więc kolejne instancje i starty niczego nie pobierają ponownie.

use crate::download::{self, Job, Reporter};
use crate::meta::{self, VersionJson};
use crate::{fabric, java, mojang, util};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Target {
    pub mc_version: String,
    pub fabric: bool,
    pub loader_version: Option<String>,
}

pub struct Ready {
    pub version: VersionJson,
    pub loader_version: Option<String>,
    pub client_jar: PathBuf,
    pub libraries: Vec<PathBuf>,
    pub assets_root: PathBuf,
    pub java: PathBuf,
}

#[derive(Deserialize)]
struct AssetIndex {
    objects: HashMap<String, AssetObject>,
}

#[derive(Deserialize)]
struct AssetObject {
    hash: String,
    size: u64,
}

fn parse(text: &str) -> Result<VersionJson, String> {
    serde_json::from_str(text).map_err(|e| format!("Błędny JSON wersji: {e}"))
}

fn read_valid_json(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<VersionJson>(&text).ok()?;
    Some(text)
}

fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, text).map_err(|e| e.to_string())
}

async fn vanilla_json(
    client: &reqwest::Client,
    shared: &Path,
    mc: &str,
) -> Result<String, String> {
    if !util::safe_token(mc) {
        return Err(format!("Nieprawidłowa wersja: {mc}"));
    }
    let path = shared.join("versions").join(mc).join(format!("{mc}.json"));
    if let Some(text) = read_valid_json(&path) {
        return Ok(text);
    }
    let entry = mojang::find_version(mc).await?;
    let text = download::get_text(client, &entry.url).await?;
    if !download::sha1_hex(text.as_bytes()).eq_ignore_ascii_case(&entry.sha1) {
        return Err("Niezgodna suma kontrolna JSON-a wersji.".to_string());
    }
    write_file(&path, &text)?;
    Ok(text)
}

async fn fabric_json(
    client: &reqwest::Client,
    shared: &Path,
    mc: &str,
    loader: &str,
) -> Result<String, String> {
    if !util::safe_token(loader) {
        return Err(format!("Nieprawidłowa wersja loadera: {loader}"));
    }
    let id = format!("fabric-loader-{loader}-{mc}");
    let path = shared.join("versions").join(&id).join(format!("{id}.json"));
    if let Some(text) = read_valid_json(&path) {
        return Ok(text);
    }
    let text = fabric::profile_json(client, mc, loader).await?;
    write_file(&path, &text)?;
    Ok(text)
}

/// Pobiera brakujące pliki i zwraca wszystko, co potrzebne do startu gry.
/// Gdy wszystko jest już na dysku, sprawdza tylko istnienie i rozmiar plików (bez sieci i bez liczenia sum).
pub async fn prepare(
    shared: &Path,
    target: &Target,
    reporter: &Reporter,
) -> Result<Ready, String> {
    let client = download::http_client()?;
    let mc = target.mc_version.as_str();

    download::emit(reporter, "Wersja", 0, 1);
    let vanilla = parse(&vanilla_json(&client, shared, mc).await?)?;

    let (version, loader_version) = if target.fabric {
        let loader = match &target.loader_version {
            Some(v) => v.clone(),
            None => fabric::latest_loader(&client, mc).await?,
        };
        let child = parse(&fabric_json(&client, shared, mc, &loader).await?)?;
        (meta::merge(vanilla, child), Some(loader))
    } else {
        (vanilla, None)
    };

    if let Some(reason) = meta::unsupported_reason(&version) {
        return Err(reason);
    }

    // 1. Plik klienta
    let dl = version
        .downloads
        .as_ref()
        .and_then(|d| d.client.as_ref())
        .ok_or_else(|| "JSON wersji nie zawiera pliku klienta.".to_string())?;
    let client_jar = shared.join("versions").join(mc).join(format!("{mc}.jar"));
    let jobs = vec![Job {
        url: dl.url.clone(),
        dest: client_jar.clone(),
        sha1: dl.sha1.clone(),
        size: dl.size,
    }];
    download::download_all(&client, "Wersja", jobs, reporter, 2).await?;

    // 2. Biblioteki
    let lib_dir = shared.join("libraries");
    let libs = meta::library_files(&version)?;
    let jobs: Vec<Job> = libs
        .iter()
        .map(|l| Job {
            url: l.url.clone(),
            dest: lib_dir.join(&l.rel_path),
            sha1: l.sha1.clone(),
            size: l.size,
        })
        .collect();
    download::download_all(&client, "Biblioteki", jobs, reporter, 16).await?;
    let libraries: Vec<PathBuf> = libs.iter().map(|l| lib_dir.join(&l.rel_path)).collect();

    // 3. Zasoby (dźwięki, języki, ikony)
    let asset_ref = version
        .asset_index
        .clone()
        .ok_or_else(|| "JSON wersji nie zawiera indeksu zasobów.".to_string())?;
    if !util::safe_token(&asset_ref.id) {
        return Err(format!("Nieprawidłowy identyfikator zasobów: {}", asset_ref.id));
    }
    let assets_root = shared.join("assets");
    let index_path = assets_root
        .join("indexes")
        .join(format!("{}.json", asset_ref.id));
    let marker = assets_root.join(format!(".verified-{}", asset_ref.id));
    if !(marker.exists() && index_path.exists()) {
        let jobs = vec![Job {
            url: asset_ref.url.clone(),
            dest: index_path.clone(),
            sha1: asset_ref.sha1.clone(),
            size: asset_ref.size,
        }];
        download::download_all(&client, "Zasoby", jobs, reporter, 1).await?;

        let index_text = std::fs::read_to_string(&index_path).map_err(|e| e.to_string())?;
        let index: AssetIndex = serde_json::from_str(&index_text)
            .map_err(|e| format!("Błędny indeks zasobów: {e}"))?;
        let jobs: Vec<Job> = index
            .objects
            .values()
            .filter(|o| o.hash.len() >= 2 && o.hash.chars().all(|c| c.is_ascii_hexdigit()))
            .map(|o| {
                let prefix = &o.hash[..2];
                Job {
                    url: format!("https://resources.download.minecraft.net/{prefix}/{}", o.hash),
                    dest: assets_root.join("objects").join(prefix).join(&o.hash),
                    sha1: Some(o.hash.clone()),
                    size: Some(o.size),
                }
            })
            .collect();
        download::download_all(&client, "Zasoby", jobs, reporter, 32).await?;
        let _ = std::fs::write(&marker, "ok");
    }

    // 4. Java
    let component = version
        .java_version
        .as_ref()
        .map(|j| j.component.clone())
        .unwrap_or_else(|| "jre-legacy".to_string());
    let java = java::ensure_runtime(&client, shared, &component, reporter).await?;
    let java_major = version.java_version.as_ref().map(|j| j.major_version).unwrap_or(8);
    java::ensure_cds_base(&java, java_major, reporter).await;

    Ok(Ready {
        version,
        loader_version,
        client_jar,
        libraries,
        assets_root,
        java,
    })
}
