//! # Rspack Resolver
//!
//! Node.js [CommonJS][cjs] and [ECMAScript][esm] Module Resolution.
//!
//! Released on [crates.io](https://crates.io/crates/rspack_resolver) and [npm](https://www.npmjs.com/package/@rspack/resolver).
//!
//! A module resolution is the process of finding the file referenced by a module specifier in
//! `import "specifier"` or `require("specifier")`.
//!
//! All [configuration options](ResolveOptions) are aligned with webpack's [enhanced-resolve].
//!
//! ## Terminology
//!
//! ### Specifier
//!
//! For [CommonJS modules][cjs],
//! the specifier is the string passed to the `require` function. e.g. `"id"` in `require("id")`.
//!
//! For [ECMAScript modules][esm],
//! the specifier of an `import` statement is the string after the `from` keyword,
//! e.g. `'specifier'` in `import 'specifier'` or `import { sep } from 'specifier'`.
//! Specifiers are also used in export from statements, and as the argument to an `import()` expression.
//!
//! This is also named "request" in some places.
//!
//! ## References:
//!
//! * Algorithm adapted from Node.js [CommonJS Module Resolution Algorithm] and [ECMAScript Module Resolution Algorithm].
//! * Tests are ported from [enhanced-resolve].
//! * Some code is adapted from [parcel-resolver].
//! * The documentation is copied from [webpack's resolve configuration](https://webpack.js.org/configuration/resolve).
//!
//! [enhanced-resolve]: https://github.com/webpack/enhanced-resolve
//! [CommonJS Module Resolution Algorithm]: https://nodejs.org/api/modules.html#all-together
//! [ECMAScript Module Resolution Algorithm]: https://nodejs.org/api/esm.html#resolution-algorithm-specification
//! [parcel-resolver]: https://github.com/parcel-bundler/parcel/blob/v2/packages/utils/node-resolver-rs
//! [cjs]: https://nodejs.org/api/modules.html
//! [esm]: https://nodejs.org/api/esm.html
//!
//! ## Feature flags
#![cfg_attr(feature = "document-features", doc = document_features::document_features!())]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]
//!
//! ## Example
//!
//! ```rust,ignore
#![doc = include_str!("../examples/resolver.rs")]
//! ```

mod builtins;
mod cache;
mod context;
mod error;
mod file_system;
mod options;
mod package_json;
mod path;
mod resolution;
mod specifier;
mod tsconfig;

#[cfg(test)]
mod tests;

use dashmap::{mapref::one::Ref, DashMap};
use rustc_hash::FxHashSet;
use serde_json::Value as JSONValue;
use std::{
    borrow::Cow,
    cmp::Ordering,
    ffi::OsStr,
    fmt,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

pub use crate::{
    builtins::NODEJS_BUILTINS,
    error::{JSONError, ResolveError, SpecifierError},
    file_system::{FileMetadata, FileSystem, FileSystemOs},
    options::{
        Alias, AliasValue, EnforceExtension, ResolveOptions, Restriction, TsconfigOptions,
        TsconfigReferences,
    },
    package_json::PackageJson,
    resolution::Resolution,
};
use crate::{
    cache::{Cache, CachedPath},
    context::ResolveContext as Ctx,
    package_json::JSONMap,
    path::{PathUtil, SLASH_START},
    specifier::Specifier,
    tsconfig::ExtendsField,
    tsconfig::{ProjectReference, TsConfig},
};
use futures::future::{try_join_all, BoxFuture};

type ResolveResult = Result<Option<CachedPath>, ResolveError>;

/// Context returned from the [Resolver::resolve_with_context] API
#[derive(Debug, Default, Clone)]
pub struct ResolveContext {
    /// Files that was found on file system
    pub file_dependencies: FxHashSet<PathBuf>,

    /// Dependencies that was not found on file system
    pub missing_dependencies: FxHashSet<PathBuf>,
}

/// Resolver with the current operating system as the file system
pub type Resolver = ResolverGeneric<FileSystemOs>;

/// Generic implementation of the resolver, can be configured by the [FileSystem] trait
pub struct ResolverGeneric<Fs> {
    options: ResolveOptions,
    cache: Arc<Cache<Fs>>,
    #[cfg(feature = "yarn_pnp")]
    pnp_manifest_content_cache: Arc<DashMap<CachedPath, Option<pnp::Manifest>>>,
    #[cfg(feature = "yarn_pnp")]
    pnp_manifest_path_cache: Arc<DashMap<PathBuf, Option<CachedPath>>>,
}

impl<Fs> fmt::Debug for ResolverGeneric<Fs> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.options.fmt(f)
    }
}

impl<Fs: Send + Sync + FileSystem + Default> Default for ResolverGeneric<Fs> {
    fn default() -> Self {
        Self::new(ResolveOptions::default())
    }
}

impl<Fs: Send + Sync + FileSystem + Default> ResolverGeneric<Fs> {
    pub fn new(options: ResolveOptions) -> Self {
        Self {
            options: options.sanitize(),
            cache: Arc::new(Cache::new(Fs::default())),
            #[cfg(feature = "yarn_pnp")]
            pnp_manifest_content_cache: Arc::new(DashMap::default()),
            #[cfg(feature = "yarn_pnp")]
            pnp_manifest_path_cache: Arc::new(DashMap::default()),
        }
    }
}

impl<Fs: FileSystem + Send + Sync> ResolverGeneric<Fs> {
    pub fn new_with_file_system(file_system: Fs, options: ResolveOptions) -> Self {
        Self {
            options: options.sanitize(),
            cache: Arc::new(Cache::new(file_system)),
            #[cfg(feature = "yarn_pnp")]
            pnp_manifest_content_cache: Arc::new(DashMap::default()),
            #[cfg(feature = "yarn_pnp")]
            pnp_manifest_path_cache: Arc::new(DashMap::default()),
        }
    }

    /// Clone the resolver using the same underlying cache.
    #[must_use]
    pub fn clone_with_options(&self, options: ResolveOptions) -> Self {
        Self {
            options: options.sanitize(),
            cache: Arc::clone(&self.cache),
            #[cfg(feature = "yarn_pnp")]
            pnp_manifest_content_cache: Arc::clone(&self.pnp_manifest_content_cache),
            #[cfg(feature = "yarn_pnp")]
            pnp_manifest_path_cache: Arc::clone(&self.pnp_manifest_path_cache),
        }
    }

    /// Returns the options.
    pub fn options(&self) -> &ResolveOptions {
        &self.options
    }

    /// Clear the underlying cache.
    pub fn clear_cache(&self) {
        self.cache.clear();
        #[cfg(feature = "yarn_pnp")]
        {
            self.pnp_manifest_content_cache.clear();
            self.pnp_manifest_path_cache.clear();
        }
    }

    /// Resolve `specifier` at an absolute path to a `directory`.
    ///
    /// A specifier is the string passed to require or import, i.e. `require("specifier")` or `import "specifier"`.
    ///
    /// `directory` must be an **absolute** path to a directory where the specifier is resolved against.
    /// For CommonJS modules, it is the `__dirname` variable that contains the absolute path to the folder containing current module.
    /// For ECMAScript modules, it is the value of `import.meta.url`.
    ///
    /// # Errors
    ///
    /// * See [ResolveError]
    pub async fn resolve<P: Send + AsRef<Path>>(
        &self,
        directory: P,
        specifier: &str,
    ) -> Result<Resolution, ResolveError> {
        let mut ctx = Ctx::default();
        self.resolve_tracing(directory.as_ref(), specifier, &mut ctx).await
    }

    /// Resolve `specifier` at absolute `path` with [ResolveContext]
    ///
    /// # Errors
    ///
    /// * See [ResolveError]
    pub async fn resolve_with_context<P: Send + AsRef<Path>>(
        &self,
        directory: P,
        specifier: &str,
        resolve_context: &mut ResolveContext,
    ) -> Result<Resolution, ResolveError> {
        let mut ctx = Ctx::default();
        ctx.init_file_dependencies();
        let result = self.resolve_tracing(directory.as_ref(), specifier, &mut ctx).await;
        if let Some(deps) = &mut ctx.file_dependencies {
            resolve_context.file_dependencies.extend(deps.drain(..));
        }
        if let Some(deps) = &mut ctx.missing_dependencies {
            resolve_context.missing_dependencies.extend(deps.drain(..));
        }
        result
    }

    /// Wrap `resolve_impl` with `tracing` information
    async fn resolve_tracing(
        &self,
        directory: &Path,
        specifier: &str,
        ctx: &mut Ctx,
    ) -> Result<Resolution, ResolveError> {
        let span = tracing::debug_span!("resolve", path = ?directory, specifier = specifier);
        let _enter = span.enter();
        let r = self.resolve_impl(directory, specifier, ctx).await;
        match &r {
            Ok(r) => {
                tracing::debug!(options = ?self.options, ret = ?r.path);
            }
            Err(err) => {
                tracing::debug!(options = ?self.options, err = ?err);
            }
        };
        r
    }

