use std::{
  fmt,
  path::{Path, PathBuf},
  sync::Arc,
};

use crate::{package_json::PackageJson, ustr_path::UstrPath};

/// The final path resolution with optional `?query` and `#fragment`
#[derive(Clone)]
pub struct Resolution {
  pub(crate) path: UstrPath,

  /// path query `?query`, contains `?`.
  pub(crate) query: Option<String>,

  /// path fragment `#query`, contains `#`.
  pub(crate) fragment: Option<String>,

  pub(crate) package_json: Option<Arc<PackageJson>>,
}

impl fmt::Debug for Resolution {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Resolution")
      .field("path", &self.path)
      .field("query", &self.query)
      .field("fragment", &self.fragment)
      .field("package_json", &self.package_json.as_ref().map(|p| &p.path))
      .finish()
  }
}

impl PartialEq for Resolution {
  fn eq(&self, other: &Self) -> bool {
    self.path == other.path && self.query == other.query && self.fragment == other.fragment
  }
}
impl Eq for Resolution {}

impl Resolution {
  /// Returns the path without query and fragment
  pub fn path(&self) -> &Path {
    self.path.as_std_path()
  }

  /// Returns the interned path without query and fragment.
  ///
  /// Zero-copy: hand this to a downstream store instead of `path()` to avoid
  /// re-allocating and re-hashing the string.
  pub fn ustr_path(&self) -> UstrPath {
    self.path
  }

  /// Returns the path without query and fragment
  pub fn into_path_buf(self) -> PathBuf {
    self.path.as_std_path().to_path_buf()
  }

  /// Returns the path query `?query`, contains the leading `?`
  pub fn query(&self) -> Option<&str> {
    self.query.as_deref()
  }

  /// Returns the path fragment `#fragment`, contains the leading `#`
  pub fn fragment(&self) -> Option<&str> {
    self.fragment.as_deref()
  }

  /// Returns serialized package_json
  pub fn package_json(&self) -> Option<&Arc<PackageJson>> {
    self.package_json.as_ref()
  }

  /// Returns the full path with query and fragment
  pub fn full_path(&self) -> PathBuf {
    let mut path = self.path.as_str().to_owned();
    if let Some(query) = &self.query {
      path.push_str(query);
    }
    if let Some(fragment) = &self.fragment {
      path.push_str(fragment);
    }
    PathBuf::from(path)
  }
}

#[tokio::test]
async fn test() {
  let resolution = Resolution {
    path: "foo".into(),
    query: Some("?query".to_string()),
    fragment: Some("#fragment".to_string()),
    package_json: None,
  };
  assert_eq!(resolution.path(), Path::new("foo"));
  assert_eq!(resolution.query(), Some("?query"));
  assert_eq!(resolution.fragment(), Some("#fragment"));
  assert_eq!(resolution.full_path(), PathBuf::from("foo?query#fragment"));
  assert_eq!(resolution.into_path_buf(), PathBuf::from("foo"));
}

#[tokio::test]
async fn ustr_path_accessor_is_the_same_pointer_as_the_stored_path() {
  let resolution = Resolution {
    path: "foo".into(),
    query: None,
    fragment: None,
    package_json: None,
  };
  assert_eq!(resolution.ustr_path().as_str(), "foo");
  assert_eq!(
    resolution.ustr_path().as_str().as_ptr(),
    UstrPath::new("foo").as_str().as_ptr()
  );
  // The legacy accessors keep their signatures.
  assert_eq!(resolution.path(), Path::new("foo"));
  assert_eq!(resolution.into_path_buf(), PathBuf::from("foo"));
}
