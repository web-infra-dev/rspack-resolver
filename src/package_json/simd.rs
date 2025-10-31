//! package.json definitions
//!
//! Code related to export field are copied from [Parcel's resolver](https://github.com/parcel-bundler/parcel/blob/v2/packages/utils/node-resolver-rs/src/package_json.rs)

use std::{
  fmt::Debug,
  path::{Path, PathBuf},
};

use self_cell::self_cell;
use simd_json::{borrowed::Value, prelude::*};

use crate::{path::PathUtil, JSONError, ResolveError};

pub type JSONMap<'a> = simd_json::borrowed::Object<'a>;

#[cfg(feature = "package_json_raw_json_api")]
use simd_json::serde::from_refborrowed_value;
pub use simd_json::BorrowedValue as JSONValue;

use crate::package_json::{ModuleType, SideEffects};

self_cell!(
    struct JSONCell {
        owner: Vec<u8>,

        #[covariant]
        dependent: JSONValue,
    }
    impl {Debug}
);

impl Default for JSONCell {
  fn default() -> Self {
    Self::new(Vec::new(), |_data| JSONValue::Static(StaticNode::Null))
  }
}

/// Deserialized package.json
#[derive(Debug, Default)]
pub struct PackageJson {
  /// Path to `package.json`. Contains the `package.json` filename.
  pub path: PathBuf,

  /// Realpath to `package.json`. Contains the `package.json` filename.
  pub realpath: PathBuf,

  /// The "name" field defines your package's name.
  /// The "name" field can be used in addition to the "exports" field to self-reference a package using its name.
  ///
  /// <https://nodejs.org/api/packages.html#name>
  pub name: Option<String>,

  /// The "type" field.
  ///
  /// <https://nodejs.org/api/packages.html#type>
  pub r#type: Option<ModuleType>,

  /// The "sideEffects" field.
  ///
  /// <https://webpack.js.org/guides/tree-shaking>
  pub side_effects: Option<SideEffects>,

  raw_json: std::sync::Arc<JSONCell>,

  #[cfg(feature = "package_json_raw_json_api")]
  serde_json: std::sync::Arc<serde_json::Value>,
}

impl PackageJson {
  /// # Panics
  /// # Errors
  pub(crate) fn parse(
    path: PathBuf,
    realpath: PathBuf,
    json: Vec<u8>,
  ) -> Result<Self, ResolveError> {
    let json_cell = JSONCell::try_new(json, |v| {
      // SAFETY: We have exclusive ownership of the Vec<u8>, so it's safe to cast to mutable.
      let slice = unsafe { std::slice::from_raw_parts_mut(v.as_ptr().cast_mut(), v.len()) };
      simd_json::to_borrowed_value(slice)
    })
    .map_err(|parse_error| {
      ResolveError::JSON(JSONError {
        path: path.clone(),
        message: "sj parse failed".to_string(),
        line: parse_error.index(),
        column: parse_error.index(),
        content: None,
      })
    })?;

    let mut package_json = Self::default();
    if let Some(json_object) = json_cell.borrow_dependent().as_object() {
      package_json.name = json_object
        .get("name")
        .and_then(|field| field.as_str())
        .map(ToString::to_string);

      package_json.r#type = json_object
        .get("type")
        .as_str()
        .and_then(|str| str.try_into().ok());
      package_json.side_effects = json_object
        .get("sideEffects")
        .and_then(|value| SideEffects::try_from(value).ok());

      #[cfg(feature = "package_json_raw_json_api")]
      {
        package_json.init_serde_json(json_object);
      }
    }

    package_json.path = path;
    package_json.realpath = realpath;
    package_json.raw_json = std::sync::Arc::new(json_cell);

    Ok(package_json)
  }

  #[cfg(feature = "package_json_raw_json_api")]
  fn init_serde_json(&mut self, value: &JSONMap) {
    const KEYS_TO_KEEP: [&str; 9] = [
      "name",
      "version",
      "sideEffects",
      "type",
      "main",
      "module",
      "exports",
      "imports",
      "browser",
    ];
    let mut json_map = serde_json::value::Map::new();

    for key in KEYS_TO_KEEP {
      if let Some(name) = value.get(key).and_then(|v| from_refborrowed_value(v).ok()) {
        json_map.insert(key.to_string(), name);
      }
    }
    self.serde_json = std::sync::Arc::new(serde_json::Value::Object(json_map));
  }