    async fn resolve_impl(
        &self,
        path: &Path,
        specifier: &str,
        ctx: &mut Ctx,
    ) -> Result<Resolution, ResolveError> {
        ctx.with_fully_specified(self.options.fully_specified);
        let cached_path = self.cache.value(path);
        let cached_path = self.require(&cached_path, specifier, ctx).await?;
        let path = self.load_realpath(&cached_path).await?;

        let package_json =
            cached_path.find_package_json(&self.cache.fs, &self.options, ctx).await?;
        if let Some(package_json) = &package_json {
            // path must be inside the package.
            debug_assert!(path.starts_with(package_json.directory()));
        }
        Ok(Resolution {
            path,
            query: ctx.query.take(),
            fragment: ctx.fragment.take(),
            package_json,
        })
    }

    /// require(X) from module at path Y
    ///
    /// X: specifier
    /// Y: path
    ///
    /// <https://nodejs.org/api/modules.html#all-together>
    fn require<'a>(
        &'a self,
        cached_path: &'a CachedPath,
        specifier: &'a str,
        ctx: &'a mut Ctx,
    ) -> BoxFuture<'a, Result<CachedPath, ResolveError>> {
        let fut = async move {
            ctx.test_for_infinite_recursion()?;

            // enhanced-resolve: parse
            let (parsed, try_fragment_as_path) =
                self.load_parse(cached_path, specifier, ctx).await?;
            if let Some(path) = try_fragment_as_path {
                return Ok(path);
            }

            self.require_without_parse(cached_path, parsed.path(), ctx).await
        };
        Box::pin(fut)
    }

    async fn require_without_parse(
        &self,
        cached_path: &CachedPath,
        specifier: &str,
        ctx: &mut Ctx,
    ) -> Result<CachedPath, ResolveError> {
        // tsconfig-paths
        if let Some(path) =
            self.load_tsconfig_paths(cached_path, specifier, &mut Ctx::default()).await?
        {
            return Ok(path);
        }

        // enhanced-resolve: try alias
        if let Some(path) =
            self.load_alias(cached_path, specifier, &self.options.alias, ctx).await?
        {
            return Ok(path);
        }

        let result = match Path::new(specifier).components().next() {
            // 2. If X begins with '/'
            Some(Component::RootDir | Component::Prefix(_)) => {
                self.require_absolute(cached_path, specifier, ctx).await
            }
            // 3. If X begins with './' or '/' or '../'
            Some(Component::CurDir | Component::ParentDir) => {
                self.require_relative(cached_path, specifier, ctx).await
            }
            // 4. If X begins with '#'
            Some(Component::Normal(_)) if specifier.as_bytes()[0] == b'#' => {
                self.require_hash(cached_path, specifier, ctx).await
            }
            _ => {
                // 1. If X is a core module,
                //   a. return the core module
                //   b. STOP
                self.require_core(specifier)?;

                // (ESM) 5. Otherwise,
                // Note: specifier is now a bare specifier.
                // Set resolved the result of PACKAGE_RESOLVE(specifier, parentURL).
                self.require_bare(cached_path, specifier, ctx).await
            }
        };

        match result {
            Ok(_) => result,
            Err(err) => {
                if err.is_ignore() {
                    return Err(err);
                }
                // enhanced-resolve: try fallback
                self.load_alias(cached_path, specifier, &self.options.fallback, ctx)
                    .await
                    .and_then(|value| value.ok_or(err))
            }
        }
    }

    // PACKAGE_RESOLVE(packageSpecifier, parentURL)
    // 3. If packageSpecifier is a Node.js builtin module name, then
    //   1. Return the string "node:" concatenated with packageSpecifier.
    fn require_core(&self, specifier: &str) -> Result<(), ResolveError> {
        if self.options.builtin_modules {
            let starts_with_node = specifier.starts_with("node:");
            if starts_with_node || NODEJS_BUILTINS.binary_search(&specifier).is_ok() {
                let mut specifier = specifier.to_string();
                if !starts_with_node {
                    specifier = format!("node:{specifier}");
                }
                return Err(ResolveError::Builtin(specifier));
            }
        }
        Ok(())
    }

    async fn require_absolute(
        &self,
        cached_path: &CachedPath,
        specifier: &str,
        ctx: &mut Ctx,
    ) -> Result<CachedPath, ResolveError> {
        // Make sure only path prefixes gets called
        debug_assert!(Path::new(specifier)
            .components()
            .next()
            .is_some_and(|c| matches!(c, Component::RootDir | Component::Prefix(_))));
        if !self.options.prefer_relative && self.options.prefer_absolute {
            if let Ok(path) =
                self.load_package_self_or_node_modules(cached_path, specifier, ctx).await
            {
                return Ok(path);
            }
        }
        if let Some(path) = self.load_roots(specifier, ctx).await {
            return Ok(path);
        }
        // 2. If X begins with '/'
        //   a. set Y to be the file system root
        let path = self.cache.value(Path::new(specifier));
        if let Some(path) = self.load_as_file_or_directory(&path, specifier, ctx).await? {
            return Ok(path);
        }
        Err(ResolveError::NotFound(specifier.to_string()))
    }

    // 3. If X begins with './' or '/' or '../'
    async fn require_relative(
        &self,
        cached_path: &CachedPath,
        specifier: &str,
        ctx: &mut Ctx,
    ) -> Result<CachedPath, ResolveError> {
        // Make sure only relative or normal paths gets called
        debug_assert!(Path::new(specifier).components().next().is_some_and(|c| matches!(
            c,
            Component::CurDir | Component::ParentDir | Component::Normal(_)
        )));
        let path = cached_path.path().normalize_with(specifier);
        let cached_path = self.cache.value(&path);
        // a. LOAD_AS_FILE(Y + X)
        // b. LOAD_AS_DIRECTORY(Y + X)
        if let Some(path) = self.load_as_file_or_directory(&cached_path, specifier, ctx).await? {
            return Ok(path);
        }
        // c. THROW "not found"
        Err(ResolveError::NotFound(specifier.to_string()))
    }

    async fn require_hash(
        &self,
        cached_path: &CachedPath,
        specifier: &str,
        ctx: &mut Ctx,
    ) -> Result<CachedPath, ResolveError> {
        debug_assert_eq!(specifier.chars().next(), Some('#'));
        // a. LOAD_PACKAGE_IMPORTS(X, dirname(Y))
        if let Some(path) = self.load_package_imports(cached_path, specifier, ctx).await? {
            return Ok(path);
        }
        self.load_package_self_or_node_modules(cached_path, specifier, ctx).await
    }

    async fn require_bare(
        &self,
        cached_path: &CachedPath,
        specifier: &str,
        ctx: &mut Ctx,
    ) -> Result<CachedPath, ResolveError> {
        // Make sure no other path prefixes gets called
        debug_assert!(Path::new(specifier)
            .components()
            .next()
            .is_some_and(|c| matches!(c, Component::Normal(_))));
        if self.options.prefer_relative {
            if let Ok(path) = self.require_relative(cached_path, specifier, ctx).await {
                return Ok(path);
            }
        }
        self.load_package_self_or_node_modules(cached_path, specifier, ctx).await
    }

    /// enhanced-resolve: ParsePlugin.
    ///
    /// It's allowed to escape # as \0# to avoid parsing it as fragment.
    /// enhanced-resolve will try to resolve requests containing `#` as path and as fragment,
    /// so it will automatically figure out if `./some#thing` means `.../some.js#thing` or `.../some#thing.js`.
    /// When a # is resolved as path it will be escaped in the result. Here: `.../some\0#thing.js`.
    ///
    /// <https://github.com/webpack/enhanced-resolve#escaping>
    async fn load_parse<'s>(
        &self,
        cached_path: &CachedPath,
        specifier: &'s str,
        ctx: &mut Ctx,
    ) -> Result<(Specifier<'s>, Option<CachedPath>), ResolveError> {
        let parsed = Specifier::parse(specifier).map_err(ResolveError::Specifier)?;
        ctx.with_query_fragment(parsed.query, parsed.fragment);

        // There is an edge-case where a request with # can be a path or a fragment -> try both
        if ctx.fragment.is_some() && ctx.query.is_none() {
            let specifier = parsed.path();
            let fragment = ctx.fragment.take().unwrap();
            let path = format!("{specifier}{fragment}");
            if let Ok(path) = self.require_without_parse(cached_path, &path, ctx).await {
                return Ok((parsed, Some(path)));
            }
            ctx.fragment.replace(fragment);
        }
        Ok((parsed, None))
    }

    async fn load_package_self_or_node_modules(
        &self,
        cached_path: &CachedPath,
        specifier: &str,
        ctx: &mut Ctx,
    ) -> Result<CachedPath, ResolveError> {
        let (_, subpath) = Self::parse_package_specifier(specifier);
        if subpath.is_empty() {
            ctx.with_fully_specified(false);
        }
        // 5. LOAD_PACKAGE_SELF(X, dirname(Y))
        if let Some(path) = self.load_package_self(cached_path, specifier, ctx).await? {
            return Ok(path);
        }
        // 6. LOAD_NODE_MODULES(X, dirname(Y))
        if let Some(path) = self.load_node_modules(cached_path, specifier, ctx).await? {
            return Ok(path);
        }
        // 7. THROW "not found"
        Err(ResolveError::NotFound(specifier.to_string()))
    }

    /// LOAD_PACKAGE_IMPORTS(X, DIR)
    async fn load_package_imports(
        &self,
        cached_path: &CachedPath,
        specifier: &str,
        ctx: &mut Ctx,
    ) -> ResolveResult {
        // 1. Find the closest package scope SCOPE to DIR.
        // 2. If no scope was found, return.
        let Some(package_json) =
            cached_path.find_package_json(&self.cache.fs, &self.options, ctx).await?
        else {
            return Ok(None);
        };
        // 3. If the SCOPE/package.json "imports" is null or undefined, return.
        // 4. let MATCH = PACKAGE_IMPORTS_RESOLVE(X, pathToFileURL(SCOPE), ["node", "require"]) defined in the ESM resolver.
        if let Some(path) = self.package_imports_resolve(specifier, &package_json, ctx).await? {
            // 5. RESOLVE_ESM_MATCH(MATCH).
            return self.resolve_esm_match(specifier, &path, ctx).await;
        }
        Ok(None)
    }

    async fn load_as_file(&self, cached_path: &CachedPath, ctx: &mut Ctx) -> ResolveResult {
        // enhanced-resolve feature: extension_alias
        if let Some(path) = self.load_extension_alias(cached_path, ctx).await? {
            return Ok(Some(path));
        }
        if self.options.enforce_extension.is_disabled() {
            // 1. If X is a file, load X as its file extension format. STOP
            if let Some(path) = self.load_alias_or_file(cached_path, ctx).await? {
                return Ok(Some(path));
            }
        }
        // 2. If X.js is a file, load X.js as JavaScript text. STOP
        // 3. If X.json is a file, parse X.json to a JavaScript Object. STOP
        // 4. If X.node is a file, load X.node as binary addon. STOP
        if let Some(path) = self.load_extensions(cached_path, &self.options.extensions, ctx).await?
        {
            return Ok(Some(path));
        }
        Ok(None)
    }

    async fn load_as_directory(&self, cached_path: &CachedPath, ctx: &mut Ctx) -> ResolveResult {
        // TODO: Only package.json is supported, so warn about having other values
        // Checking for empty files is needed for omitting checks on package.json
        // 1. If X/package.json is a file,
        if !self.options.description_files.is_empty() {
            // a. Parse X/package.json, and look for "main" field.
            if let Some(package_json) =
                cached_path.package_json(&self.cache.fs, &self.options, ctx).await?
            {
                // b. If "main" is a falsy value, GOTO 2.
                for main_field in package_json.main_fields(&self.options.main_fields) {
                    // ref https://github.com/webpack/enhanced-resolve/blob/main/lib/MainFieldPlugin.js#L66-L67
                    let main_field =
                        if main_field.starts_with("./") || main_field.starts_with("../") {
                            Cow::Borrowed(main_field)
                        } else {
                            Cow::Owned(format!("./{main_field}"))
                        };

                    // c. let M = X + (json main field)
                    let main_field_path = cached_path.path().normalize_with(main_field.as_ref());
                    // d. LOAD_AS_FILE(M)
                    let cached_path = self.cache.value(&main_field_path);
                    if let Ok(Some(path)) = self.load_as_file(&cached_path, ctx).await {
                        return Ok(Some(path));
                    }
                    // e. LOAD_INDEX(M)
                    if let Some(path) = self.load_index(&cached_path, ctx).await? {
                        return Ok(Some(path));
                    }
                }
                // f. LOAD_INDEX(X) DEPRECATED
                // g. THROW "not found"
            }
        }
        // 2. LOAD_INDEX(X)
        self.load_index(cached_path, ctx).await
    }

    async fn load_as_file_or_directory(
        &self,
        cached_path: &CachedPath,
        specifier: &str,
        ctx: &mut Ctx,
    ) -> ResolveResult {
        if self.options.resolve_to_context {
            return Ok(cached_path.is_dir(&self.cache.fs, ctx).await.then(|| cached_path.clone()));
        }
        if !specifier.ends_with('/') {
            if let Some(path) = self.load_as_file(cached_path, ctx).await? {
                return Ok(Some(path));
            }
        }
        if cached_path.is_dir(&self.cache.fs, ctx).await {
            if let Some(path) = self.load_as_directory(cached_path, ctx).await? {
                return Ok(Some(path));
            }
        }
        Ok(None)
    }

    async fn load_extensions(
        &self,
        path: &CachedPath,
        extensions: &[String],
        ctx: &mut Ctx,
    ) -> ResolveResult {
        if ctx.fully_specified {
            return Ok(None);
        }
        let path = path.path().as_os_str();
        for extension in extensions {
            let mut path_with_extension = path.to_os_string();
            path_with_extension.reserve_exact(extension.len());
            path_with_extension.push(extension);
            let cached_path = self.cache.value(Path::new(&path_with_extension));
            if let Some(path) = self.load_alias_or_file(&cached_path, ctx).await? {
                return Ok(Some(path));
            }
        }
        Ok(None)
    }

    async fn load_realpath(&self, cached_path: &CachedPath) -> Result<PathBuf, ResolveError> {
        if self.options.symlinks {
            cached_path.realpath(&self.cache.fs).await.map_err(ResolveError::from)
        } else {
            Ok(cached_path.to_path_buf())
        }
    }

    fn check_restrictions(&self, path: &Path) -> bool {
        // https://github.com/webpack/enhanced-resolve/blob/a998c7d218b7a9ec2461fc4fddd1ad5dd7687485/lib/RestrictionsPlugin.js#L19-L24
        fn is_inside(path: &Path, parent: &Path) -> bool {
            if !path.starts_with(parent) {
                return false;
            }
            if path.as_os_str().len() == parent.as_os_str().len() {
                return true;
            }
            path.strip_prefix(parent).is_ok_and(|p| p == Path::new("./"))
        }
        for restriction in &self.options.restrictions {
            match restriction {
                Restriction::Path(restricted_path) => {
                    if !is_inside(path, restricted_path) {
                        return false;
                    }
                }
                Restriction::Fn(f) => {
                    if !f(path) {
                        return false;
                    }
                }
            }
        }
        true
    }

    async fn load_index(&self, cached_path: &CachedPath, ctx: &mut Ctx) -> ResolveResult {
        for main_file in &self.options.main_files {
            let main_path = cached_path.path().normalize_with(main_file);
            let cached_path = self.cache.value(&main_path);
            if self.options.enforce_extension.is_disabled() {
                if let Some(path) = self.load_alias_or_file(&cached_path, ctx).await? {
                    if self.check_restrictions(path.path()) {
                        return Ok(Some(path));
                    }
                }
            }
            // 1. If X/index.js is a file, load X/index.js as JavaScript text. STOP
            // 2. If X/index.json is a file, parse X/index.json to a JavaScript object. STOP
            // 3. If X/index.node is a file, load X/index.node as binary addon. STOP
            if let Some(path) =
                self.load_extensions(&cached_path, &self.options.extensions, ctx).await?
            {
                return Ok(Some(path));
            }
        }
        Ok(None)
    }

    async fn load_alias_or_file(&self, cached_path: &CachedPath, ctx: &mut Ctx) -> ResolveResult {
        if !self.options.alias_fields.is_empty() {
            if let Some(package_json) =
                cached_path.find_package_json(&self.cache.fs, &self.options, ctx).await?
            {
                if let Some(path) =
                    self.load_browser_field(cached_path, None, &package_json, ctx).await?
                {
                    return Ok(Some(path));
                }
            }
        }
        // enhanced-resolve: try file as alias
        let alias_specifier = cached_path.path().to_string_lossy();
        if let Some(path) =
            self.load_alias(cached_path, &alias_specifier, &self.options.alias, ctx).await?
        {
            return Ok(Some(path));
        }
        if cached_path.is_file(&self.cache.fs, ctx).await
            && self.check_restrictions(cached_path.path())
        {
            return Ok(Some(cached_path.clone()));
        }
        Ok(None)
    }

    async fn load_node_modules(
        &self,
        cached_path: &CachedPath,
        specifier: &str,
        ctx: &mut Ctx,
    ) -> ResolveResult {
        #[cfg(feature = "yarn_pnp")]
        {
            if self.options.enable_pnp {
                if let Some(resolved_path) = self.load_pnp(cached_path, specifier, ctx).await? {
                    return Ok(Some(resolved_path));
                }
            }
        }

        let (package_name, subpath) = Self::parse_package_specifier(specifier);
        // 1. let DIRS = NODE_MODULES_PATHS(START)
        // 2. for each DIR in DIRS:
        for module_name in &self.options.modules {
            for cached_path in std::iter::successors(Some(cached_path), |p| p.parent()) {
                // Skip if /path/to/node_modules does not exist
                if !cached_path.is_dir(&self.cache.fs, ctx).await {
                    continue;
                }

                let Some(cached_path) =
                    self.get_module_directory(cached_path, module_name, ctx).await
                else {
                    continue;
                };
                // Optimize node_modules lookup by inspecting whether the package exists
                // From LOAD_PACKAGE_EXPORTS(X, DIR)
                // 1. Try to interpret X as a combination of NAME and SUBPATH where the name
                //    may have a @scope/ prefix and the subpath begins with a slash (`/`).
                if !package_name.is_empty() {
                    let package_path = cached_path.path().normalize_with(package_name);
                    let cached_path = self.cache.value(&package_path);
                    // Try foo/node_modules/package_name
                    if cached_path.is_dir(&self.cache.fs, ctx).await {
                        // a. LOAD_PACKAGE_EXPORTS(X, DIR)
                        if let Some(path) =
                            self.load_package_exports(specifier, subpath, &cached_path, ctx).await?
                        {
                            return Ok(Some(path));
                        }
                    } else {
                        // foo/node_modules/package_name is not a directory, so useless to check inside it
                        if !subpath.is_empty() {
                            continue;
                        }
                        // Skip if the directory lead to the scope package does not exist
                        // i.e. `foo/node_modules/@scope` is not a directory for `foo/node_modules/@scope/package`
                        if package_name.starts_with('@') {
                            if let Some(path) = cached_path.parent() {
                                if !path.is_dir(&self.cache.fs, ctx).await {
                                    continue;
                                }
                            }
                        }
                    }
                }

                // Try as file or directory for all other cases
                // b. LOAD_AS_FILE(DIR/X)
                // c. LOAD_AS_DIRECTORY(DIR/X)
                let node_module_file = cached_path.path().normalize_with(specifier);
                let cached_path = self.cache.value(&node_module_file);
                if let Some(path) =
                    self.load_as_file_or_directory(&cached_path, specifier, ctx).await?
                {
                    return Ok(Some(path));
                }
            }
        }
        Ok(None)
    }

    #[cfg(feature = "yarn_pnp")]
    fn find_pnp_manifest(
        &self,
        cached_path: &CachedPath,
    ) -> Ref<'_, CachedPath, Option<pnp::Manifest>> {
        let base_path = cached_path.to_path_buf();

        let cached_manifest_path =
            self.pnp_manifest_path_cache.entry(base_path.clone()).or_insert_with(|| {
                pnp::find_closest_pnp_manifest_path(&base_path).map(|p| self.cache.value(&p))
            });

        let cache_key = cached_manifest_path.as_ref().unwrap_or(cached_path);

        tracing::debug!("use manifest path: {:?}", cache_key.path());

        let entry = self
            .pnp_manifest_content_cache
            .entry(cache_key.clone())
            .or_insert_with(|| pnp::load_pnp_manifest(cache_key.path()).ok());

        entry.downgrade()
    }

    #[cfg(feature = "yarn_pnp")]
    async fn load_pnp(
        &self,
        cached_path: &CachedPath,
        specifier: &str,
        ctx: &mut Ctx,
    ) -> Result<Option<CachedPath>, ResolveError> {
        let pnp_manifest = self.find_pnp_manifest(cached_path);

        if let Some(pnp_manifest) = pnp_manifest.as_ref() {
            // `resolve_to_unqualified` requires a trailing slash
            let mut path = cached_path.to_path_buf();
            path.push("");

            let resolution =
                pnp::resolve_to_unqualified_via_manifest(pnp_manifest, specifier, &path);

            tracing::debug!("pnp resolve unqualified as: {:?}", resolution);

            match resolution {
                Ok(pnp::Resolution::Resolved(path, subpath)) => {
                    let cached_path = self.cache.value(&path);

                    let export_resolution =
                        self.load_package_self(&cached_path, specifier, ctx).await?;
                    // can be found in pnp cached folder
                    if export_resolution.is_some() {
                        return Ok(export_resolution);
                    }

                    let inner_request = subpath.map_or_else(
                        || ".".to_string(),
                        |mut p| {
                            p.insert_str(0, "./");
                            p
                        },
                    );
                    let inner_resolver = self.clone_with_options(self.options().clone());

                    // try as file or directory `path` in the pnp folder
                    let Ok(inner_resolution) = inner_resolver.resolve(&path, &inner_request).await
                    else {
                        return Err(ResolveError::NotFound(specifier.to_string()));
                    };

                    Ok(Some(self.cache.value(inner_resolution.path())))
                }

                Ok(pnp::Resolution::Skipped) => Ok(None),
                Err(_) => Err(ResolveError::NotFound(specifier.to_string())),
            }
        } else {
            Ok(None)
        }
    }

    async fn get_module_directory(
        &self,
        cached_path: &CachedPath,
        module_name: &str,
        ctx: &mut Ctx,
    ) -> Option<CachedPath> {
        if module_name == "node_modules" {
            cached_path.cached_node_modules(&self.cache, ctx).await
        } else if cached_path.path().components().next_back()
            == Some(Component::Normal(OsStr::new(module_name)))
        {
            Some(cached_path.clone())
        } else {
            cached_path.module_directory(module_name, &self.cache, ctx).await
        }
    }

    async fn load_package_exports(
        &self,
        specifier: &str,
        subpath: &str,
        cached_path: &CachedPath,
        ctx: &mut Ctx,
    ) -> ResolveResult {
        // 2. If X does not match this pattern or DIR/NAME/package.json is not a file,
        //    return.
        let Some(package_json) =
            cached_path.package_json(&self.cache.fs, &self.options, ctx).await?
        else {
            return Ok(None);
        };
        // 3. Parse DIR/NAME/package.json, and look for "exports" field.
        // 4. If "exports" is null or undefined, return.
        // 5. let MATCH = PACKAGE_EXPORTS_RESOLVE(pathToFileURL(DIR/NAME), "." + SUBPATH,
        //    `package.json` "exports", ["node", "require"]) defined in the ESM resolver.
        // Note: The subpath is not prepended with a dot on purpose
        for exports in package_json.exports_fields(&self.options.exports_fields) {
            if let Some(path) = self
                .package_exports_resolve(cached_path.path(), &format!(".{subpath}"), exports, ctx)
                .await?
            {
                // 6. RESOLVE_ESM_MATCH(MATCH)
                return self.resolve_esm_match(specifier, &path, ctx).await;
            };
        }
        Ok(None)
    }

    async fn load_package_self(
        &self,
        cached_path: &CachedPath,
        specifier: &str,
        ctx: &mut Ctx,
    ) -> ResolveResult {
        // 1. Find the closest package scope SCOPE to DIR.
        // 2. If no scope was found, return.
        let Some(package_json) =
            cached_path.find_package_json(&self.cache.fs, &self.options, ctx).await?
        else {
            return Ok(None);
        };
        // 3. If the SCOPE/package.json "exports" is null or undefined, return.
        // 4. If the SCOPE/package.json "name" is not the first segment of X, return.
        if let Some(subpath) = package_json
            .name
            .as_ref()
            .and_then(|package_name| Self::strip_package_name(specifier, package_name))
        {
            // 5. let MATCH = PACKAGE_EXPORTS_RESOLVE(pathToFileURL(SCOPE),
            // "." + X.slice("name".length), `package.json` "exports", ["node", "require"])
            // defined in the ESM resolver.
            let package_url = package_json.directory();
            // Note: The subpath is not prepended with a dot on purpose
            // because `package_exports_resolve` matches subpath without the leading dot.
            for exports in package_json.exports_fields(&self.options.exports_fields) {
                if let Some(cached_path) = self
                    .package_exports_resolve(package_url, &format!(".{subpath}"), exports, ctx)
                    .await?
                {
                    // 6. RESOLVE_ESM_MATCH(MATCH)
                    return self.resolve_esm_match(specifier, &cached_path, ctx).await;
                }
            }
        }
        self.load_browser_field(cached_path, Some(specifier), &package_json, ctx).await
    }

    /// RESOLVE_ESM_MATCH(MATCH)
    async fn resolve_esm_match(
        &self,
        specifier: &str,
        cached_path: &CachedPath,
        ctx: &mut Ctx,
    ) -> ResolveResult {
        // 1. let RESOLVED_PATH = fileURLToPath(MATCH)
        // 2. If the file at RESOLVED_PATH exists, load RESOLVED_PATH as its extension format. STOP
        //
        // Non-compliant ESM can result in a directory, so directory is tried as well.
        if let Some(path) = self.load_as_file_or_directory(cached_path, "", ctx).await? {
            return Ok(Some(path));
        }

        let mut path_str = cached_path.path().to_str();

        // 3. If the RESOLVED_PATH contains `?``, it could be a path with query
        //    so try to resolve it as a file or directory without the query,
        //    but also `?` is a valid character in a path, so we should try from right to left.
        while let Some(s) = path_str {
            if let Some((before, _)) = s.rsplit_once('?') {
                if (self
                    .load_as_file_or_directory(&self.cache.value(Path::new(before)), "", ctx)
                    .await?)
                    .is_some()
                {
                    return Ok(Some(cached_path.clone()));
                }
                path_str = Some(before);
            } else {
                break;
            }
        }

        // 3. THROW "not found"
        Err(ResolveError::NotFound(specifier.to_string()))
    }

    /// enhanced-resolve: AliasFieldPlugin for [ResolveOptions::alias_fields]
    async fn load_browser_field(
        &self,
        cached_path: &CachedPath,
        module_specifier: Option<&str>,
        package_json: &PackageJson,
        ctx: &mut Ctx,
    ) -> ResolveResult {
        let path = cached_path.path();
        let Some(new_specifier) = package_json.resolve_browser_field(
            path,
            module_specifier,
            &self.options.alias_fields,
        )?
        else {
            return Ok(None);
        };
        // Abort when resolving recursive module
        if module_specifier.is_some_and(|s| s == new_specifier) {
            return Ok(None);
        }
        if ctx.resolving_alias.as_ref().is_some_and(|s| s == new_specifier) {
            // Complete when resolving to self `{"./a.js": "./a.js"}`
            if new_specifier.strip_prefix("./").filter(|s| path.ends_with(Path::new(s))).is_some() {
                return if cached_path.is_file(&self.cache.fs, ctx).await {
                    if self.check_restrictions(cached_path.path()) {
                        Ok(Some(cached_path.clone()))
                    } else {
                        Ok(None)
                    }
                } else {
                    Err(ResolveError::NotFound(new_specifier.to_string()))
                };
            }
            return Err(ResolveError::Recursion);
        }
        ctx.with_resolving_alias(new_specifier.to_string());
        ctx.with_fully_specified(false);
        let cached_path = self.cache.value(package_json.directory());
        self.require(&cached_path, new_specifier, ctx).await.map(Some)
    }

    /// enhanced-resolve: AliasPlugin for [ResolveOptions::alias] and [ResolveOptions::fallback].
    async fn load_alias(
        &self,
        cached_path: &CachedPath,
        specifier: &str,
        aliases: &Alias,
        ctx: &mut Ctx,
    ) -> ResolveResult {
        for (alias_key_raw, specifiers) in aliases {
            let alias_key = if let Some(alias_key) = alias_key_raw.strip_suffix('$') {
                if alias_key != specifier {
                    continue;
                }
                alias_key
            } else {
                let strip_package_name = Self::strip_package_name(specifier, alias_key_raw);
                if strip_package_name.is_none() {
                    continue;
                }
                alias_key_raw
            };
            // It should stop resolving when all of the tried alias values
            // failed to resolve.
            // <https://github.com/webpack/enhanced-resolve/blob/570337b969eee46120a18b62b72809a3246147da/lib/AliasPlugin.js#L65>
            let mut should_stop = false;
            for r in specifiers {
                match r {
                    AliasValue::Path(alias_value) => {
                        if let Some(path) = self
                            .load_alias_value(
                                cached_path,
                                alias_key,
                                alias_value,
                                specifier,
                                ctx,
                                &mut should_stop,
                            )
                            .await?
                        {
                            return Ok(Some(path));
                        }
                    }
                    AliasValue::Ignore => {
                        let path = cached_path.path().normalize_with(alias_key);
                        return Err(ResolveError::Ignored(path));
                    }
                }
            }
            if should_stop {
                return Err(ResolveError::MatchedAliasNotFound(
                    specifier.to_string(),
                    alias_key.to_string(),
                ));
            }
        }
        Ok(None)
    }

    async fn load_alias_value(
        &self,
        cached_path: &CachedPath,
        alias_key: &str,
        alias_value: &str,
        request: &str,
        ctx: &mut Ctx,
        should_stop: &mut bool,
    ) -> ResolveResult {
        if request != alias_value
            && !request.strip_prefix(alias_value).is_some_and(|prefix| prefix.starts_with('/'))
        {
            let tail = &request[alias_key.len()..];

            let new_specifier = if tail.is_empty() {
                Cow::Borrowed(alias_value)
            } else {
                let alias_path = Path::new(alias_value).normalize();
                // Must not append anything to alias_value if it is a file.
                let alias_value_cached_path = self.cache.value(&alias_path);
                if alias_value_cached_path.is_file(&self.cache.fs, ctx).await {
                    return Ok(None);
                }

                // Remove the leading slash so the final path is concatenated.
                let tail = tail.trim_start_matches(SLASH_START);
                if tail.is_empty() {
                    Cow::Borrowed(alias_value)
                } else {
                    let normalized = alias_path.normalize_with(tail);
                    Cow::Owned(normalized.to_string_lossy().to_string())
                }
            };

            *should_stop = true;
            ctx.with_fully_specified(false);
            return match self.require(cached_path, new_specifier.as_ref(), ctx).await {
                Err(ResolveError::NotFound(_) | ResolveError::MatchedAliasNotFound(_, _)) => {
                    Ok(None)
                }
                Ok(path) => return Ok(Some(path)),
                Err(err) => return Err(err),
            };
        }
        Ok(None)
    }

    /// Given an extension alias map `{".js": [".ts", ".js"]}`,
    /// load the mapping instead of the provided extension
    ///
    /// This is an enhanced-resolve feature
    ///
    /// # Errors
    ///
    /// * [ResolveError::ExtensionAlias]: When all of the aliased extensions are not found
    async fn load_extension_alias(&self, cached_path: &CachedPath, ctx: &mut Ctx) -> ResolveResult {
        if self.options.extension_alias.is_empty() {
            return Ok(None);
        }
        let Some(path_extension) = cached_path.path().extension() else {
            return Ok(None);
        };
        let Some((_, extensions)) = self
            .options
            .extension_alias
            .iter()
            .find(|(ext, _)| OsStr::new(ext.trim_start_matches('.')) == path_extension)
        else {
            return Ok(None);
        };
        let path = cached_path.path();
        let Some(filename) = path.file_name() else { return Ok(None) };
        let path_without_extension = path.with_extension("");

        ctx.with_fully_specified(true);
        for extension in extensions {
            let mut path_with_extension = path_without_extension.clone().into_os_string();
            path_with_extension.reserve_exact(extension.len());
            path_with_extension.push(extension);
            let cached_path = self.cache.value(Path::new(&path_with_extension));
            if let Some(path) = self.load_alias_or_file(&cached_path, ctx).await? {
                ctx.with_fully_specified(false);
                return Ok(Some(path));
            }
        }
        // Bail if path is module directory such as `ipaddr.js`
        if !cached_path.is_file(&self.cache.fs, ctx).await
            || !self.check_restrictions(cached_path.path())
        {
            ctx.with_fully_specified(false);
            return Ok(None);
        }
        // Create a meaningful error message.
        let dir = path.parent().unwrap().to_path_buf();
        let filename_without_extension = Path::new(filename).with_extension("");
        let filename_without_extension = filename_without_extension.to_string_lossy();
        let files = extensions
            .iter()
            .map(|ext| format!("{filename_without_extension}{ext}"))
            .collect::<Vec<_>>()
            .join(",");
        Err(ResolveError::ExtensionAlias(filename.to_string_lossy().to_string(), files, dir))
    }

    /// enhanced-resolve: RootsPlugin
    ///
    /// A list of directories where requests of server-relative URLs (starting with '/') are resolved,
    /// defaults to context configuration option.
    ///
    /// On non-Windows systems these requests are resolved as an absolute path first.
    async fn load_roots(&self, specifier: &str, ctx: &mut Ctx) -> Option<CachedPath> {
        if self.options.roots.is_empty() {
            return None;
        }
        if let Some(specifier) = specifier.strip_prefix(SLASH_START) {
            for root in &self.options.roots {
                let cached_path = self.cache.value(root);
                if let Ok(path) = self.require_relative(&cached_path, specifier, ctx).await {
                    return Some(path);
                }
            }
        }
        None
    }

    async fn load_tsconfig_paths(
        &self,
        cached_path: &CachedPath,
        specifier: &str,
        ctx: &mut Ctx,
    ) -> ResolveResult {
        let Some(tsconfig_options) = &self.options.tsconfig else {
            return Ok(None);
        };
        let tsconfig = self
            .load_tsconfig(
                /* root */ true,
                &tsconfig_options.config_file,
                &tsconfig_options.references,
            )
            .await?;
        let paths = tsconfig.resolve(cached_path.path(), specifier);
        for path in paths {
            let cached_path = self.cache.value(&path);
            if let Ok(path) = self.require_relative(&cached_path, ".", ctx).await {
                return Ok(Some(path));
            }
        }
        Ok(None)
    }

    fn load_tsconfig<'a>(
        &'a self,
        root: bool,
        path: &'a Path,
        references: &'a TsconfigReferences,
    ) -> BoxFuture<'a, Result<Arc<TsConfig>, ResolveError>> {
        let fut = async move {
            self.cache
                .tsconfig(root, path, |mut tsconfig| async move {
                    let directory = self.cache.value(tsconfig.directory());
                    tracing::trace!(tsconfig = ?tsconfig, "load_tsconfig");

                    // Extend tsconfig
                    if let Some(extends) = &tsconfig.extends {
                        let extended_tsconfig_paths = match extends {
                            ExtendsField::Single(s) => {
                                vec![
                                    self.get_extended_tsconfig_path(&directory, &tsconfig, s)
                                        .await?,
                                ]
                            }
                            ExtendsField::Multiple(specifiers) => {
                                try_join_all(specifiers.iter().map(|s| {
                                    self.get_extended_tsconfig_path(&directory, &tsconfig, s)
                                }))
                                .await?
                            }
                        };
                        for extended_tsconfig_path in extended_tsconfig_paths {
                            let extended_tsconfig = self
                                .load_tsconfig(
                                    /* root */ false,
                                    &extended_tsconfig_path,
                                    &TsconfigReferences::Disabled,
                                )
                                .await?;
                            tsconfig.extend_tsconfig(&extended_tsconfig);
                        }
                    }

                    // Load project references
                    match references {
                        TsconfigReferences::Disabled => {
                            tsconfig.references.drain(..);
                        }
                        TsconfigReferences::Auto => {}
                        TsconfigReferences::Paths(paths) => {
                            tsconfig.references = paths
                                .iter()
                                .map(|path| ProjectReference { path: path.clone(), tsconfig: None })
                                .collect();
                        }
                    }
                    if !tsconfig.references.is_empty() {
                        let directory = tsconfig.directory().to_path_buf();
                        for reference in &mut tsconfig.references {
                            let reference_tsconfig_path = directory.normalize_with(&reference.path);
                            let tsconfig = self
                                .cache
                                .tsconfig(
                                    /* root */ true,
                                    &reference_tsconfig_path,
                                    |reference_tsconfig| async {
                                        if reference_tsconfig.path == tsconfig.path {
                                            return Err(ResolveError::TsconfigSelfReference(
                                                reference_tsconfig.path.clone(),
                                            ));
                                        }
                                        Ok(reference_tsconfig)
                                    },
                                )
                                .await?;
                            reference.tsconfig.replace(tsconfig);
                        }
                    }

                    Ok(tsconfig)
                })
                .await
        };
        Box::pin(fut)
    }

    async fn get_extended_tsconfig_path(
        &self,
        directory: &CachedPath,
        tsconfig: &TsConfig,
        specifier: &str,
    ) -> Result<PathBuf, ResolveError> {
        match specifier.as_bytes().first() {
            None => Err(ResolveError::Specifier(SpecifierError::Empty(specifier.to_string()))),
            Some(b'/') => Ok(PathBuf::from(specifier)),
            Some(b'.') => Ok(tsconfig.directory().normalize_with(specifier)),
            _ => self
                .clone_with_options(ResolveOptions {
                    description_files: vec![],
                    extensions: vec![".json".into()],
                    main_files: vec!["tsconfig.json".into()],
                    ..ResolveOptions::default()
                })
                .load_package_self_or_node_modules(directory, specifier, &mut Ctx::default())
                .await
                .map(|p| p.to_path_buf())
                .map_err(|err| match err {
                    ResolveError::NotFound(_) => {
                        ResolveError::TsconfigNotFound(PathBuf::from(specifier))
                    }
                    _ => err,
                }),
        }
    }

    /// PACKAGE_RESOLVE(packageSpecifier, parentURL)
    async fn package_resolve(
        &self,
        cached_path: &CachedPath,
        specifier: &str,
        ctx: &mut Ctx,
    ) -> ResolveResult {
        let (package_name, subpath) = Self::parse_package_specifier(specifier);

        // 3. If packageSpecifier is a Node.js builtin module name, then
        //   1. Return the string "node:" concatenated with packageSpecifier.
        self.require_core(package_name)?;

        // 11. While parentURL is not the file system root,
        for module_name in &self.options.modules {
            for cached_path in std::iter::successors(Some(cached_path), |p| p.parent()) {
                // 1. Let packageURL be the URL resolution of "node_modules/" concatenated with packageSpecifier, relative to parentURL.
                let Some(cached_path) =
                    self.get_module_directory(cached_path, module_name, ctx).await
                else {
                    continue;
                };
                // 2. Set parentURL to the parent folder URL of parentURL.
                let package_path = cached_path.path().normalize_with(package_name);
                let cached_path = self.cache.value(&package_path);
                // 3. If the folder at packageURL does not exist, then
                //   1. Continue the next loop iteration.
                if cached_path.is_dir(&self.cache.fs, ctx).await {
                    // 4. Let pjson be the result of READ_PACKAGE_JSON(packageURL).
                    if let Some(package_json) =
                        cached_path.package_json(&self.cache.fs, &self.options, ctx).await?
                    {
                        // 5. If pjson is not null and pjson.exports is not null or undefined, then
                        // 1. Return the result of PACKAGE_EXPORTS_RESOLVE(packageURL, packageSubpath, pjson.exports, defaultConditions).
                        for exports in package_json.exports_fields(&self.options.exports_fields) {
                            if let Some(path) = self
                                .package_exports_resolve(
                                    cached_path.path(),
                                    &format!(".{subpath}"),
                                    exports,
                                    ctx,
                                )
                                .await?
                            {
                                return Ok(Some(path));
                            }
                        }
                        // 6. Otherwise, if packageSubpath is equal to ".", then
                        if subpath == "." {
                            // 1. If pjson.main is a string, then
                            for main_field in package_json.main_fields(&self.options.main_fields) {
                                // 1. Return the URL resolution of main in packageURL.
                                let path = cached_path.path().normalize_with(main_field);
                                let cached_path = self.cache.value(&path);
                                if cached_path.is_file(&self.cache.fs, ctx).await
                                    && self.check_restrictions(cached_path.path())
                                {
                                    return Ok(Some(cached_path.clone()));
                                }
                            }
                        }
                    }
                    let subpath = format!(".{subpath}");
                    ctx.with_fully_specified(false);
                    return self.require(&cached_path, &subpath, ctx).await.map(Some);
                }
            }
        }

        Err(ResolveError::NotFound(specifier.to_string()))
    }

    /// PACKAGE_EXPORTS_RESOLVE(packageURL, subpath, exports, conditions)
    fn package_exports_resolve<'a>(
        &'a self,
        package_url: &'a Path,
        subpath: &'a str,
        exports: &'a JSONValue,
        ctx: &'a mut Ctx,
    ) -> BoxFuture<'a, ResolveResult> {
        let fut = async move {
            let conditions = &self.options.condition_names;
            // 1. If exports is an Object with both a key starting with "." and a key not starting with ".", throw an Invalid Package Configuration error.
            if let JSONValue::Object(map) = exports {
                let mut has_dot = false;
                let mut without_dot = false;
                for key in map.keys() {
                    let starts_with_dot_or_hash = key.starts_with(['.', '#']);
                    has_dot = has_dot || starts_with_dot_or_hash;
                    without_dot = without_dot || !starts_with_dot_or_hash;
                    if has_dot && without_dot {
                        return Err(ResolveError::InvalidPackageConfig(
                            package_url.join("package.json"),
                        ));
                    }
                }
            }
            // 2. If subpath is equal to ".", then
            // Note: subpath is not prepended with a dot when passed in.
            if subpath == "." {
                // enhanced-resolve appends query and fragment when resolving exports field
                // https://github.com/webpack/enhanced-resolve/blob/a998c7d218b7a9ec2461fc4fddd1ad5dd7687485/lib/ExportsFieldPlugin.js#L57-L62
                // This is only need when querying the main export, otherwise ctx is passed through.
                if ctx.query.is_some() || ctx.fragment.is_some() {
                    let query = ctx.query.clone().unwrap_or_default();
                    let fragment = ctx.fragment.clone().unwrap_or_default();
                    return Err(ResolveError::PackagePathNotExported(
                        format!("./{}{query}{fragment}", subpath.trim_start_matches('.')),
                        package_url.join("package.json"),
                    ));
                }
                // 1. Let mainExport be undefined.
                let main_export = match exports {
                    // 2. If exports is a String or Array, or an Object containing no keys starting with ".", then
                    JSONValue::String(_) | JSONValue::Array(_) => {
                        // 1. Set mainExport to exports.
                        Some(exports)
                    }
                    // 3. Otherwise if exports is an Object containing a "." property, then
                    JSONValue::Object(map) => {
                        // 1. Set mainExport to exports["."].
                        map.get(".").map_or_else(
                            || {
                                if map
                                    .keys()
                                    .any(|key| key.starts_with("./") || key.starts_with('#'))
                                {
                                    None
                                } else {
                                    Some(exports)
                                }
                            },
                            Some,
                        )
                    }
                    _ => None,
                };
                // 4. If mainExport is not undefined, then
                if let Some(main_export) = main_export {
                    // 1. Let resolved be the result of PACKAGE_TARGET_RESOLVE( packageURL, mainExport, null, false, conditions).
                    let resolved = self
                        .package_target_resolve(
                            package_url,
                            ".",
                            main_export,
                            None,
                            /* is_imports */ false,
                            conditions,
                            ctx,
                        )
                        .await?;
                    // 2. If resolved is not null or undefined, return resolved.
                    if let Some(path) = resolved {
                        return Ok(Some(path));
                    }
                }
            }
            // 3. Otherwise, if exports is an Object and all keys of exports start with ".", then
            if let JSONValue::Object(exports) = exports {
                // 1. Let matchKey be the string "./" concatenated with subpath.
                // Note: `package_imports_exports_resolve` does not require the leading dot.
                let match_key = &subpath;
                // 2. Let resolved be the result of PACKAGE_IMPORTS_EXPORTS_RESOLVE( matchKey, exports, packageURL, false, conditions).
                if let Some(path) = self
                    .package_imports_exports_resolve(
                        match_key,
                        exports,
                        package_url,
                        /* is_imports */ false,
                        conditions,
                        ctx,
                    )
                    .await?
                {
                    // 3. If resolved is not null or undefined, return resolved.
                    return Ok(Some(path));
                }
            }
            // 4. Throw a Package Path Not Exported error.
            Err(ResolveError::PackagePathNotExported(
                subpath.to_string(),
                package_url.join("package.json"),
            ))
        };
        Box::pin(fut)
    }

    /// PACKAGE_IMPORTS_RESOLVE(specifier, parentURL, conditions)
    async fn package_imports_resolve(
        &self,
        specifier: &str,
        package_json: &PackageJson,
        ctx: &mut Ctx,
    ) -> Result<Option<CachedPath>, ResolveError> {
        // 1. Assert: specifier begins with "#".
        debug_assert!(specifier.starts_with('#'), "{specifier}");
        //   2. If specifier is exactly equal to "#" or starts with "#/", then
        //   1. Throw an Invalid Module Specifier error.
        // 3. Let packageURL be the result of LOOKUP_PACKAGE_SCOPE(parentURL).
        // 4. If packageURL is not null, then

        // 1. Let pjson be the result of READ_PACKAGE_JSON(packageURL).
        // 2. If pjson.imports is a non-null Object, then

        // 1. Let resolved be the result of PACKAGE_IMPORTS_EXPORTS_RESOLVE( specifier, pjson.imports, packageURL, true, conditions).
        let mut has_imports = false;
        for imports in package_json.imports_fields(&self.options.imports_fields) {
            if !has_imports {
                has_imports = true;
                // TODO: fill in test case for this case
                if specifier == "#" || specifier.starts_with("#/") {
                    return Err(ResolveError::InvalidModuleSpecifier(
                        specifier.to_string(),
                        package_json.path.clone(),
                    ));
                }
            }
            if let Some(path) = self
                .package_imports_exports_resolve(
                    specifier,
                    imports,
                    package_json.directory(),
                    /* is_imports */ true,
                    &self.options.condition_names,
                    ctx,
                )
                .await?
            {
                // 2. If resolved is not null or undefined, return resolved.
                return Ok(Some(path));
            }
        }

        // 5. Throw a Package Import Not Defined error.
        if has_imports {
            Err(ResolveError::PackageImportNotDefined(
                specifier.to_string(),
                package_json.path.clone(),
            ))
        } else {
            Ok(None)
        }
    }

    /// PACKAGE_IMPORTS_EXPORTS_RESOLVE(matchKey, matchObj, packageURL, isImports, conditions)
    async fn package_imports_exports_resolve(
        &self,
        match_key: &str,
        match_obj: &JSONMap,
        package_url: &Path,
        is_imports: bool,
        conditions: &[String],
        ctx: &mut Ctx,
    ) -> ResolveResult {
        // enhanced-resolve behaves differently, it throws
        // Error: CachedPath to directories is not possible with the exports field (specifier was ./dist/)
        if match_key.ends_with('/') {
            return Ok(None);
        }
        // 1. If matchKey is a key of matchObj and does not contain "*", then
        if !match_key.contains('*') {
            // 1. Let target be the value of matchObj[matchKey].
            if let Some(target) = match_obj.get(match_key) {
                // 2. Return the result of PACKAGE_TARGET_RESOLVE(packageURL, target, null, isImports, conditions).
                return self
                    .package_target_resolve(
                        package_url,
                        match_key,
                        target,
                        None,
                        is_imports,
                        conditions,
                        ctx,
                    )
                    .await;
            }
        }

        let mut best_target = None;
        let mut best_match = "";
        let mut best_key = "";
        // 2. Let expansionKeys be the list of keys of matchObj containing only a single "*", sorted by the sorting function PATTERN_KEY_COMPARE which orders in descending order of specificity.
        // 3. For each key expansionKey in expansionKeys, do
        for (expansion_key, target) in match_obj {
            if expansion_key.starts_with("./") || expansion_key.starts_with('#') {
                // 1. Let patternBase be the substring of expansionKey up to but excluding the first "*" character.
                if let Some((pattern_base, pattern_trailer)) = expansion_key.split_once('*') {
                    // 2. If matchKey starts with but is not equal to patternBase, then
                    if match_key.starts_with(pattern_base)
                        // 1. Let patternTrailer be the substring of expansionKey from the index after the first "*" character.
                        && !pattern_trailer.contains('*')
                        // 2. If patternTrailer has zero length, or if matchKey ends with patternTrailer and the length of matchKey is greater than or equal to the length of expansionKey, then
                        && (pattern_trailer.is_empty()
                        || (match_key.len() >= expansion_key.len()
                        && match_key.ends_with(pattern_trailer)))
                        && Self::pattern_key_compare(best_key, expansion_key).is_gt()
                    {
                        // 1. Let target be the value of matchObj[expansionKey].
                        best_target = Some(target);
                        // 2. Let patternMatch be the substring of matchKey starting at the index of the length of patternBase up to the length of matchKey minus the length of patternTrailer.
                        best_match =
                            &match_key[pattern_base.len()..match_key.len() - pattern_trailer.len()];
                        best_key = expansion_key;
                    }
                } else if expansion_key.ends_with('/')
                    && match_key.starts_with(expansion_key)
                    && Self::pattern_key_compare(best_key, expansion_key).is_gt()
                {
                    // TODO: [DEP0148] DeprecationWarning: Use of deprecated folder mapping "./dist/" in the "exports" field module resolution of the package at xxx/package.json.
                    best_target = Some(target);
                    best_match = &match_key[expansion_key.len()..];
                    best_key = expansion_key;
                }
            }
        }
        if let Some(best_target) = best_target {
            // 3. Return the result of PACKAGE_TARGET_RESOLVE(packageURL, target, patternMatch, isImports, conditions).
            return self
                .package_target_resolve(
                    package_url,
                    best_key,
                    best_target,
                    Some(best_match),
                    is_imports,
                    conditions,
                    ctx,
                )
                .await;
        }
        // 4. Return null.
        Ok(None)
    }

    /// PACKAGE_TARGET_RESOLVE(packageURL, target, patternMatch, isImports, conditions)
    #[allow(clippy::too_many_arguments)]
    fn package_target_resolve<'a>(
        &'a self,
        package_url: &'a Path,
        target_key: &'a str,
        target: &'a JSONValue,
        pattern_match: Option<&'a str>,
        is_imports: bool,
        conditions: &'a [String],
        ctx: &'a mut Ctx,
    ) -> BoxFuture<'a, ResolveResult> {
        let fut = async move {
            fn normalize_string_target<'a>(
                target_key: &'a str,
                target: &'a str,
                pattern_match: Option<&'a str>,
                package_url: &Path,
            ) -> Result<Cow<'a, str>, ResolveError> {
                let target = if let Some(pattern_match) = pattern_match {
                    if !target_key.contains('*') && !target.contains('*') {
                        // enhanced-resolve behaviour
                        // TODO: [DEP0148] DeprecationWarning: Use of deprecated folder mapping "./dist/" in the "exports" field module resolution of the package at xxx/package.json.
                        if target_key.ends_with('/') && target.ends_with('/') {
                            Cow::Owned(format!("{target}{pattern_match}"))
                        } else {
                            return Err(ResolveError::InvalidPackageConfigDirectory(
                                package_url.join("package.json"),
                            ));
                        }
                    } else {
                        Cow::Owned(target.replace('*', pattern_match))
                    }
                } else {
                    Cow::Borrowed(target)
                };
                Ok(target)
            }

            match target {
                // 1. If target is a String, then
                JSONValue::String(target) => {
                    // 1. If target does not start with "./", then
                    if !target.starts_with("./") {
                        // 1. If isImports is false, or if target starts with "../" or "/", or if target is a valid URL, then
                        if !is_imports || target.starts_with("../") || target.starts_with('/') {
                            // 1. Throw an Invalid Package Target error.
                            return Err(ResolveError::InvalidPackageTarget(
                                target.to_string(),
                                target_key.to_string(),
                                package_url.join("package.json"),
                            ));
                        }
                        // 2. If patternMatch is a String, then
                        //   1. Return PACKAGE_RESOLVE(target with every instance of "*" replaced by patternMatch, packageURL + "/").
                        let target = normalize_string_target(
                            target_key,
                            target,
                            pattern_match,
                            package_url,
                        )?;
                        let package_url = self.cache.value(package_url);
                        // // 3. Return PACKAGE_RESOLVE(target, packageURL + "/").
                        return self.package_resolve(&package_url, &target, ctx).await;
                    }

                    // 2. If target split on "/" or "\" contains any "", ".", "..", or "node_modules" segments after the first "." segment, case insensitive and including percent encoded variants, throw an Invalid Package Target error.
                    // 3. Let resolvedTarget be the URL resolution of the concatenation of packageURL and target.
                    // 4. Assert: resolvedTarget is contained in packageURL.
                    // 5. If patternMatch is null, then
                    let target =
                        normalize_string_target(target_key, target, pattern_match, package_url)?;
                    if Path::new(target.as_ref()).is_invalid_exports_target() {
                        return Err(ResolveError::InvalidPackageTarget(
                            target.to_string(),
                            target_key.to_string(),
                            package_url.join("package.json"),
                        ));
                    }
                    let resolved_target = package_url.normalize_with(target.as_ref());
                    // 6. If patternMatch split on "/" or "\" contains any "", ".", "..", or "node_modules" segments, case insensitive and including percent encoded variants, throw an Invalid Module Specifier error.
                    // 7. Return the URL resolution of resolvedTarget with every instance of "*" replaced with patternMatch.
                    let value = self.cache.value(&resolved_target);
                    return Ok(Some(value));
                }
                // 2. Otherwise, if target is a non-null Object, then
                JSONValue::Object(target) => {
                    // 1. If exports contains any index property keys, as defined in ECMA-262 6.1.7 Array Index, throw an Invalid Package Configuration error.
                    // 2. For each property p of target, in object insertion order as,
                    for (key, target_value) in target {
                        // 1. If p equals "default" or conditions contains an entry for p, then
                        if key == "default" || conditions.contains(key) {
                            // 1. Let targetValue be the value of the p property in target.
                            // 2. Let resolved be the result of PACKAGE_TARGET_RESOLVE( packageURL, targetValue, patternMatch, isImports, conditions).
                            let resolved = self
                                .package_target_resolve(
                                    package_url,
                                    target_key,
                                    target_value,
                                    pattern_match,
                                    is_imports,
                                    conditions,
                                    ctx,
                                )
                                .await;
                            // 3. If resolved is equal to undefined, continue the loop.
                            if let Some(path) = resolved? {
                                // 4. Return resolved.
                                return Ok(Some(path));
                            }
                        }
                    }
                    // 3. Return undefined.
                    return Ok(None);
                }
                // 3. Otherwise, if target is an Array, then
                JSONValue::Array(targets) => {
                    // 1. If _target.length is zero, return null.
                    if targets.is_empty() {
                        // Note: return PackagePathNotExported has the same effect as return because there are no matches.
                        return Err(ResolveError::PackagePathNotExported(
                            pattern_match.unwrap_or(".").to_string(),
                            package_url.join("package.json"),
                        ));
                    }
                    // 2. For each item targetValue in target, do
                    for (i, target_value) in targets.iter().enumerate() {
                        // 1. Let resolved be the result of PACKAGE_TARGET_RESOLVE( packageURL, targetValue, patternMatch, isImports, conditions), continuing the loop on any Invalid Package Target error.
                        let resolved = self
                            .package_target_resolve(
                                package_url,
                                target_key,
                                target_value,
                                pattern_match,
                                is_imports,
                                conditions,
                                ctx,
                            )
                            .await;

                        if resolved.is_err() && i == targets.len() {
                            return resolved;
                        }

                        // 2. If resolved is undefined, continue the loop.
                        if let Ok(Some(path)) = resolved {
                            // 3. Return resolved.
                            return Ok(Some(path));
                        }
                    }
                    // 3. Return or throw the last fallback resolution null return or error.
                    // Note: see `resolved.is_err() && i == targets.len()`
                }
                _ => {}
            }
            // 4. Otherwise, if target is null, return null.
            Ok(None)
            // 5. Otherwise throw an Invalid Package Target error.
        };
        Box::pin(fut)
    }

    // Returns (module, subpath)
    // https://github.com/nodejs/node/blob/8f0f17e1e3b6c4e58ce748e06343c5304062c491/lib/internal/modules/esm/resolve.js#L688
    fn parse_package_specifier(specifier: &str) -> (&str, &str) {
        let mut separator_index = specifier.as_bytes().iter().position(|b| *b == b'/');
        // let mut valid_package_name = true;
        // let mut is_scoped = false;
        if specifier.starts_with('@') {
            // is_scoped = true;
            if separator_index.is_none() || specifier.is_empty() {
                // valid_package_name = false;
            } else if let Some(index) = &separator_index {
                separator_index = specifier[*index + 1..]
                    .as_bytes()
                    .iter()
                    .position(|b| *b == b'/')
                    .map(|i| i + *index + 1);
            }
        }
        let package_name =
            separator_index.map_or(specifier, |separator_index| &specifier[..separator_index]);

        // TODO: https://github.com/nodejs/node/blob/8f0f17e1e3b6c4e58ce748e06343c5304062c491/lib/internal/modules/esm/resolve.js#L705C1-L714C1
        // Package name cannot have leading . and cannot have percent-encoding or
        // \\ separators.
        // if (RegExpPrototypeExec(invalidPackageNameRegEx, packageName) !== null)
        // validPackageName = false;

        // if (!validPackageName) {
        // throw new ERR_INVALID_MODULE_SPECIFIER(
        // specifier, 'is not a valid package name', fileURLToPath(base));
        // }
        let package_subpath =
            separator_index.map_or("", |separator_index| &specifier[separator_index..]);
        (package_name, package_subpath)
    }

    /// PATTERN_KEY_COMPARE(keyA, keyB)
    fn pattern_key_compare(key_a: &str, key_b: &str) -> Ordering {
        if key_a.is_empty() {
            return Ordering::Greater;
        }
        // 1. Assert: keyA ends with "/" or contains only a single "*".
        debug_assert!(key_a.ends_with('/') || key_a.match_indices('*').count() == 1, "{key_a}");
        // 2. Assert: keyB ends with "/" or contains only a single "*".
        debug_assert!(key_b.ends_with('/') || key_b.match_indices('*').count() == 1, "{key_b}");
        // 3. Let baseLengthA be the index of "*" in keyA plus one, if keyA contains "*", or the length of keyA otherwise.
        let a_pos = key_a.chars().position(|c| c == '*');
        let base_length_a = a_pos.map_or(key_a.len(), |p| p + 1);
        // 4. Let baseLengthB be the index of "*" in keyB plus one, if keyB contains "*", or the length of keyB otherwise.
        let b_pos = key_b.chars().position(|c| c == '*');
        let base_length_b = b_pos.map_or(key_b.len(), |p| p + 1);
        // 5. If baseLengthA is greater than baseLengthB, return -1.
        if base_length_a > base_length_b {
            return Ordering::Less;
        }
        // 6. If baseLengthB is greater than baseLengthA, return 1.
        if base_length_b > base_length_a {
            return Ordering::Greater;
        }
        // 7. If keyA does not contain "*", return 1.
        if !key_a.contains('*') {
            return Ordering::Greater;
        }
        // 8. If keyB does not contain "*", return -1.
        if !key_b.contains('*') {
            return Ordering::Less;
        }
        // 9. If the length of keyA is greater than the length of keyB, return -1.
        if key_a.len() > key_b.len() {
            return Ordering::Less;
        }
        // 10. If the length of keyB is greater than the length of keyA, return 1.
        if key_b.len() > key_a.len() {
            return Ordering::Greater;
        }
        // 11. Return 0.
        Ordering::Equal
    }

    fn strip_package_name<'a>(specifier: &'a str, package_name: &'a str) -> Option<&'a str> {
        specifier
            .strip_prefix(package_name)
            .filter(|tail| tail.is_empty() || tail.starts_with(SLASH_START))
    }
}
