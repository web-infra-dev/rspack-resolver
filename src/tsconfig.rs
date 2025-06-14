use indexmap::IndexMap;
use rustc_hash::FxHasher;
use serde::Deserialize;
use std::{
    hash::BuildHasherDefault,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::PathUtil;

pub type CompilerOptionsPathsMap = IndexMap<String, Vec<String>, BuildHasherDefault<FxHasher>>;

#[derive(Debug, Clone, Eq, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum ExtendsField {
    Single(String),
    Multiple(Vec<String>),
}

const TEMPLATE_VARIABLE: &str = "${configDir}";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TsConfig {
    /// Whether this is the caller tsconfig.
    /// Used for final template variable substitution when all configs are extended and merged.
    #[serde(skip)]
    root: bool,

    /// Path to `tsconfig.json`. Contains the `tsconfig.json` filename.
    #[serde(skip)]
    pub(crate) path: PathBuf,

    #[serde(default)]
    pub extends: Option<ExtendsField>,

    #[serde(default)]
    pub compiler_options: CompilerOptions,

    /// Bubbled up project references with a reference to their tsconfig.
    #[serde(default)]
    pub references: Vec<ProjectReference>,
}

/// Compiler Options
///
/// <https://www.typescriptlang.org/tsconfig#compilerOptions>
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompilerOptions {
    base_url: Option<PathBuf>,

    /// Path aliases
    paths: Option<CompilerOptionsPathsMap>,

    /// The actual base for where path aliases are resolved from.
    #[serde(skip)]
    paths_base: PathBuf,
}

/// Project Reference
///
/// <https://www.typescriptlang.org/docs/handbook/project-references.html>
#[derive(Debug, Deserialize)]
pub struct ProjectReference {
    /// The path property of each reference can point to a directory containing a tsconfig.json file,
    /// or to the config file itself (which may have any name).
    pub path: PathBuf,

    /// Reference to the resolved tsconfig
    #[serde(skip)]
    pub tsconfig: Option<Arc<TsConfig>>,
}

impl TsConfig {
    pub fn parse(root: bool, path: &Path, json: &mut str) -> Result<Self, serde_json::Error> {
        _ = json_strip_comments::strip(json);
        if json.trim().is_empty() {
            let mut tsconfig: Self = serde_json::from_str("{}")?;
            tsconfig.root = root;
            tsconfig.path = path.to_path_buf();
            return Ok(tsconfig);
        }
        let mut tsconfig: Self = serde_json::from_str(json)?;
        tsconfig.root = root;
        tsconfig.path = path.to_path_buf();
        let directory = tsconfig.directory().to_path_buf();
        if let Some(base_url) = &tsconfig.compiler_options.base_url {
            // keep the `${configDir}` template variable in the baseUrl
            if !base_url.starts_with(TEMPLATE_VARIABLE) {
                tsconfig.compiler_options.base_url = Some(directory.normalize_with(base_url));
            }
        }
        if tsconfig.compiler_options.paths.is_some() {
            tsconfig.compiler_options.paths_base =
                tsconfig.compiler_options.base_url.as_ref().map_or(directory, Clone::clone);
        }
        Ok(tsconfig)
    }

    pub fn build(mut self) -> Self {
        if self.root {
            let dir = self.directory().to_path_buf();
            // Substitute template variable in `tsconfig.compilerOptions.paths`
            if let Some(paths) = &mut self.compiler_options.paths {
                for paths in paths.values_mut() {
                    for path in paths {
                        Self::substitute_template_variable(&dir, path);
                    }
                }
            }

            let mut p = self.compiler_options.paths_base.to_string_lossy().to_string();
            Self::substitute_template_variable(&dir, &mut p);
            self.compiler_options.paths_base = p.into();

            if let Some(base_url) = self.compiler_options.base_url.as_mut() {
                let mut p = base_url.to_string_lossy().to_string();
                Self::substitute_template_variable(&dir, &mut p);
                *base_url = p.into();
            }
        }
        self
    }

