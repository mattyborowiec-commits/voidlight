use serde::{Deserialize, Serialize};

const MANIFEST_URLS: [&str; 2] = [
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
    "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: String,
}

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub versions: Vec<VersionEntry>,
}

/// Pobiera oficjalną listę wersji Minecrafta od Mojang.
pub async fn fetch_manifest() -> Result<Manifest, String> {
    let client = reqwest::Client::builder()
        .user_agent("Voidlight/0.1")
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let mut last = String::new();
    for url in MANIFEST_URLS {
        for _ in 0..2 {
            let attempt: Result<Manifest, String> = async {
                let resp = client.get(url).send().await.map_err(|e| format!("{e:?}"))?;
                if !resp.status().is_success() {
                    return Err(format!("HTTP {}", resp.status()));
                }
                resp.json::<Manifest>().await.map_err(|e| e.to_string())
            }
            .await;
            match attempt {
                Ok(m) => return Ok(m),
                Err(e) => last = format!("{url}: {e}"),
            }
        }
    }
    Err(format!("Nie można połączyć się z serwerem Mojang ({last}). Sprawdź internet, zaporę lub antywirus."))
}

/// Wpis o konkretnej wersji z oficjalnego manifestu Mojang.
pub async fn find_version(id: &str) -> Result<VersionEntry, String> {
    let manifest = fetch_manifest().await?;
    manifest
        .versions
        .into_iter()
        .find(|v| v.id == id)
        .ok_or_else(|| format!("Nie znaleziono wersji {id} w manifeście Mojang."))
}

/// Zwraca tylko wydania (bez snapshotów).
pub async fn fetch_releases() -> Result<Vec<VersionEntry>, String> {
    let manifest = fetch_manifest().await?;
    Ok(manifest
        .versions
        .into_iter()
        .filter(|v| v.kind == "release")
        .collect())
}
