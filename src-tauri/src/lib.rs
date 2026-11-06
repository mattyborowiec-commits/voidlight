mod auth;
mod download;
mod fabric;
mod install;
mod instance;
mod java;
mod jvm;
mod launch;
mod meta;
mod modrinth;
mod mrpack;
mod mojang;
mod util;

use instance::{list_instances, Instance, Loader};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

#[derive(Serialize)]
struct SystemInfo {
    total_ram_mb: u64,
    recommended_modded_mb: u64,
    recommended_vanilla_mb: u64,
}

#[derive(Clone, Serialize)]
struct GameExit {
    name: String,
    code: Option<i32>,
}

#[derive(Clone, Serialize)]
struct GameStarted {
    name: String,
    prep_ms: u64,
}

/// Nazwy instancji, które właśnie się przygotowują albo działają (żeby nie uruchomić dwa razy).
#[derive(Default)]
struct Running(Mutex<HashSet<String>>);

/// Flaga anulowania trwającego logowania Microsoft.
#[derive(Default)]
struct AuthState {
    cancel: AtomicBool,
}

fn data_root(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// Nazwa instancji trafia do nazwy folderu, więc dopuszczamy tylko bezpieczne znaki.
fn valid_name(name: &str) -> bool {
    let len = name.chars().count();
    (1..=40).contains(&len)
        && name == name.trim()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_')
}

#[tauri::command]
async fn list_releases() -> Result<Vec<mojang::VersionEntry>, String> {
    mojang::fetch_releases().await
}

#[tauri::command]
fn system_info() -> SystemInfo {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total_mb = sys.total_memory() / 1024 / 1024;
    SystemInfo {
        total_ram_mb: total_mb,
        recommended_modded_mb: jvm::recommended_ram_mb(total_mb, true),
        recommended_vanilla_mb: jvm::recommended_ram_mb(total_mb, false),
    }
}

#[tauri::command]
fn jvm_args(profile: jvm::JvmProfile, ram_mb: u64) -> Vec<String> {
    jvm::build_args(profile, ram_mb)
}

#[tauri::command]
fn get_instances(app: tauri::AppHandle) -> Result<Vec<Instance>, String> {
    list_instances(&data_root(&app)?).map_err(|e| e.to_string())
}

#[tauri::command]
fn create_instance(app: tauri::AppHandle, instance: Instance) -> Result<Instance, String> {
    if !valid_name(&instance.name) {
        return Err("Nieprawidłowa nazwa instancji (litery, cyfry, spacja, - i _).".into());
    }
    if !(1024..=65536).contains(&instance.ram_mb) {
        return Err("Nieprawidłowa ilość pamięci RAM.".into());
    }
    let root = data_root(&app)?;
    if instance.dir(&root).exists() {
        return Err("Instancja o tej nazwie już istnieje.".into());
    }
    instance.save(&root).map_err(|e| e.to_string())?;
    Ok(instance)
}

async fn do_launch(
    app: &tauri::AppHandle,
    root: &Path,
    inst: &mut Instance,
    player: &str,
) -> Result<(), String> {
    let fabric = match inst.loader {
        Loader::Vanilla => false,
        Loader::Fabric => true,
        _ => return Err("Ten loader nie jest jeszcze obsługiwany (na razie Vanilla i Fabric).".into()),
    };

    let reporter: download::Reporter = {
        let app = app.clone();
        Arc::new(move |p: download::Progress| {
            let _ = app.emit("launch-progress", p);
        })
    };

    let shared = root.join("shared");
    let target = install::Target {
        mc_version: inst.mc_version.clone(),
        fabric,
        loader_version: inst.loader_version.clone(),
    };
    let t0 = std::time::Instant::now();
    // Logowanie Microsoft odświeżamy równolegle z przygotowaniem plików gry.
    let (ready, session) = futures::join!(
        install::prepare(&shared, &target, &reporter),
        auth::session(root)
    );
    let ready = ready?;
    let prep_ms = t0.elapsed().as_millis();
    let session = match session {
        Ok(Some(s)) => s,
        other => {
            if let Err(e) = other {
                let _ = app.emit("auth-warning", e);
            }
            auth::Session {
                name: player.to_string(),
                uuid: util::offline_uuid(player),
                access_token: "0".to_string(),
                user_type: "legacy".to_string(),
            }
        }
    };

    // Zapamiętaj wybraną wersję Fabric Loader, żeby kolejne starty nie potrzebowały sieci.
    if fabric && inst.loader_version.is_none() {
        inst.loader_version = ready.loader_version.clone();
        inst.save(root).map_err(|e| e.to_string())?;
    }

    download::emit(&reporter, "Start", 0, 1);
    let instance_dir = inst.dir(root);
    std::fs::create_dir_all(instance_dir.join("mods")).map_err(|e| e.to_string())?;

    // Nie przydzielamy więcej, niż system ma teraz wolne (inaczej JVM nie może wystartować).
    let mut ram_mb = inst.ram_mb;
    {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let free_mb = sys.available_memory() / 1024 / 1024;
        let cap = (free_mb * 8 / 10).max(2048);
        if ram_mb > cap {
            ram_mb = cap / 256 * 256;
        }
    }
    let profile_args = jvm::build_args(inst.jvm_profile, ram_mb);
    let mut cmd = launch::build_command(&ready, &shared, &instance_dir, &session, profile_args, prep_ms)?;
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Nie udało się uruchomić Javy: {e}"))?;

    let name = inst.name.clone();
    let log_path = instance_dir.join("launcher.log");
    let _ = app.emit(
        "game-started",
        GameStarted {
            name: name.clone(),
            prep_ms: prep_ms as u64,
        },
    );

    let app2 = app.clone();
    std::thread::spawn(move || {
        let code = child.wait().ok().and_then(|s| s.code());
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&log_path) {
            use std::io::Write;
            let _ = writeln!(f, "\n[Voidlight] gra zakończyła działanie, kod wyjścia: {:?}", code);
        }
        if let Ok(mut running) = app2.state::<Running>().0.lock() {
            running.remove(&name);
        }
        let _ = app2.emit("game-exit", GameExit { name, code });
    });

    Ok(())
}

