use std::ops::{Deref, DerefMut};

use crate::{
  error::ResolveError,
  resolver_path::{ResolverPath, ToResolverPath},
};

#[derive(Debug, Default, Clone)]
pub struct ResolveContext(ResolveContextImpl);

#[derive(Debug, Default, Clone)]
pub struct ResolveContextImpl {
  pub fully_specified: bool,

  pub query: Option<String>,

  pub fragment: Option<String>,

  /// Files that were found on file system
  pub file_dependencies: Option<Vec<ResolverPath>>,

  /// Files that were not found on file system
  pub missing_dependencies: Option<Vec<ResolverPath>>,

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

  // Accepts anything path-shaped, by reference. Taking `&P` (not `P`) matters:
  // interning is a global-lock + hashtable probe with a permanent allocation on
  // miss, so it must only happen inside the `Some` branch — a by-value `dep`
  // would be evaluated (and interned) before the `if let` ever runs, silently
  // paying that cost on every call even when `resolve()` was invoked without a
  // context and these fields are `None`.
  pub fn add_file_dependency<P: ToResolverPath + ?Sized>(&mut self, dep: &P) {
    if let Some(deps) = &mut self.file_dependencies {
      deps.push(dep.to_resolver_path());
    }
  }

  pub fn add_missing_dependency<P: ToResolverPath + ?Sized>(&mut self, dep: &P) {
    if let Some(deps) = &mut self.missing_dependencies {
      deps.push(dep.to_resolver_path());
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
