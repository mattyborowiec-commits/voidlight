//! Integracja z Modrinth: wyszukiwanie, instalacja z zależnościami, usuwanie i aktualizacje modów.

use crate::download::{self, Job};
use crate::util;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const API: &str = "https://api.modrinth.com/v2";

/// Mody poprawiające wydajność (identyfikatory projektów na Modrinth).
pub const PERF_PACK: [&str; 6] = [
    "sodium",
    "lithium",
    "ferrite-core",
    "entityculling",
    "modernfix",
    "immediatelyfast",
];

/// Rodzaj treści z Modrinth. Nazwy (małymi literami) są zarazem typami projektów w API.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Mod,
    Shader,
    ResourcePack,
    DataPack,
}

impl Kind {
    fn project_type(self) -> &'static str {
        match self {
            Kind::Mod => "mod",
            Kind::Shader => "shader",
            Kind::ResourcePack => "resourcepack",
            Kind::DataPack => "datapack",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Kind::Mod => "Fabric",
            Kind::Shader => "shadery",
            Kind::ResourcePack => "paczki zasobów",
            Kind::DataPack => "paczki danych",
        }
    }
    /// Filtr loaderów przy pytaniu o wersje projektu.
    fn loaders(self) -> Option<&'static str> {
        match self {
            Kind::Mod => Some("[\"fabric\"]"),
            Kind::DataPack => Some("[\"datapack\"]"),
            _ => None,
        }
    }
    fn extension(self) -> &'static str {
        match self {
            Kind::Mod => ".jar",
            _ => ".zip",
        }
    }
}

/// Miejsce, w którym leżą pliki danego rodzaju w instancji (i ich manifest).
pub struct Store {
    pub kind: Kind,
    pub inst_dir: PathBuf,
    /// Tylko dla paczek danych: nazwa świata z folderu `saves`.
    pub world: Option<String>,
}

impl Store {
    pub fn new(kind: Kind, inst_dir: PathBuf, world: Option<String>) -> Result<Store, String> {
        if kind == Kind::DataPack {
            let w = world
                .as_deref()
                .ok_or_else(|| "Wybierz świat, do którego dodać paczkę danych.".to_string())?;
            if !valid_world(w) || !inst_dir.join("saves").join(w).is_dir() {
                return Err("Nie znaleziono wybranego świata.".to_string());
            }
        }
        Ok(Store { kind, inst_dir, world })
    }
    pub fn content_dir(&self) -> PathBuf {
        match self.kind {
            Kind::Mod => self.inst_dir.join("mods"),
            Kind::Shader => self.inst_dir.join("shaderpacks"),
            Kind::ResourcePack => self.inst_dir.join("resourcepacks"),
            Kind::DataPack => self
                .inst_dir
                .join("saves")
                .join(self.world.as_deref().unwrap_or(""))
                .join("datapacks"),
        }
    }
    fn manifest_path(&self) -> PathBuf {
        match self.kind {
            Kind::Mod => self.inst_dir.join("voidlight-mods.json"),
            Kind::Shader => self.inst_dir.join("voidlight-shaders.json"),
            Kind::ResourcePack => self.inst_dir.join("voidlight-resourcepacks.json"),
            Kind::DataPack => {
                let w: String = self
                    .world
                    .as_deref()
                    .unwrap_or("")
                    .chars()
                    .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
                    .collect();
                self.inst_dir.join(format!("voidlight-datapacks-{w}.json"))
            }
        }
    }
}

/// Nazwa folderu świata nie może wychodzić poza `saves`.
pub fn valid_world(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name != "."
        && name != ".."
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(':')
}