#[tauri::command]
async fn launch_instance(
    app: tauri::AppHandle,
    state: tauri::State<'_, Running>,
    name: String,
    player: String,
) -> Result<(), String> {
    if !valid_name(&name) {
        return Err("Nieprawidłowa nazwa instancji.".into());
    }
    let root = data_root(&app)?;
    // Zalogowane konto Microsoft ma własną nazwę gracza, więc nick offline sprawdzamy tylko bez niego.
    if !auth::status(&root).signed_in && !util::valid_player_name(&player) {
        return Err("Nick musi mieć 3-16 znaków: litery, cyfry lub podkreślnik.".into());
    }
    let mut inst =
        Instance::load(&root, &name).map_err(|_| "Nie znaleziono instancji.".to_string())?;

    {
        let mut running = state
            .0
            .lock()
            .map_err(|_| "Błąd wewnętrzny launchera.".to_string())?;
        if !running.insert(name.clone()) {
            return Err("Ta instancja już się uruchamia albo działa.".into());
        }
    }

    let result = do_launch(&app, &root, &mut inst, &player).await;
    if result.is_err() {
        if let Ok(mut running) = state.0.lock() {
            running.remove(&name);
        }
    }
    result
}

/// Miejsce na pliki wybranego rodzaju treści w instancji (mody tylko dla Fabric).
fn content_target(
    app: &tauri::AppHandle,
    name: &str,
    kind: modrinth::Kind,
    world: Option<String>,
) -> Result<(modrinth::Store, Instance), String> {
    if !valid_name(name) {
        return Err("Nieprawidłowa nazwa instancji.".into());
    }
    let root = data_root(app)?;
    let inst =
        Instance::load(&root, name).map_err(|_| "Nie znaleziono instancji.".to_string())?;
    if kind == modrinth::Kind::Mod && inst.loader != Loader::Fabric {
        return Err("Mody są obsługiwane tylko w instancjach Fabric.".into());
    }
    let store = modrinth::Store::new(kind, inst.dir(&root), world)?;
    Ok((store, inst))
}

