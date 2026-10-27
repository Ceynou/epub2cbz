pub mod barnes;
pub mod detection;
pub mod metadata;
pub mod parsing;
pub mod toc;

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

pub struct EpubArchive {
    archive: zip::ZipArchive<std::fs::File>,
    name_map: HashMap<String, usize>,
}

impl EpubArchive {
    pub fn open(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)
            .with_context(|| format!("Failed to open: {}", path.display()))?;
        let mut archive = zip::ZipArchive::new(file)?;
        let mut name_map = HashMap::new();
        for i in 0..archive.len() {
            if let Ok(entry) = archive.by_index_raw(i) {
                let name = entry.name().to_string();
                name_map.insert(name.to_lowercase(), i);
            }
        }
        Ok(Self { archive, name_map })
    }

    pub fn read_entry(&mut self, name: &str) -> Result<Vec<u8>> {
        let idx = self
            .name_map
            .get(&name.to_lowercase())
            .copied()
            .with_context(|| format!("Entry not found: {name}"))?;
        let mut entry = self.archive.by_index(idx)?;
        let mut buf = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buf)?;
        Ok(buf)
    }

    pub fn read_entry_string(&mut self, name: &str) -> Result<String> {
        let bytes = self.read_entry(name)?;
        // Handle BOM
        let s = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            String::from_utf8_lossy(&bytes[3..]).into_owned()
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        Ok(s)
    }

    pub fn entry_exists(&self, name: &str) -> bool {
        self.name_map.contains_key(&name.to_lowercase())
    }

    pub fn entry_size(&mut self, name: &str) -> Option<u64> {
        let idx = self.name_map.get(&name.to_lowercase()).copied()?;
        self.archive.by_index_raw(idx).ok().map(|e| e.size())
    }

    pub fn entry_names(&mut self) -> Vec<String> {
        let mut names = Vec::new();
        for i in 0..self.archive.len() {
            if let Ok(entry) = self.archive.by_index_raw(i) {
                names.push(entry.name().to_string());
            }
        }
        names
    }
}

pub fn resolve_root_path(root_file: &str, relative_file: &str) -> String {
    let decoded = percent_encoding::percent_decode_str(relative_file)
        .decode_utf8_lossy()
        .into_owned();

    let base_str = format!("dummy://root/{root_file}");
    let base = match url::Url::parse(&base_str) {
        Ok(u) => u,
        Err(_) => return decoded,
    };
    match base.join(&decoded) {
        Ok(resolved) => resolved.path().trim_start_matches('/').to_string(),
        Err(_) => decoded,
    }
}
