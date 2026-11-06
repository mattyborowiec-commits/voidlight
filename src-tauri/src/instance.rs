use crate::jvm::JvmProfile;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::{Path, PathBuf}};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Vanilla,
    Fabric,
    Quilt,
    NeoForge,
}

/// Jedna instancja gry = osobny folder z własnymi modami, światami i ustawieniami.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instance {
    pub name: String,
    pub mc_version: String,
    pub loader: Loader,
    pub loader_version: Option<String>,
    pub ram_mb: u64,
    pub jvm_profile: JvmProfile,
}

impl Instance {
    pub fn dir(&self, root: &Path) -> PathBuf {
        root.join("instances").join(&self.name)
    }

    pub fn save(&self, root: &Path) -> io::Result<()> {
        let dir = self.dir(root);
        fs::create_dir_all(dir.join("mods"))?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        fs::write(dir.join("instance.json"), json)
    }

    pub fn load(root: &Path, name: &str) -> io::Result<Self> {
        let raw = fs::read_to_string(root.join("instances").join(name).join("instance.json"))?;
        serde_json::from_str(&raw).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

/// Lista wszystkich instancji w folderze launchera.
pub fn list_instances(root: &Path) -> io::Result<Vec<Instance>> {
    let dir = root.join("instances");
    if !dir.exists() {
        return Ok(vec![]);
    }
    let mut out = vec![];
    for entry in fs::read_dir(dir)? {
        let name = entry?.file_name().to_string_lossy().to_string();
        if let Ok(inst) = Instance::load(root, &name) {
            out.push(inst);
        }
    }
    Ok(out)
}