fn ensure_not_running(state: &Running, name: &str) -> Result<(), String> {
    let running = state
        .0
        .lock()
        .map_err(|_| "Błąd wewnętrzny launchera.".to_string())?;
    if running.contains(name) {
        Err("Zamknij grę przed zmianą zawartości instancji.".into())
    } else {
        Ok(())
    }
}

/// Zmienia nazwę, pamięć RAM i profil wydajności istniejącej instancji.
#[tauri::command]
fn update_instance(
    app: tauri::AppHandle,
    state: tauri::State<'_, Running>,
    name: String,
    new_name: String,
    ram_mb: u64,
    jvm_profile: jvm::JvmProfile,
) -> Result<Instance, String> {
    if !valid_name(&name) || !valid_name(&new_name) {
        return Err("Nieprawidłowa nazwa instancji (litery, cyfry, spacja, - i _).".into());
    }
    if !(1024..=65536).contains(&ram_mb) {
        return Err("Nieprawidłowa ilość pamięci RAM.".into());
    }
    ensure_not_running(&state, &name)?;
    let root = data_root(&app)?;
    let mut inst =
        Instance::load(&root, &name).map_err(|_| "Nie znaleziono instancji.".to_string())?;
    if new_name != name {
        let target = root.join("instances").join(&new_name);
        if target.exists() && !new_name.eq_ignore_ascii_case(&name) {
            return Err("Instancja o tej nazwie już istnieje.".into());
        }
        std::fs::rename(inst.dir(&root), &target)
            .map_err(|e| format!("Nie udało się zmienić nazwy folderu: {e}"))?;
        inst.name = new_name;
    }
    inst.ram_mb = ram_mb;
    inst.jvm_profile = jvm_profile;
    inst.save(&root).map_err(|e| e.to_string())?;
    Ok(inst)
}

/// Kopiuje folder rekurencyjnie, pomijając wskazane pozycje na najwyższym poziomie (dowiązania są pomijane).
fn copy_dir(src: &Path, dst: &Path, skip: &[&str]) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_name = entry.file_name();
        let label = file_name.to_string_lossy().to_string();
        if skip.contains(&label.as_str()) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&file_name);
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_dir(&from, &to, &[])?;
        } else if kind.is_file() {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[tauri::command]
async fn search_modpacks(
    query: String,
    offset: u32,
    sort: String,
) -> Result<modrinth::SearchResult, String> {
    modrinth::search_modpacks(&query, offset, &sort).await
}

/// Pobiera modpack z Modrinth i tworzy z niego nową instancję.
#[tauri::command]
async fn install_modpack(
    app: tauri::AppHandle,
    project_id: String,
) -> Result<mrpack::ImportReport, String> {
    let root = data_root(&app)?;
    let reporter: download::Reporter = {
        let app = app.clone();
        Arc::new(move |p: download::Progress| {
            let _ = app.emit("launch-progress", p);
        })
    };
    download::emit(&reporter, "Pobieranie paczki", 0, 1);
    let (url, sha1, size) = modrinth::modpack_file(&project_id).await?;
    let tmp_dir = root.join("tmp");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| e.to_string())?;
    let file = tmp_dir.join(format!("{project_id}.mrpack"));
    let client = download::http_client()?;
    let job = download::Job {
        url,
        dest: file.clone(),
        sha1,
        size: Some(size),
    };
    let result = match download::download_one(&client, &job).await {
        Ok(()) => mrpack::import(&root, &file, &reporter).await,
        Err(e) => Err(e),
    };
    let _ = std::fs::remove_file(&file);
    result
}