    /// Directory to `tsconfig.json`
    ///
    /// # Panics
    ///
    /// * When the `tsconfig.json` path is misconfigured.
    pub fn directory(&self) -> &Path {
        debug_assert!(self.path.file_name().is_some());
        self.path.parent().unwrap()
    }

    pub fn extend_tsconfig(&mut self, other_config: &Self) {
        let compiler_options = &mut self.compiler_options;
        if compiler_options.paths.is_none() {
            compiler_options.paths_base = compiler_options
                .base_url
                .as_ref()
                .map_or_else(|| other_config.compiler_options.paths_base.clone(), Clone::clone);
            compiler_options.paths.clone_from(&other_config.compiler_options.paths);
        }
        if compiler_options.base_url.is_none() {
            compiler_options.base_url.clone_from(&other_config.compiler_options.base_url);
        }
    }

    pub fn resolve(&self, path: &Path, specifier: &str) -> Vec<PathBuf> {
        for tsconfig in self.references.iter().filter_map(|reference| reference.tsconfig.as_ref()) {
            if path.starts_with(tsconfig.base_path()) {
                return tsconfig.resolve_path_alias(specifier);
            }
        }

        self.resolve_path_alias(specifier)
    }

    // Copied from parcel
    // <https://github.com/parcel-bundler/parcel/blob/b6224fd519f95e68d8b93ba90376fd94c8b76e69/packages/utils/node-resolver-rs/src/tsconfig.rs#L93>
    pub fn resolve_path_alias(&self, specifier: &str) -> Vec<PathBuf> {
        if specifier.starts_with(['/', '.']) {
            return vec![];
        }

        let base_url_iter = self
            .compiler_options
            .base_url
            .as_ref()
            .map_or_else(Vec::new, |base_url| vec![base_url.normalize_with(specifier)]);

        let Some(paths_map) = &self.compiler_options.paths else {
            return base_url_iter;
        };

        let paths = paths_map.get(specifier).map_or_else(
            || {
                let mut longest_prefix_length = 0;
                let mut longest_suffix_length = 0;
                let mut best_key: Option<&String> = None;

                for key in paths_map.keys() {
                    if let Some((prefix, suffix)) = key.split_once('*') {
                        if (best_key.is_none() || prefix.len() > longest_prefix_length)
                            && specifier.starts_with(prefix)
                            && specifier.ends_with(suffix)
                        {
                            longest_prefix_length = prefix.len();
                            longest_suffix_length = suffix.len();
                            best_key.replace(key);
                        }
                    }
                }

                best_key.and_then(|key| paths_map.get(key)).map_or_else(Vec::new, |paths| {
                    paths
                        .iter()
                        .map(|path| {
                            path.replace(
                                '*',
                                &specifier[longest_prefix_length
                                    ..specifier.len() - longest_suffix_length],
                            )
                        })
                        .collect::<Vec<_>>()
                })
            },
            Clone::clone,
        );

        paths
            .into_iter()
            .map(|p| self.compiler_options.paths_base.normalize_with(p))
            .chain(base_url_iter)
            .collect()
    }

    fn base_path(&self) -> &Path {
        self.compiler_options
            .base_url
            .as_ref()
            .map_or_else(|| self.directory(), |path| path.as_ref())
    }

    /// Template variable `${configDir}` for substitution of config files directory path
    ///
    /// NOTE: All tests cases are just a head replacement of `${configDir}`, so we are constrained as such.
    ///
    /// See <https://github.com/microsoft/TypeScript/pull/58042>
    fn substitute_template_variable(directory: &Path, path: &mut String) {
        if let Some(stripped_path) = path.strip_prefix(TEMPLATE_VARIABLE) {
            if let Some(unleashed_path) = stripped_path.strip_prefix("/") {
                *path = directory.join(unleashed_path).to_string_lossy().to_string();
            } else {
                *path = directory.join(stripped_path).to_string_lossy().to_string();
            }
        }
    }
}
