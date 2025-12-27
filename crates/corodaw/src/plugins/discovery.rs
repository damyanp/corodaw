use std::{
    cell::RefCell,
    io::Write,
    path::{Path, PathBuf},
};

use clack_host::{bundle::PluginBundle, plugin::PluginDescriptor};
use gpui::SharedString;
use serde::{Deserialize, Serialize};
use walkdir::{DirEntry, WalkDir};

#[derive(Serialize, Deserialize, Clone)]
pub struct FoundPlugin {
    pub id: SharedString,
    pub name: SharedString,
    pub path: PathBuf,

    #[serde(skip)]
    _bundle: Option<PluginBundle>,
}

impl FoundPlugin {
    fn try_from_descriptor(
        descriptor: &PluginDescriptor,
        path: PathBuf,
        bundle: PluginBundle,
    ) -> Option<Self> {
        let id = descriptor
            .id()
            .and_then(|id| id.to_str().ok())
            .map(SharedString::new);
        let name = descriptor
            .name()
            .and_then(|name| name.to_str().ok())
            .map(SharedString::new);

        if let Some(id) = id
            && let Some(name) = name
        {
            Some(Self {
                id,
                name,
                path,
                _bundle: Some(bundle),
            })
        } else {
            None
        }
    }

    pub fn load_bundle(&mut self) -> PluginBundle {
        if let Some(bundle) = &self._bundle {
            bundle.clone()
        } else {
            println!("Loading bundle from {}", self.path.display());
            let bundle = unsafe { PluginBundle::load(&self.path) }
                .expect("Currently no error handling around loading bundles!");
            self._bundle = Some(bundle.clone());
            bundle
        }
    }
}

pub fn get_plugins() -> Vec<RefCell<FoundPlugin>> {
    load_plugin_cache()
        .unwrap_or_else(|| {
            let plugins = find_plugins();
            save_plugin_cache(&plugins);
            plugins
        })
        .into_iter()
        .map(RefCell::new)
        .collect()
}

fn save_plugin_cache(plugins: &Vec<FoundPlugin>) {
    let plugins_json = serde_json::to_string_pretty(plugins)
        .unwrap_or_else(|e| format!("{{\"error\":\"failed to serialize plugins: {e}\"}}"));

    // Write JSON to ".plugins.json" next to the current working directory.
    let mut f = std::fs::File::create(".plugins.json").expect("create .plugins.json");
    f.write_all(plugins_json.as_bytes())
        .and_then(|_| f.write_all(b"\n"))
        .expect("write .plugins.json");

    println!("Wrote .plugins.json");
}

fn load_plugin_cache() -> Option<Vec<FoundPlugin>> {
    match std::fs::read_to_string(".plugins.json") {
        Ok(contents) => match serde_json::from_str::<Vec<FoundPlugin>>(&contents) {
            Ok(plugins) => {
                println!("Loaded {} plugins from .plugins.json", plugins.len());
                return Some(plugins);
            }
            Err(err) => {
                println!("Failed to parse .plugins.json ({err})");
            }
        },
        Err(err) => {
            println!("No .plugins.json cache found ({err})");
        }
    }
    None
}

pub fn find_plugins() -> Vec<FoundPlugin> {
    find_bundles()
        .iter()
        .flat_map(|(p, b)| get_plugins_in_bundle(p, b))
        .collect()
}

fn find_bundles() -> Vec<(PathBuf, PluginBundle)> {
    standard_clap_paths()
        .iter()
        .flat_map(|path| {
            WalkDir::new(path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(is_clap_bundle)
                .filter_map(|bundle_dir_entry| {
                    unsafe { PluginBundle::load(bundle_dir_entry.path()) }
                        .ok()
                        .map(|bundle| (bundle_dir_entry.into_path(), bundle))
                })
        })
        .collect()
}

fn get_plugins_in_bundle(path: &Path, bundle: &PluginBundle) -> Vec<FoundPlugin> {
    bundle
        .get_plugin_factory()
        .map(|factory| {
            factory
                .plugin_descriptors()
                .filter_map(|descriptor| {
                    FoundPlugin::try_from_descriptor(descriptor, path.to_path_buf(), bundle.clone())
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Returns a list of all the standard CLAP search paths, per the CLAP specification.
// From clack/host/examples/cpal/src/discovery.rs
fn standard_clap_paths() -> Vec<PathBuf> {
    let mut paths = vec![];

    if let Some(home_dir) = dirs::home_dir() {
        paths.push(home_dir.join(".clap"));

        #[cfg(target_os = "macos")]
        {
            paths.push(home_dir.join("Library/Audio/Plug-Ins/CLAP"));
        }
    }

    #[cfg(windows)]
    {
        if let Some(val) = std::env::var_os("CommonProgramFiles") {
            paths.push(PathBuf::from(val).join("CLAP"))
        }

        if let Some(dir) = dirs::config_local_dir() {
            paths.push(dir.join("Programs\\Common\\CLAP"));
        }
    }

    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from("/Library/Audio/Plug-Ins/CLAP"));
    }

    #[cfg(target_family = "unix")]
    {
        paths.push("/usr/lib/clap".into())
    }

    if let Some(env_var) = std::env::var_os("CLAP_PATH") {
        paths.extend(std::env::split_paths(&env_var))
    }

    paths
}

/// Returns `true` if the given entry could refer to a CLAP bundle.
///
/// CLAP bundles are files that end with the `.clap` extension.
fn is_clap_bundle(dir_entry: &DirEntry) -> bool {
    is_bundle(dir_entry)
        && dir_entry
            .path()
            .extension()
            .is_some_and(|ext| ext == "clap")
}

/// Returns `true` if the given entry could refer to a bundle.
///
/// CLAP bundles are directories on MacOS and files everywhere else.
fn is_bundle(dir_entry: &DirEntry) -> bool {
    if cfg!(target_os = "macos") {
        dir_entry.file_type().is_dir()
    } else {
        dir_entry.file_type().is_file()
    }
}