/// Importuje paczkę `.mrpack` jako nową instancję.
#[tauri::command]
async fn import_mrpack(
    app: tauri::AppHandle,
    path: String,
) -> Result<mrpack::ImportReport, String> {
    let root = data_root(&app)?;
    let reporter: download::Reporter = {
        let app = app.clone();
        Arc::new(move |p: download::Progress| {
            let _ = app.emit("launch-progress", p);
        })
    };
    mrpack::import(&root, Path::new(&path), &reporter).await
}

/// Tworzy kopię instancji (z własnymi modami i ustawieniami, opcjonalnie ze światami).
#[tauri::command]
async fn duplicate_instance(
    app: tauri::AppHandle,
    name: String,
    new_name: String,
    with_worlds: bool,
) -> Result<Instance, String> {
    if !valid_name(&name) || !valid_name(&new_name) {
        return Err("Nieprawidłowa nazwa instancji (litery, cyfry, spacja, - i _).".into());
    }
    let root = data_root(&app)?;
    let inst = Instance::load(&root, &name).map_err(|_| "Nie znaleziono instancji.".to_string())?;
    let target = root.join("instances").join(&new_name);
    if target.exists() {
        return Err("Instancja o tej nazwie już istnieje.".into());
    }
    let src = inst.dir(&root);
    let dst = target.clone();
    let result = tokio::task::spawn_blocking(move || {
        let mut skip = vec!["natives", "cds", "logs", "launcher.log"];
        if !with_worlds {
            skip.push("saves");
        }
        copy_dir(&src, &dst, &skip)
    })
    .await
    .map_err(|e| e.to_string())?;
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&target);
        return Err(format!("Nie udało się skopiować instancji: {e}"));
    }
    let mut copy = inst;
    copy.name = new_name;
    copy.save(&root).map_err(|e| e.to_string())?;
    Ok(copy)
}

/// Otwiera w Eksploratorze główny folder instancji.
#[tauri::command]
fn open_instance_folder(app: tauri::AppHandle, name: String) -> Result<(), String> {
    if !valid_name(&name) {
        return Err("Nieprawidłowa nazwa instancji.".into());
    }
    let root = data_root(&app)?;
    let inst = Instance::load(&root, &name).map_err(|_| "Nie znaleziono instancji.".to_string())?;
    let dir = inst.dir(&root);
    #[cfg(windows)]
    let result = std::process::Command::new("explorer").arg(&dir).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&dir).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(&dir).spawn();
    result.map(|_| ()).map_err(|e| format!("Nie udało się otworzyć folderu: {e}"))
}

/// Usuwa instancję razem z jej folderem (mody, światy, logi). Wspólne pliki gry zostają.
#[tauri::command]
fn delete_instance(
    app: tauri::AppHandle,
    state: tauri::State<'_, Running>,
    name: String,
) -> Result<(), String> {
    if !valid_name(&name) {
        return Err("Nieprawidłowa nazwa instancji.".into());
    }
    {
        let running = state
            .0
            .lock()
            .map_err(|_| "Błąd wewnętrzny launchera.".to_string())?;
        if running.contains(&name) {
            return Err("Zamknij grę przed usunięciem instancji.".into());
        }
    }
    let root = data_root(&app)?;
    let inst = Instance::load(&root, &name).map_err(|_| "Nie znaleziono instancji.".to_string())?;
    let dir = inst.dir(&root);
    if !dir.starts_with(root.join("instances")) || !dir.join("instance.json").exists() {
        return Err("Nieprawidłowy folder instancji.".into());
    }
    std::fs::remove_dir_all(&dir).map_err(|e| format!("Nie udało się usunąć instancji: {e}"))
}

