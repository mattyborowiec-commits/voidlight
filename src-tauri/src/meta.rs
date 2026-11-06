//! Format JSON-a wersji Minecrafta (także profilu Fabric), reguły systemowe, argumenty i biblioteki.

use crate::util;
use serde::Deserialize;
use std::collections::HashMap;

fn current_os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "osx"
    } else {
        "linux"
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OsRule {
    pub name: Option<String>,
    pub arch: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Rule {
    pub action: String,
    pub os: Option<OsRule>,
    pub features: Option<HashMap<String, bool>>,
}

fn rule_matches(r: &Rule) -> bool {
    if let Some(os) = &r.os {
        if let Some(name) = &os.name {
            if name != current_os() {
                return false;
            }
        }
        if let Some(arch) = &os.arch {
            // Uruchamiamy wyłącznie 64-bitowo, więc "x86" (32-bit) nie pasuje.
            if !matches!(arch.as_str(), "x86_64" | "amd64") {
                return false;
            }
        }
    }
    if let Some(features) = &r.features {
        // Nie włączamy żadnych funkcji opcjonalnych (demo, własna rozdzielczość, quick play).
        if features.values().any(|v| *v) {
            return false;
        }
    }
    true
}

/// Ocena listy reguł: ostatnia pasująca reguła decyduje; brak reguł oznacza zgodę.
pub fn rules_allow(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }
    let mut allowed = false;
    for r in rules {
        if rule_matches(r) {
            allowed = r.action == "allow";
        }
    }
    allowed
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum StringOrVec {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional {
        #[serde(default)]
        rules: Vec<Rule>,
        value: StringOrVec,
    },
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub path: Option<String>,
    pub url: Option<String>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LibDownloads {
    pub artifact: Option<Artifact>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibDownloads>,
    /// Adres repozytorium Mavena (używany m.in. przez Fabric).
    pub url: Option<String>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    pub natives: Option<HashMap<String, String>>,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Download {
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Downloads {
    pub client: Option<Download>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersion {
    pub component: String,
    pub major_version: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    pub inherits_from: Option<String>,
    pub main_class: Option<String>,
    pub arguments: Option<Arguments>,
    pub minecraft_arguments: Option<String>,
    pub asset_index: Option<AssetIndexRef>,
    pub assets: Option<String>,
    pub downloads: Option<Downloads>,
    pub java_version: Option<JavaVersion>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

/// Łączy wersję bazową (Minecraft) z profilem loadera (Fabric).
pub fn merge(parent: VersionJson, child: VersionJson) -> VersionJson {
    let mut out = parent;
    if let Some(main_class) = child.main_class {
        out.main_class = Some(main_class);
    }
    if let Some(extra) = child.arguments {
        let mut args = out.arguments.take().unwrap_or_default();
        args.game.extend(extra.game);
        args.jvm.extend(extra.jvm);
        out.arguments = Some(args);
    }
    out.libraries.extend(child.libraries);
    if child.java_version.is_some() {
        out.java_version = child.java_version;
    }
    out.id = child.id;
    out
}

/// Powód, dla którego nie umiemy jeszcze uruchomić danej wersji (albo None, jeśli jest obsługiwana).
pub fn unsupported_reason(v: &VersionJson) -> Option<String> {
    if v.arguments.is_none() {
        return Some(
            "Wersje przed 1.13 nie są jeszcze obsługiwane (stary format argumentów).".to_string(),
        );
    }
    if v.main_class.is_none() {
        return Some("JSON wersji nie zawiera klasy głównej.".to_string());
    }
    None
}

#[derive(Debug, Clone)]
pub struct LibFile {
    pub url: String,
    pub rel_path: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

/// Lista bibliotek do pobrania i dodania do classpath (po regułach systemowych i bez duplikatów).
/// Przy duplikatach wygrywa późniejszy wpis (np. z profilu Fabric).
pub fn library_files(v: &VersionJson) -> Result<Vec<LibFile>, String> {
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, LibFile> = HashMap::new();

    for lib in &v.libraries {
        if !rules_allow(&lib.rules) {
            continue;
        }
        if lib.natives.is_some() {
            return Err(format!(
                "Biblioteka {} używa starego formatu natywnych plików. Ta wersja gry nie jest jeszcze obsługiwana (potrzebne rozpakowywanie natywnych bibliotek).",
                lib.name
            ));
        }

        let artifact = lib.downloads.as_ref().and_then(|d| d.artifact.as_ref());
        let file = match artifact {
            Some(a) => {
                let rel = a
                    .path
                    .clone()
                    .or_else(|| util::maven_path(&lib.name))
                    .ok_or_else(|| format!("Nieprawidłowa nazwa biblioteki: {}", lib.name))?;
                let url = match a.url.clone() {
                    Some(u) if !u.is_empty() => u,
                    _ => format!("https://libraries.minecraft.net/{rel}"),
                };
                LibFile {
                    url,
                    rel_path: rel,
                    sha1: a.sha1.clone(),
                    size: a.size,
                }
            }
            None => {
                let rel = util::maven_path(&lib.name)
                    .ok_or_else(|| format!("Nieprawidłowa nazwa biblioteki: {}", lib.name))?;
                let mut base = lib
                    .url
                    .clone()
                    .unwrap_or_else(|| "https://libraries.minecraft.net/".to_string());
                if !base.ends_with('/') {
                    base.push('/');
                }
                LibFile {
                    url: format!("{base}{rel}"),
                    rel_path: rel,
                    sha1: lib.sha1.clone(),
                    size: lib.size,
                }
            }
        };

        if !util::safe_rel(&file.rel_path) {
            return Err(format!("Niebezpieczna ścieżka biblioteki: {}", file.rel_path));
        }

        let key = util::lib_key(&lib.name);
        if !map.contains_key(&key) {
            order.push(key.clone());
        }
        map.insert(key, file);
    }

    Ok(order.into_iter().filter_map(|k| map.remove(&k)).collect())
}

/// Rozwija listę argumentów (reguły + podstawianie zmiennych).
pub fn expand_args(list: &[Argument], vars: &HashMap<&str, String>) -> Vec<String> {
    let mut out = Vec::new();
    for a in list {
        match a {
            Argument::Plain(s) => out.push(util::substitute(s, vars)),
            Argument::Conditional { rules, value } => {
                if rules_allow(rules) {
                    match value {
                        StringOrVec::One(s) => out.push(util::substitute(s, vars)),
                        StringOrVec::Many(items) => {
                            out.extend(items.iter().map(|s| util::substitute(s, vars)))
                        }
                    }
                }
            }
        }
    }
    out
}