  fn get_value_by_paths<'a>(fields: &'a JSONMap, paths: &[String]) -> Option<&'a JSONValue<'a>> {
    if paths.is_empty() {
      return None;
    }

    let mut value = fields.get(paths[0].as_str())?;
    for key in paths.iter().skip(1) {
      if let Some(inner_value) = value.as_object().and_then(|o| o.get(key.as_str())) {
        value = inner_value;
      } else {
        return None;
      }
    }
    Some(value)
  }

  /// Raw serde json value of `package.json`.
  ///
  /// This is currently used in Rspack for:
  /// * getting the `sideEffects` field
  /// * query in <https://www.rspack.dev/config/module.html#ruledescriptiondata> - search on GitHub indicates query on the `type` field.
  ///
  /// To reduce overall memory consumption, large fields that useless for pragmatic use are removed.
  /// They are: `description`, `keywords`, `scripts`,
  /// `dependencies` and `devDependencies`, `peerDependencies`, `optionalDependencies`.
  #[cfg(feature = "package_json_raw_json_api")]
  pub fn raw_json(&self) -> &std::sync::Arc<serde_json::Value> {
    &self.serde_json
  }

  /// Directory to `package.json`
  ///
  /// # Panics
  ///
  /// * When the package.json path is misconfigured.
  pub fn directory(&self) -> &Path {
    debug_assert!(self
      .realpath
      .file_name()
      .is_some_and(|x| x == "package.json"));
    self.realpath.parent().unwrap()
  }

  /// The "main" field defines the entry point of a package when imported by name via a node_modules lookup. Its value is a path.
  ///
  /// When a package has an "exports" field, this will take precedence over the "main" field when importing the package by name.
  ///
  /// Values are dynamically retrieved from [ResolveOptions::main_fields].
  ///
  /// <https://nodejs.org/api/packages.html#main>
  pub(crate) fn main_fields<'a>(
    &'a self,
    main_fields: &'a [String],
  ) -> impl Iterator<Item = &'a str> + 'a {
    main_fields.iter().filter_map(|main_field| {
      self
        .raw_json
        .borrow_dependent()
        .get_str(main_field.as_str())
    })
  }

  /// The "exports" field allows defining the entry points of a package when imported by name loaded either via a node_modules lookup or a self-reference to its own name.
  ///
  /// <https://nodejs.org/api/packages.html#exports>
  pub(crate) fn exports_fields<'a>(
    &'a self,
    exports_fields: &'a [Vec<String>],
  ) -> impl Iterator<Item = &'a JSONValue<'a>> + 'a {
    exports_fields.iter().filter_map(|object_path| {
      self
        .raw_json
        .borrow_dependent()
        .as_object()
        .and_then(|json_object| Self::get_value_by_paths(json_object, object_path))
    })
  }

  /// In addition to the "exports" field, there is a package "imports" field to create private mappings that only apply to import specifiers from within the package itself.
  ///
  /// <https://nodejs.org/api/packages.html#subpath-imports>
  pub(crate) fn imports_fields<'a>(
    &'a self,
    imports_fields: &'a [Vec<String>],
  ) -> impl Iterator<Item = &'a JSONMap<'a>> + 'a {
    imports_fields.iter().filter_map(|object_path| {
      self
        .raw_json
        .borrow_dependent()
        .as_object()
        .and_then(|json_object| Self::get_value_by_paths(json_object, object_path))
        .and_then(|value| value.as_object())
    })
  }

  /// The "browser" field is provided by a module author as a hint to javascript bundlers or component tools when packaging modules for client side use.
  /// Multiple values are configured by [ResolveOptions::alias_fields].
  ///
  /// <https://github.com/defunctzombie/package-browser-field-spec>
  fn browser_fields<'a>(
    &'a self,
    alias_fields: &'a [Vec<String>],
  ) -> impl Iterator<Item = &'a JSONMap<'a>> + 'a {
    alias_fields.iter().filter_map(|object_path| {
      self
        .raw_json
        .borrow_dependent()
        .as_object()
        .and_then(|json_object| Self::get_value_by_paths(json_object, object_path))
        // Only object is valid, all other types are invalid
        // https://github.com/webpack/enhanced-resolve/blob/3a28f47788de794d9da4d1702a3a583d8422cd48/lib/AliasFieldPlugin.js#L44-L52
        .and_then(|value| value.as_object())
    })
  }

  /// Resolve the request string for this package.json by looking at the `browser` field.
  ///
  /// # Errors
  ///
  /// * Returns [ResolveError::Ignored] for `"path": false` in `browser` field.
  pub(crate) fn resolve_browser_field<'a>(
    &'a self,
    path: &Path,
    request: Option<&str>,
    alias_fields: &'a [Vec<String>],
  ) -> Result<Option<&'a str>, ResolveError> {
    for object in self.browser_fields(alias_fields) {
      if let Some(request) = request {
        if let Some(value) = object.get(request) {
          return Self::alias_value(path, value);
        }
      } else {
        let dir = self.path.parent().unwrap();
        for (key, value) in object {
          let joined = dir.normalize_with(key.to_string());
          if joined == path {
            return Self::alias_value(path, value);
          }
        }
      }
    }
    Ok(None)
  }

  fn alias_value<'a>(key: &Path, value: &'a JSONValue) -> Result<Option<&'a str>, ResolveError> {
    match value {
      JSONValue::String(value) => Ok(Some(value)),
      JSONValue::Static(sn) => {
        if matches!(sn.as_bool(), Some(false)) {
          Err(ResolveError::Ignored(key.to_path_buf()))
        } else {
          Ok(None)
        }
      }
      _ => Ok(None),
    }
  }
}

impl<'a> TryFrom<&'a JSONValue<'a>> for SideEffects {
  type Error = &'static str;
  fn try_from(value: &'a JSONValue<'a>) -> Result<Self, Self::Error> {
    match value {
      Value::Static(StaticNode::Bool(b)) => Ok(Self::Bool(*b)),
      Value::String(str) => Ok(Self::String(str.to_string())),
      Value::Array(arr) => {
        let mut vec = Vec::with_capacity(arr.len());
        for item in arr.iter() {
          if let Value::String(s) = item {
            vec.push(s.to_string());
          } else {
            return Err("Invalid sideEffects array item, expected string");
          }
        }
        Ok(Self::Array(vec))
      }
      _ => Err("Invalid sideEffects value, expected bool, string or array of strings"),
    }
  }
}