/// Usuwa znaczniki weryfikacji zasobów: następny start sprawdzi wszystkie pliki od nowa.
#[tauri::command]
fn reset_verification(app: tauri::AppHandle) -> Result<(), String> {
    let dir = data_root(&app)?.join("shared").join("assets");
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with(".verified-") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn auth_status(app: tauri::AppHandle) -> Result<auth::Status, String> {
    Ok(auth::status(&data_root(&app)?))
}

#[tauri::command]
fn auth_set_client_id(app: tauri::AppHandle, client_id: String) -> Result<auth::Status, String> {
    let root = data_root(&app)?;
    auth::set_client_id(&root, &client_id)?;
    Ok(auth::status(&root))
}

/// Logowanie: wysyła do interfejsu kod (zdarzenie `auth-code`) i czeka na zatwierdzenie w przeglądarce.
#[tauri::command]
async fn auth_login(
    app: tauri::AppHandle,
    state: tauri::State<'_, AuthState>,
) -> Result<auth::Status, String> {
    let root = data_root(&app)?;
    state.cancel.store(false, Ordering::SeqCst);
    let (code, device_code, interval) = auth::begin(&root).await?;
    let expires = code.expires_in;
    let _ = app.emit("auth-code", code);
    auth::finish(&root, &device_code, interval, expires, || {
        state.cancel.load(Ordering::SeqCst)
    })
    .await
}

#[tauri::command]
fn auth_cancel(state: tauri::State<'_, AuthState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

#[tauri::command]
fn auth_logout(app: tauri::AppHandle) -> Result<auth::Status, String> {
    let root = data_root(&app)?;
    auth::logout(&root);
    Ok(auth::status(&root))
}

/// Otwiera w przeglądarce tylko wybrane, zaufane adresy (logowanie Microsoft i instrukcje).
#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    const ALLOWED: [&str; 5] = [
        "https://www.microsoft.com/link",
        "https://login.microsoftonline.com/",
        "https://login.live.com/",
        "https://portal.azure.com",
        "https://aka.ms/",
    ];
    if !ALLOWED.iter().any(|p| url.starts_with(p)) || url.chars().any(|c| c.is_whitespace() || c == '"') {
        return Err("Ten adres nie może zostać otwarty z launchera.".into());
    }
    #[cfg(windows)]
    let result = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", &url])
        .spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(&url).spawn();
    result.map(|_| ()).map_err(|e| format!("Nie udało się otworzyć przeglądarki: {e}"))
}

#[tauri::command]
async fn search_mods(
    kind: modrinth::Kind,
    query: String,
    mc_version: String,
    offset: u32,
    sort: String,
) -> Result<modrinth::SearchResult, String> {
    modrinth::search(kind, &query, &mc_version, offset, &sort).await
}

#[tauri::command]
fn list_worlds(app: tauri::AppHandle, name: String) -> Result<Vec<String>, String> {
    if !valid_name(&name) {
        return Err("Nieprawidłowa nazwa instancji.".into());
    }
    let root = data_root(&app)?;
    let inst = Instance::load(&root, &name).map_err(|_| "Nie znaleziono instancji.".to_string())?;
    Ok(modrinth::list_worlds(&inst.dir(&root)))
}

#[tauri::command]
async fn list_mods(
    app: tauri::AppHandle,
    name: String,
    kind: modrinth::Kind,
    world: Option<String>,
) -> Result<Vec<modrinth::InstalledMod>, String> {
    let (store, _) = content_target(&app, &name, kind, world)?;
    Ok(modrinth::load_with_icons(&store).await)
}

#[tauri::command]
async fn install_mod(
    app: tauri::AppHandle,
    state: tauri::State<'_, Running>,
    name: String,
    project: String,
    kind: modrinth::Kind,
    world: Option<String>,
) -> Result<modrinth::InstallReport, String> {
    let (store, inst) = content_target(&app, &name, kind, world)?;
    ensure_not_running(&state, &name)?;
    modrinth::install(&store, &inst.mc_version, &[project], true).await
}

/// Kopiuje wybrane przez użytkownika pliki (np. przeciągnięte na okno) do folderu instancji.
#[tauri::command]
fn import_files(
    app: tauri::AppHandle,
    state: tauri::State<'_, Running>,
    name: String,
    kind: modrinth::Kind,
    world: Option<String>,
    paths: Vec<String>,
) -> Result<modrinth::InstallReport, String> {
    let (store, _) = content_target(&app, &name, kind, world)?;
    ensure_not_running(&state, &name)?;
    modrinth::import_files(&store, &paths)
}

