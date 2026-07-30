use std::ops::{Deref, DerefMut};

use crate::{
  error::ResolveError,
  ustr_path::{ToUstrPath, UstrPath},
};

#[derive(Debug, Default, Clone)]
pub struct ResolveContext(ResolveContextImpl);

#[derive(Debug, Default, Clone)]
pub struct ResolveContextImpl {
  pub fully_specified: bool,

  pub query: Option<String>,

  pub fragment: Option<String>,

  /// Files that were found on file system
  pub file_dependencies: Option<Vec<UstrPath>>,

  /// Files that were not found on file system
  pub missing_dependencies: Option<Vec<UstrPath>>,

  /// The current resolving alias for bailing recursion alias.
  pub resolving_alias: Option<String>,

  /// For avoiding infinite recursion, which will cause stack overflow.
  depth: u8,
}

impl Deref for ResolveContext {
  type Target = ResolveContextImpl;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl DerefMut for ResolveContext {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.0
  }
}

impl ResolveContext {
  pub fn with_fully_specified(&mut self, yes: bool) {
    self.fully_specified = yes;
  }

  pub fn with_query_fragment(&mut self, query: Option<&str>, fragment: Option<&str>) {
    if let Some(query) = query {
      self.query.replace(query.to_string());
    }
    if let Some(fragment) = fragment {
      self.fragment.replace(fragment.to_string());
    }
  }

  pub fn init_file_dependencies(&mut self) {
    self.file_dependencies.replace(vec![]);
    self.missing_dependencies.replace(vec![]);
  }

  // Accepts anything path-shaped. The interning only runs inside the `Some`
  // branch, so `resolve()` calls without a context still pay zero.
  // `ToUstrPath::to_ustr_path` only ever borrows `dep`, so clippy sees the
  // by-value `P` as needless; kept by value anyway so callers can pass owned
  // or borrowed path types through the same generic call site.
  #[allow(clippy::needless_pass_by_value)]
  pub fn add_file_dependency<P: ToUstrPath>(&mut self, dep: P) {
    if let Some(deps) = &mut self.file_dependencies {
      deps.push(dep.to_ustr_path());
    }
  }

  #[allow(clippy::needless_pass_by_value)]
  pub fn add_missing_dependency<P: ToUstrPath>(&mut self, dep: P) {
    if let Some(deps) = &mut self.missing_dependencies {
      deps.push(dep.to_ustr_path());
    }
  }

  pub fn with_resolving_alias(&mut self, alias: String) {
    self.resolving_alias = Some(alias);
  }

  pub fn test_for_infinite_recursion(&mut self) -> Result<(), ResolveError> {
    self.depth += 1;
    // 64 should be more than enough for detecting infinite recursion.
    if self.depth > 32 {
      return Err(ResolveError::Recursion);
    }
    Ok(())
  }
}
