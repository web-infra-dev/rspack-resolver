use std::{
  hash::BuildHasherDefault,
  path::{Path, PathBuf},
  sync::Arc,
};

use indexmap::IndexMap;
use rustc_hash::FxHasher;
use serde::Deserialize;

use crate::{JsonParseError, PathUtil};

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
  pub fn parse(root: bool, path: &Path, json: &mut str) -> Result<Self, JsonParseError> {
    _ = json_strip_comments::strip(json);
    let value = if json.trim().is_empty() {
      simd_json::OwnedValue::Object(Default::default())
    } else {
      simd_json::to_owned_value(json.as_bytes_mut()).map_err(JsonParseError::from)?
    };
    let object = match value {
      simd_json::OwnedValue::Object(map) => map,
      _ => {
        return Err(JsonParseError::new(
          "tsconfig root value must be a JSON object",
        ))
      }
    };
    let mut tsconfig = Self::from_object(object)?;
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
      tsconfig.compiler_options.paths_base = tsconfig
        .compiler_options
        .base_url
        .as_ref()
        .map_or(directory, Clone::clone);
    }
    Ok(tsconfig)
  }

  fn from_object(object: simd_json::value::owned::Object) -> Result<Self, JsonParseError> {
    let mut tsconfig = Self {
      root: false,
      path: PathBuf::new(),
      extends: None,
      compiler_options: CompilerOptions::default(),
      references: Vec::new(),
    };

    if let Some(extends) = object.get("extends") {
      tsconfig.extends = Self::parse_extends(extends)?;
    }

    if let Some(compiler_options) = object.get("compilerOptions") {
      tsconfig.compiler_options = Self::parse_compiler_options(compiler_options)?;
    }

    if let Some(references) = object.get("references") {
      tsconfig.references = Self::parse_references(references)?;
    }

    Ok(tsconfig)
  }

  fn parse_extends(value: &simd_json::OwnedValue) -> Result<Option<ExtendsField>, JsonParseError> {
    if value.is_null() {
      return Ok(None);
    }
    if let Some(string) = value.as_str() {
      return Ok(Some(ExtendsField::Single(string.to_string())));
    }
    if let Some(values) = value.as_array() {
      let mut extends = Vec::with_capacity(values.len());
      for entry in values {
        let Some(string) = entry.as_str() else {
          return Err(JsonParseError::new(
            "tsconfig extends array entries must be strings",
          ));
        };
        extends.push(string.to_string());
      }
      return Ok(Some(ExtendsField::Multiple(extends)));
    }
    Err(JsonParseError::new(
      "tsconfig extends must be a string or an array of strings",
    ))
  }

  fn parse_compiler_options(
    value: &simd_json::OwnedValue,
  ) -> Result<CompilerOptions, JsonParseError> {
    let mut options = CompilerOptions::default();
    let Some(object) = value.as_object() else {
      return Err(JsonParseError::new(
        "tsconfig compilerOptions must be an object",
      ));
    };

    if let Some(base_url) = object.get("baseUrl") {
      let Some(base_url) = base_url.as_str() else {
        return Err(JsonParseError::new(
          "tsconfig compilerOptions.baseUrl must be a string",
        ));
      };
      options.base_url = Some(PathBuf::from(base_url));
    }

    if let Some(paths) = object.get("paths") {
      let Some(paths_object) = paths.as_object() else {
        return Err(JsonParseError::new(
          "tsconfig compilerOptions.paths must be an object",
        ));
      };
      let mut map = CompilerOptionsPathsMap::default();
      for (alias, targets) in paths_object {
        let Some(array) = targets.as_array() else {
          return Err(JsonParseError::new(
            "tsconfig compilerOptions.paths values must be arrays",
          ));
        };
        let mut paths = Vec::with_capacity(array.len());
        for value in array {
          let Some(target) = value.as_str() else {
            return Err(JsonParseError::new(
              "tsconfig compilerOptions.paths entries must be strings",
            ));
          };
          paths.push(target.to_string());
        }
        map.insert(alias.clone(), paths);
      }
      options.paths = Some(map);
    }

    Ok(options)
  }

  fn parse_references(
    value: &simd_json::OwnedValue,
  ) -> Result<Vec<ProjectReference>, JsonParseError> {
    let Some(array) = value.as_array() else {
      return Err(JsonParseError::new("tsconfig references must be an array"));
    };
    let mut references = Vec::with_capacity(array.len());
    for entry in array {
      let Some(object) = entry.as_object() else {
        return Err(JsonParseError::new(
          "tsconfig references entries must be objects",
        ));
      };
      let Some(path) = object.get("path") else {
        return Err(JsonParseError::new(
          "tsconfig references entries must contain a path",
        ));
      };
      let Some(path) = path.as_str() else {
        return Err(JsonParseError::new(
          "tsconfig references path must be a string",
        ));
      };
      references.push(ProjectReference {
        path: PathBuf::from(path),
        tsconfig: None,
      });
    }
    Ok(references)
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

      let mut p = self
        .compiler_options
        .paths_base
        .to_string_lossy()
        .to_string();
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
      compiler_options.paths_base = compiler_options.base_url.as_ref().map_or_else(
        || other_config.compiler_options.paths_base.clone(),
        Clone::clone,
      );
      compiler_options
        .paths
        .clone_from(&other_config.compiler_options.paths);
    }
    if compiler_options.base_url.is_none() {
      compiler_options
        .base_url
        .clone_from(&other_config.compiler_options.base_url);
    }
  }

  pub fn resolve(&self, path: &Path, specifier: &str) -> Vec<PathBuf> {
    for tsconfig in self
      .references
      .iter()
      .filter_map(|reference| reference.tsconfig.as_ref())
    {
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
      .map_or_else(
        Vec::new,
        |base_url| vec![base_url.normalize_with(specifier)],
      );

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

        best_key
          .and_then(|key| paths_map.get(key))
          .map_or_else(Vec::new, |paths| {
            paths
              .iter()
              .map(|path| {
                path.replace(
                  '*',
                  &specifier[longest_prefix_length..specifier.len() - longest_suffix_length],
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
    self
      .compiler_options
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
