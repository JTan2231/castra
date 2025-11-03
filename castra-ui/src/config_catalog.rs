use std::cmp::Ordering;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use castra::load_project_config;
use sha2::{Digest, Sha256};

/// Root directory (relative to the user's home) that stores curated configs.
const DEFAULT_CONFIG_SUBDIR: &str = ".castra/configs";

/// Metadata describing a discovered configuration file.
#[derive(Clone, Debug)]
pub struct ConfigEntry {
    pub path: PathBuf,
    pub display_name: String,
    pub workspace_slug: Option<String>,
    pub vm_count: Option<usize>,
    pub error: Option<String>,
}

impl ConfigEntry {
    pub fn summary(&self) -> Option<String> {
        if let Some(error) = &self.error {
            return Some(error.clone());
        }

        match (self.workspace_slug.as_ref(), self.vm_count) {
            (Some(slug), Some(count)) => Some(format!("workspace {slug} • {count} VM(s)")),
            (Some(slug), None) => Some(format!("workspace {slug}")),
            (None, Some(count)) => Some(format!("{count} VM(s)")),
            (None, None) => None,
        }
    }
}

/// Result of scanning the catalog directory.
#[derive(Clone, Debug)]
pub struct CatalogDiscovery {
    pub root: PathBuf,
    pub entries: Vec<ConfigEntry>,
}

/// Discover curated configs under `~/.castra/configs`, creating the directory if missing.
pub fn discover() -> Result<CatalogDiscovery, String> {
    let root = resolve_catalog_root()?;
    discover_in(&root)
}

/// Discover curated configs within a specific directory. Primarily exposed for tests.
pub fn discover_in(root: &Path) -> Result<CatalogDiscovery, String> {
    fs::create_dir_all(root).map_err(|err| {
        format!(
            "unable to create config directory {}: {err}",
            root.display()
        )
    })?;

    let mut entries = Vec::new();
    let iter = fs::read_dir(root)
        .map_err(|err| format!("unable to read config directory {}: {err}", root.display()))?;

    for candidate in iter {
        let entry = match candidate {
            Ok(entry) => entry,
            Err(err) => {
                entries.push(ConfigEntry {
                    path: root.to_path_buf(),
                    display_name: "unknown".to_string(),
                    workspace_slug: None,
                    vm_count: None,
                    error: Some(format!("entry read failed: {err}")),
                });
                continue;
            }
        };

        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension() != Some(OsStr::new("toml")) {
            continue;
        }

        entries.push(load_entry(&path));
    }

    entries.sort_by(entry_order);

    Ok(CatalogDiscovery {
        root: root.to_path_buf(),
        entries,
    })
}

fn resolve_catalog_root() -> Result<PathBuf, String> {
    let home = user_home_dir()
        .ok_or_else(|| "unable to determine home directory (HOME not set or empty)".to_string())?;
    Ok(home.join(DEFAULT_CONFIG_SUBDIR))
}

pub fn load_entry(path: &Path) -> ConfigEntry {
    match load_project_config(path) {
        Ok(project) => ConfigEntry {
            path: path.to_path_buf(),
            display_name: project.project_name.clone(),
            workspace_slug: derive_workspace_slug(&project.state_root),
            vm_count: Some(project.vms.len()),
            error: None,
        },
        Err(err) => {
            let display_name = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| stem.to_string())
                .unwrap_or_else(|| path.display().to_string());
            ConfigEntry {
                path: path.to_path_buf(),
                display_name,
                workspace_slug: None,
                vm_count: None,
                error: Some(err.to_string()),
            }
        }
    }
}

fn derive_workspace_slug(state_root: &Path) -> Option<String> {
    let repr = if state_root.exists() {
        match state_root.canonicalize() {
            Ok(path) => path.to_string_lossy().into_owned(),
            Err(_) => state_root.to_string_lossy().into_owned(),
        }
    } else {
        state_root.to_string_lossy().into_owned()
    };

    if repr.is_empty() {
        return None;
    }

    let mut hasher = Sha256::new();
    hasher.update(repr.as_bytes());
    let digest = hasher.finalize();
    Some(hex::encode(&digest[..8]))
}

fn entry_order(a: &ConfigEntry, b: &ConfigEntry) -> Ordering {
    let by_name = a
        .display_name
        .to_ascii_lowercase()
        .cmp(&b.display_name.to_ascii_lowercase());
    if by_name == Ordering::Equal {
        return a.path.cmp(&b.path);
    }
    by_name
}

fn user_home_dir() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("HOME") {
        if !home.is_empty() {
            return Some(PathBuf::from(home));
        }
    }

    #[cfg(windows)]
    {
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            if !profile.is_empty() {
                return Some(PathBuf::from(profile));
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_quickstart_config(path: &Path) {
        let contents = r#"
version = "0.2.0"

[project]
name = "sample"

[[vms]]
name = "alpine"
base_image = "alpine.qcow2"
cpus = 2
memory = "2 GiB"
"#;
        fs::write(path, contents.trim_start()).expect("config write should succeed");
    }

    #[test]
    fn discovery_returns_sorted_entries() {
        let temp_home = TempDir::new().expect("temp dir should exist");
        let catalog_root = temp_home.path().join("configs");
        fs::create_dir_all(&catalog_root).expect("catalog dir should exist");

        write_quickstart_config(&catalog_root.join("b.toml"));
        write_quickstart_config(&catalog_root.join("a.toml"));

        let result = discover_in(&catalog_root).expect("discovery should succeed");
        let labels: Vec<_> = result
            .entries
            .iter()
            .map(|entry| entry.display_name.clone())
            .collect();
        assert_eq!(labels, vec!["sample", "sample"]);
        let paths: Vec<_> = result
            .entries
            .iter()
            .map(|entry| {
                entry
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(paths, vec!["a.toml", "b.toml"]);
    }

    #[test]
    fn discovery_records_parse_errors() {
        let temp_home = TempDir::new().expect("temp dir should exist");
        let catalog_root = temp_home.path().join("configs");
        fs::create_dir_all(&catalog_root).expect("catalog dir should exist");

        fs::write(catalog_root.join("broken.toml"), "not toml").expect("write should succeed");

        let result = discover_in(&catalog_root).expect("discovery should succeed");
        assert_eq!(result.entries.len(), 1);
        let entry = &result.entries[0];
        assert_eq!(entry.display_name, "broken");
        assert!(entry.error.as_ref().unwrap().contains("Invalid"));
    }

    #[test]
    fn discovery_ignores_non_toml_files() {
        let temp_home = TempDir::new().expect("temp dir should exist");
        let catalog_root = temp_home.path().join("configs");
        fs::create_dir_all(&catalog_root).expect("catalog dir should exist");

        fs::write(catalog_root.join("notes.txt"), "hello").expect("write should succeed");
        write_quickstart_config(&catalog_root.join("only.toml"));

        let result = discover_in(&catalog_root).expect("discovery should succeed");
        assert_eq!(result.entries.len(), 1);
        assert_eq!(
            result.entries[0]
                .path
                .file_name()
                .unwrap()
                .to_string_lossy(),
            "only.toml"
        );
    }
}