/// Światy zapisane w instancji (foldery z plikiem level.dat).
pub fn list_worlds(inst_dir: &Path) -> Vec<String> {
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(inst_dir.join("saves")) {
        for e in rd.flatten() {
            if e.path().join("level.dat").exists() {
                let n = e.file_name().to_string_lossy().to_string();
                if valid_world(&n) {
                    out.push(n);
                }
            }
        }
    }
    out.sort();
    out
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModHit {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub hits: Vec<ModHit>,
    pub total_hits: u64,
}

/// Wpis w manifeście modów zainstalowanych przez launcher (plik voidlight-mods.json w folderze instancji).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMod {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub version_id: String,
    pub version_number: String,
    pub filename: String,
    /// true, gdy użytkownik zainstalował go sam; false, gdy to zależność innego moda.
    pub explicit: bool,
    /// Adres ikony projektu. Pusty napis oznacza, że projekt nie ma ikony (sprawdzone).
    #[serde(default)]
    pub icon_url: Option<String>,
    /// false, gdy plik jest wyłączony (na dysku ma dopisek .disabled i gra go pomija).
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Dostępna nowsza wersja zainstalowanej pozycji.
#[derive(Debug, Serialize)]
pub struct UpdateAvailable {
    pub project_id: String,
    pub version_number: String,
}

#[derive(Debug, Default, Serialize)]
pub struct InstallReport {
    pub installed: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
pub struct UpdateReport {
    pub updated: Vec<String>,
    pub failed: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct Dependency {
    project_id: Option<String>,
    dependency_type: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Hashes {
    sha1: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct VFile {
    url: String,
    filename: String,
    #[serde(default)]
    primary: bool,
    size: u64,
    hashes: Hashes,
}

#[derive(Debug, Clone, Deserialize)]
struct Version {
    id: String,
    version_number: String,
    version_type: String,
    files: Vec<VFile>,
    #[serde(default)]
    dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, Deserialize)]
struct Project {
    id: String,
    slug: String,
    title: String,
    icon_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ProjectIcon {
    id: String,
    icon_url: Option<String>,
}

async fn get_json<T: serde::de::DeserializeOwned>(
    client: &reqwest::Client,
    url: &str,
    query: &[(&str, String)],
) -> Result<T, String> {
    let resp = client
        .get(url)
        .query(query)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Modrinth: HTTP {}", resp.status()));
    }
    resp.json::<T>()
        .await
        .map_err(|e| format!("Błędna odpowiedź Modrinth: {e}"))
}

fn safe_filename(name: &str, kind: Kind) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains('/')
        && !name.contains('\\')
        && name.to_ascii_lowercase().ends_with(kind.extension())
}

pub fn load_manifest(store: &Store) -> Vec<InstalledMod> {
    std::fs::read_to_string(store.manifest_path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_manifest(store: &Store, manifest: &[InstalledMod]) -> Result<(), String> {
    let json = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&store.inst_dir).map_err(|e| e.to_string())?;
    std::fs::write(store.manifest_path(), json).map_err(|e| e.to_string())
}

/// Wpis dla pliku dodanego ręcznie (spoza Modrinth).
fn local_entry(filename: &str) -> InstalledMod {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or(filename)
        .to_string();
    InstalledMod {
        project_id: format!("local:{filename}"),
        slug: format!("local:{filename}"),
        title: stem,
        version_id: "local".to_string(),
        version_number: "własny plik".to_string(),
        filename: filename.to_string(),
        explicit: true,
        // Pusty napis = brak ikony (nie pytamy o nią Modrinth).
        icon_url: Some(String::new()),
        enabled: true,
    }
}

const DISABLED_SUFFIX: &str = ".disabled";

/// Ścieżka pliku na dysku: wyłączony plik ma dopisek `.disabled`.
fn disk_path(dir: &Path, filename: &str, enabled: bool) -> PathBuf {
    if enabled {
        dir.join(filename)
    } else {
        dir.join(format!("{filename}{DISABLED_SUFFIX}"))
    }
}

/// Usuwa plik w obu wariantach (włączonym i wyłączonym).
fn remove_both(dir: &Path, filename: &str) {
    let _ = std::fs::remove_file(dir.join(filename));
    let _ = std::fs::remove_file(dir.join(format!("{filename}{DISABLED_SUFFIX}")));
}

fn is_local(m: &InstalledMod) -> bool {
    m.project_id.starts_with("local:")
}

/// Zgrywa manifest z zawartością folderu: dopisuje pliki dodane ręcznie, usuwa wpisy plików, które zniknęły,
/// i odświeża stan włączony/wyłączony według tego, co faktycznie leży na dysku.
fn sync_local(store: &Store, manifest: &mut Vec<InstalledMod>) -> bool {
    let dir = store.content_dir();
    let rd = match std::fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(_) => return false,
    };
    // (nazwa bazowa, czy włączony)
    let mut on_disk: Vec<(String, bool)> = vec![];
    for e in rd.flatten() {
        if !e.path().is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        if let Some(base) = name.strip_suffix(DISABLED_SUFFIX) {
            if safe_filename(base, store.kind) {
                on_disk.push((base.to_string(), false));
            }
        } else if safe_filename(&name, store.kind) {
            on_disk.push((name, true));
        }
    }
    let before = manifest.len();
    manifest.retain(|m| on_disk.iter().any(|(n, _)| n == &m.filename));
    let mut changed = manifest.len() != before;
    for m in manifest.iter_mut() {
        if let Some((_, en)) = on_disk.iter().find(|(n, _)| n == &m.filename) {
            if m.enabled != *en {
                m.enabled = *en;
                changed = true;
            }
        }
    }
    on_disk.sort();
    for (name, en) in on_disk {
        if !manifest.iter().any(|m| m.filename == name) {
            let mut entry = local_entry(&name);
            entry.enabled = en;
            manifest.push(entry);
            changed = true;
        }
    }
    changed
}

/// Włącza albo wyłącza pozycję: zmienia nazwę pliku (dopisek .disabled) i zapisuje stan w manifeście.
pub fn set_enabled(store: &Store, project_id: &str, enabled: bool) -> Result<(), String> {
    let mut manifest = load_manifest(store);
    let m = manifest
        .iter_mut()
        .find(|m| m.project_id == project_id)
        .ok_or_else(|| "Nie znaleziono tej pozycji na liście.".to_string())?;
    if !safe_filename(&m.filename, store.kind) {
        return Err("Niebezpieczna nazwa pliku.".to_string());
    }
    if m.enabled != enabled {
        let dir = store.content_dir();
        let from = disk_path(&dir, &m.filename, m.enabled);
        let to = disk_path(&dir, &m.filename, enabled);
        if !from.exists() {
            return Err("Nie znaleziono pliku na dysku.".to_string());
        }
        std::fs::rename(&from, &to).map_err(|e| format!("Nie udało się zmienić nazwy pliku: {e}"))?;
        m.enabled = enabled;
    }
    save_manifest(store, &manifest)
}

/// Sprawdza, które zainstalowane pozycje mają nowszą wersję (własnych plików nie sprawdza).
pub async fn check_updates(store: &Store, mc: &str) -> Result<Vec<UpdateAvailable>, String> {
    let client = download::http_client()?;
    let kind = store.kind;
    let items: Vec<(String, String)> = load_manifest(store)
        .iter()
        .filter(|m| !is_local(m))
        .map(|m| (m.project_id.clone(), m.version_id.clone()))
        .collect();
    let client_ref = &client;
    let found: Vec<Option<UpdateAvailable>> = futures::stream::iter(items)
        .map(|(pid, vid)| async move {
            match best_version(client_ref, &pid, mc, kind).await {
                Ok(Some(v)) if v.id != vid => Some(UpdateAvailable {
                    project_id: pid,
                    version_number: v.version_number,
                }),
                _ => None,
            }
        })
        .buffer_unordered(6)
        .collect()
        .await;
    Ok(found.into_iter().flatten().collect())
}

/// Kopiuje wskazane pliki do folderu danego rodzaju treści (import własnych modów i paczek).
pub fn import_files(store: &Store, paths: &[String]) -> Result<InstallReport, String> {
    const MAX_SIZE: u64 = 2 * 1024 * 1024 * 1024;
    let dir = store.content_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut manifest = load_manifest(store);
    let mut report = InstallReport::default();
    for p in paths {
        let src = PathBuf::from(p);
        let name = match src.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => {
                report.failed.push(format!("{p}: nieprawidłowa ścieżka"));
                continue;
            }
        };
        let meta = match std::fs::metadata(&src) {
            Ok(m) if m.is_file() => m,
            _ => {
                report.failed.push(format!("{name}: to nie jest plik"));
                continue;
            }
        };
        if !safe_filename(&name, store.kind) {
            report.failed.push(format!(
                "{name}: oczekiwano pliku {} dla tego rodzaju zawartości",
                store.kind.extension()
            ));
            continue;
        }
        if meta.len() > MAX_SIZE {
            report.failed.push(format!("{name}: plik jest zbyt duży"));
            continue;
        }
        let dest = dir.join(&name);
        if dest.exists() || disk_path(&dir, &name, false).exists() {
            report.skipped.push(name);
            continue;
        }
        match std::fs::copy(&src, &dest) {
            Ok(_) => {
                manifest.push(local_entry(&name));
                report.installed.push(name);
            }
            Err(e) => report.failed.push(format!("{name}: {e}")),
        }
    }
    save_manifest(store, &manifest)?;
    Ok(report)
}

/// Lista zainstalowanych modów. Wpisom bez ikony (np. zainstalowanym wcześniej) dociąga ikony
/// jednym zapytaniem zbiorczym i zapisuje je w manifeście. Brak sieci nie jest błędem.
pub async fn load_with_icons(store: &Store) -> Vec<InstalledMod> {
    let mut manifest = load_manifest(store);
    if sync_local(store, &mut manifest) {
        let _ = save_manifest(store, &manifest);
    }
    let missing: Vec<String> = manifest
        .iter()
        .filter(|m| m.icon_url.is_none())
        .map(|m| m.project_id.clone())
        .filter(|id| util::safe_token(id))
        .collect();
    if missing.is_empty() {
        return manifest;
    }
    let client = match download::http_client() {
        Ok(c) => c,
        Err(_) => return manifest,
    };
    let ids = serde_json::to_string(&missing).unwrap_or_default();
    let found: Result<Vec<ProjectIcon>, String> =
        get_json(&client, &format!("{API}/projects"), &[("ids", ids)]).await;
    if let Ok(list) = found {
        let mut changed = false;
        for m in manifest.iter_mut() {
            if m.icon_url.is_some() {
                continue;
            }
            if let Some(p) = list.iter().find(|p| p.id == m.project_id) {
                m.icon_url = Some(p.icon_url.clone().unwrap_or_default());
                changed = true;
            }
        }
        if changed {
            let _ = save_manifest(store, &manifest);
        }
    }
    manifest
}

/// Wyszukiwanie treści (mody Fabric, shadery, paczki) zgodnych z daną wersją Minecrafta.
pub async fn search(
    kind: Kind,
    query: &str,
    mc: &str,
    offset: u32,
    sort: &str,
) -> Result<SearchResult, String> {
    if !util::safe_token(mc) {
        return Err(format!("Nieprawidłowa wersja: {mc}"));
    }
    let query: String = query.trim().chars().take(100).collect();
    let mut index = match sort {
        "relevance" | "downloads" | "follows" | "newest" | "updated" => sort,
        _ => "downloads",
    };
    if query.is_empty() && index == "relevance" {
        index = "downloads";
    }
    let mut facets = vec![
        serde_json::json!([format!("project_type:{}", kind.project_type())]),
        serde_json::json!([format!("versions:{mc}")]),
    ];
    if kind == Kind::Mod {
        facets.push(serde_json::json!(["categories:fabric"]));
        facets.push(serde_json::json!(["client_side:required", "client_side:optional"]));
    }
    let facets = serde_json::Value::Array(facets).to_string();

    let client = download::http_client()?;
    get_json(
        &client,
        &format!("{API}/search"),
        &[
            ("query", query),
            ("facets", facets),
            ("limit", "30".to_string()),
            ("offset", offset.to_string()),
            ("index", index.to_string()),
        ],
    )
    .await
}

/// Wyszukiwanie modpacków (Fabric) na Modrinth. Wersję gry wybiera sama paczka.
pub async fn search_modpacks(query: &str, offset: u32, sort: &str) -> Result<SearchResult, String> {
    let query: String = query.trim().chars().take(100).collect();
    let mut index = match sort {
        "relevance" | "downloads" | "follows" | "newest" | "updated" => sort,
        _ => "downloads",
    };
    if query.is_empty() && index == "relevance" {
        index = "downloads";
    }
    let facets = serde_json::json!([["project_type:modpack"], ["categories:fabric"]]).to_string();
    let client = download::http_client()?;
    get_json(
        &client,
        &format!("{API}/search"),
        &[
            ("query", query),
            ("facets", facets),
            ("limit", "30".to_string()),
            ("offset", offset.to_string()),
            ("index", index.to_string()),
        ],
    )
    .await
}

/// Najnowszy plik `.mrpack` danego modpacka (wydanie, a w razie braku beta/alfa): (adres, sha1, rozmiar).
pub async fn modpack_file(project: &str) -> Result<(String, Option<String>, u64), String> {
    if !util::safe_token(project) {
        return Err("Nieprawidłowy identyfikator modpacka.".to_string());
    }
    let client = download::http_client()?;
    let versions: Vec<Version> = get_json(
        &client,
        &format!("{API}/project/{project}/version"),
        &[("loaders", "[\"fabric\"]".to_string())],
    )
    .await?;
    for kind in ["release", "beta", "alpha"] {
        for v in versions.iter().filter(|v| v.version_type == kind) {
            let file = v
                .files
                .iter()
                .find(|f| f.primary && f.filename.ends_with(".mrpack"))
                .or_else(|| v.files.iter().find(|f| f.filename.ends_with(".mrpack")));
            if let Some(f) = file {
                if !f.url.starts_with("https://cdn.modrinth.com/") {
                    return Err("Nieoczekiwany adres pobierania modpacka.".to_string());
                }
                return Ok((f.url.clone(), f.hashes.sha1.clone(), f.size));
            }
        }
    }
    Err("Ten modpack nie ma wersji dla Fabrica.".to_string())
}

/// Najlepsza wersja moda dla danej wersji gry (Fabric): wydanie, w razie braku beta, w razie braku alfa.
async fn best_version(
    client: &reqwest::Client,
    project: &str,
    mc: &str,
    kind: Kind,
) -> Result<Option<Version>, String> {
    if !util::safe_token(project) || !util::safe_token(mc) {
        return Err("Nieprawidłowy identyfikator moda lub wersji gry.".to_string());
    }
    let url = format!("{API}/project/{project}/version");
    let mut query = vec![("game_versions", format!("[\"{mc}\"]"))];
    if let Some(l) = kind.loaders() {
        query.push(("loaders", l.to_string()));
    }
    let versions: Vec<Version> = get_json(client, &url, &query).await?;
    for kind in ["release", "beta", "alpha"] {
        if let Some(v) = versions.iter().find(|v| v.version_type == kind) {
            return Ok(Some(v.clone()));
        }
    }
    Ok(None)
}

/// Pobiera plik wskazanej wersji do folderu danego rodzaju treści i zwraca jego nazwę.
async fn fetch_version_file(
    client: &reqwest::Client,
    mods_dir: &Path,
    version: &Version,
    kind: Kind,
) -> Result<String, String> {
    let file = version
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| version.files.first())
        .ok_or_else(|| "wersja nie zawiera pliku".to_string())?;
    if !safe_filename(&file.filename, kind) {
        return Err(format!("niebezpieczna nazwa pliku: {}", file.filename));
    }
    let job = Job {
        url: file.url.clone(),
        dest: mods_dir.join(&file.filename),
        sha1: file.hashes.sha1.clone(),
        size: Some(file.size),
    };
    download::download_one(client, &job).await?;
    Ok(file.filename.clone())
}

async fn install_one(
    client: &reqwest::Client,
    mods_dir: &Path,
    mc: &str,
    project: &str,
    kind: Kind,
) -> Result<(InstalledMod, Vec<String>), String> {
    let version = best_version(client, project, mc, kind)
        .await?
        .ok_or_else(|| format!("brak wersji dla Minecrafta {mc} ({})", kind.label()))?;
    let info: Project = get_json(client, &format!("{API}/project/{project}"), &[]).await?;
    let filename = fetch_version_file(client, mods_dir, &version, kind).await?;
    // Zależności instalujemy tylko dla modów; paczki zasobów i shadery mają ich zwykle opcjonalne.
    let deps: Vec<String> = version
        .dependencies
        .iter()
        .filter(|d| kind == Kind::Mod && d.dependency_type == "required")
        .filter_map(|d| d.project_id.clone())
        .collect();
    Ok((
        InstalledMod {
            project_id: info.id,
            slug: info.slug,
            title: info.title,
            version_id: version.id,
            version_number: version.version_number,
            filename,
            explicit: false,
            icon_url: info.icon_url,
            enabled: true,
        },
        deps,
    ))
}

/// Instaluje mody razem z wymaganymi zależnościami.
pub async fn install(
    store: &Store,
    mc: &str,
    projects: &[String],
    explicit: bool,
) -> Result<InstallReport, String> {
    let client = download::http_client()?;
    let kind = store.kind;
    let mods_dir = store.content_dir();
    std::fs::create_dir_all(&mods_dir).map_err(|e| e.to_string())?;

    let mut manifest = load_manifest(store);
    let mut report = InstallReport::default();
    let mut queue: Vec<(String, bool)> = projects.iter().map(|p| (p.clone(), explicit)).collect();
    queue.reverse();
    let mut seen: HashSet<String> = HashSet::new();

    while let Some((project, is_explicit)) = queue.pop() {
        if !util::safe_token(&project) {
            report
                .failed
                .push(format!("{project}: nieprawidłowy identyfikator"));
            continue;
        }
        if !seen.insert(project.clone()) {
            continue;
        }
        if let Some(existing) = manifest
            .iter_mut()
            .find(|m| m.project_id == project || m.slug == project)
        {
            if is_explicit {
                existing.explicit = true;
            }
            report.skipped.push(existing.title.clone());
            continue;
        }
        match install_one(&client, &mods_dir, mc, &project, kind).await {
            Ok((mut entry, deps)) => {
                entry.explicit = is_explicit;
                report.installed.push(entry.title.clone());
                manifest.push(entry);
                for d in deps {
                    queue.push((d, false));
                }
            }
            Err(e) => report.failed.push(format!("{project}: {e}")),
        }
    }

    save_manifest(store, &manifest)?;
    Ok(report)
}

/// Usuwa mod (plik i wpis w manifeście). Zależności zostają, bo mogą być potrzebne innym modom.
pub fn remove(store: &Store, project_id: &str) -> Result<(), String> {
    let mut manifest = load_manifest(store);
    let idx = manifest
        .iter()
        .position(|m| m.project_id == project_id)
        .ok_or_else(|| "Nie znaleziono tego moda na liście.".to_string())?;
    let removed = manifest.remove(idx);
    if safe_filename(&removed.filename, store.kind) {
        remove_both(&store.content_dir(), &removed.filename);
    }
    save_manifest(store, &manifest)
}

/// Aktualizuje wszystkie mody z manifestu do najnowszych zgodnych wersji.
pub async fn update_all(
    store: &Store,
    mc: &str,
    only: Option<&str>,
) -> Result<UpdateReport, String> {
    let client = download::http_client()?;
    let kind = store.kind;
    let mods_dir = store.content_dir();
    let mut manifest = load_manifest(store);
    let mut report = UpdateReport::default();

    for m in manifest.iter_mut() {
        if is_local(m) {
            continue; // własne pliki nie mają źródła, z którego dałoby się je zaktualizować
        }
        if let Some(id) = only {
            if m.project_id != id {
                continue;
            }
        }
        let latest = match best_version(&client, &m.project_id, mc, kind).await {
            Ok(Some(v)) => v,
            Ok(None) => {
                report
                    .failed
                    .push(format!("{}: brak wersji dla {mc}", m.title));
                continue;
            }
            Err(e) => {
                report.failed.push(format!("{}: {e}", m.title));
                continue;
            }
        };
        if latest.id == m.version_id {
            continue;
        }
        match fetch_version_file(&client, &mods_dir, &latest, kind).await {
            Ok(filename) => {
                if filename != m.filename && safe_filename(&m.filename, kind) {
                    remove_both(&mods_dir, &m.filename);
                }
                // Wyłączona pozycja zostaje wyłączona także po aktualizacji.
                if !m.enabled {
                    let _ = std::fs::rename(
                        mods_dir.join(&filename),
                        disk_path(&mods_dir, &filename, false),
                    );
                }
                m.version_id = latest.id.clone();
                m.version_number = latest.version_number.clone();
                m.filename = filename;
                report.updated.push(m.title.clone());
            }
            Err(e) => report.failed.push(format!("{}: {e}", m.title)),
        }
    }

    save_manifest(store, &manifest)?;
    Ok(report)
}