/// Otwiera w Eksploratorze folder z zawartością danego rodzaju (mods, shaderpacks itd.).
#[tauri::command]
fn open_content_folder(
    app: tauri::AppHandle,
    name: String,
    kind: modrinth::Kind,
    world: Option<String>,
) -> Result<(), String> {
    let (store, _) = content_target(&app, &name, kind, world)?;
    let dir = store.content_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let result = std::process::Command::new("explorer").arg(&dir).spawn();
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&dir).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = std::process::Command::new("xdg-open").arg(&dir).spawn();
    result.map(|_| ()).map_err(|e| format!("Nie udało się otworzyć folderu: {e}"))
}

#[tauri::command]
async fn install_perf_pack(
    app: tauri::AppHandle,
    state: tauri::State<'_, Running>,
    name: String,
) -> Result<modrinth::InstallReport, String> {
    let (store, inst) = content_target(&app, &name, modrinth::Kind::Mod, None)?;
    ensure_not_running(&state, &name)?;
    let projects: Vec<String> = modrinth::PERF_PACK.iter().map(|s| s.to_string()).collect();
    modrinth::install(&store, &inst.mc_version, &projects, true).await
}

#[tauri::command]
fn remove_mod(
    app: tauri::AppHandle,
    state: tauri::State<'_, Running>,
    name: String,
    project_id: String,
    kind: modrinth::Kind,
    world: Option<String>,
) -> Result<(), String> {
    let (store, _) = content_target(&app, &name, kind, world)?;
    ensure_not_running(&state, &name)?;
    modrinth::remove(&store, &project_id)
}

#[tauri::command]
async fn update_mods(
    app: tauri::AppHandle,
    state: tauri::State<'_, Running>,
    name: String,
    kind: modrinth::Kind,
    world: Option<String>,
    project_id: Option<String>,
) -> Result<modrinth::UpdateReport, String> {
    let (store, inst) = content_target(&app, &name, kind, world)?;
    ensure_not_running(&state, &name)?;
    modrinth::update_all(&store, &inst.mc_version, project_id.as_deref()).await
}

/// Które zainstalowane pozycje mają nowszą wersję na Modrinth.
#[tauri::command]
async fn check_updates(
    app: tauri::AppHandle,
    name: String,
    kind: modrinth::Kind,
    world: Option<String>,
) -> Result<Vec<modrinth::UpdateAvailable>, String> {
    let (store, inst) = content_target(&app, &name, kind, world)?;
    modrinth::check_updates(&store, &inst.mc_version).await
}

/// Włącza lub wyłącza mod, shader albo paczkę bez usuwania pliku.
#[tauri::command]
fn set_content_enabled(
    app: tauri::AppHandle,
    state: tauri::State<'_, Running>,
    name: String,
    kind: modrinth::Kind,
    world: Option<String>,
    project_id: String,
    enabled: bool,
) -> Result<(), String> {
    let (store, _) = content_target(&app, &name, kind, world)?;
    ensure_not_running(&state, &name)?;
    modrinth::set_enabled(&store, &project_id, enabled)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(Running::default())
        .manage(AuthState::default())
        .invoke_handler(tauri::generate_handler![
            list_releases,
            system_info,
            jvm_args,
            get_instances,
            create_instance,
            launch_instance,
            auth_status,
            auth_set_client_id,
            auth_login,
            auth_cancel,
            auth_logout,
            open_url,
            search_mods,
            list_worlds,
            list_mods,
            install_mod,
            install_perf_pack,
            import_files,
            open_content_folder,
            remove_mod,
            update_mods,
            check_updates,
            set_content_enabled,
            reset_verification,
            update_instance,
            duplicate_instance,
            open_instance_folder,
            delete_instance,
            import_mrpack,
            search_modpacks,
            install_modpack
        ])
        .run(tauri::generate_context!())
        .expect("błąd podczas uruchamiania aplikacji");
}
