use std::path::PathBuf;

use clack_host::{plugin::PluginDescriptor, prelude::*};
use walkdir::{DirEntry, WalkDir};

pub struct FoundPlugin {
    pub descriptor: PluginDescriptor,
    pub bundle: PluginBundle,
}

pub fn find_plugins() -> Vec<FoundPlugin> {
    find_bundles()
        .iter()
        .flat_map(get_plugins_in_bundle)
        .collect()
}

fn find_bundles() -> Vec<PluginBundle> {
    standard_clap_paths()
        .iter()
        .flat_map(|path| {
            WalkDir::new(path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(is_clap_bundle)
                .filter_map(|bundle_dir_entry| {
                    unsafe { PluginBundle::load(bundle_dir_entry.path()) }.ok()
                })
        })
        .collect()
}

fn get_plugins_in_bundle(bundle: &PluginBundle) -> Vec<FoundPlugin> {
    bundle
        .get_plugin_factory()
        .map(|factory| {
            factory
                .plugin_descriptors()
                .map(|descriptor| FoundPlugin {
                    descriptor: descriptor.clone(),
                    bundle: bundle.clone(),
                })
                .collect()
        })
        .unwrap_or(Vec::new())
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
