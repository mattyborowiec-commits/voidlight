use crate::{download, util};
use serde::Deserialize;

#[derive(Deserialize)]
struct Entry {
    loader: LoaderInfo,
}

#[derive(Deserialize)]
struct LoaderInfo {
    version: String,
    stable: bool,
}

/// Najnowsza stabilna wersja Fabric Loader dla danej wersji Minecrafta.
pub async fn latest_loader(client: &reqwest::Client, mc: &str) -> Result<String, String> {
    if !util::safe_token(mc) {
        return Err(format!("Nieprawidłowa wersja: {mc}"));
    }
    let url = format!("https://meta.fabricmc.net/v2/versions/loader/{mc}");
    let text = download::get_text(client, &url).await?;
    let list: Vec<Entry> =
        serde_json::from_str(&text).map_err(|e| format!("Błędna odpowiedź Fabric: {e}"))?;
    list.iter()
        .find(|e| e.loader.stable)
        .or_else(|| list.first())
        .map(|e| e.loader.version.clone())
        .ok_or_else(|| format!("Fabric nie obsługuje wersji {mc}."))
}

/// Profil Fabric (JSON w formacie oficjalnego launchera) dla pary wersja gry + wersja loadera.
pub async fn profile_json(
    client: &reqwest::Client,
    mc: &str,
    loader: &str,
) -> Result<String, String> {
    if !util::safe_token(mc) || !util::safe_token(loader) {
        return Err("Nieprawidłowa wersja gry lub loadera.".to_string());
    }
    let url = format!("https://meta.fabricmc.net/v2/versions/loader/{mc}/{loader}/profile/json");
    download::get_text(client, &url).await
}
