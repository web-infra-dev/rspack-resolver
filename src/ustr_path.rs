use std::{
  collections::HashSet,
  fmt,
  hash::{BuildHasherDefault, Hash, Hasher},
  ops::Deref,
  path::{Path, PathBuf},
};

use camino::{Utf8Path, Utf8PathBuf};
use ustr::Ustr;

/// A globally interned UTF-8 path.
///
/// 8 bytes, `Copy`, and `'static`: every distinct path string exists exactly
/// once process-wide, so the same path handed to many consumers costs one
/// pointer each instead of one heap allocation each.
///
/// Equality degenerates to a pointer comparison (interning guarantees one
/// pointer per string) and hashing to a single `u64` load from the interner
/// entry header, so `UstrPathSet` lookups cost one `write_u64`.
///
/// The interner is shared with rspack — both crates depend on the same
/// `ustr-fxhash` version, hence the same static — so a path interned here is
/// already interned for rspack.
///
/// # Lifetime
///
/// Interned strings are **never freed**. See `CLAUDE_USTR_PATH_DESIGN.md` §4.3.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct UstrPath(Ustr);

#[inline]
const fn is_sep(c: u8) -> bool {
  c == b'/' || c == b'\\'
}

/// Rewrite `s` into canonical Windows form: `\` separators, no repeated
/// separators, no trailing separator (roots excepted), and an uppercase
/// drive letter — matching the folding std's `Path` applies via
/// `Prefix::Disk` before comparing paths in `Hash`/`Eq`.
///
/// Returns `None` when `s` is already canonical — the common case — so the
/// caller interns the original `&str` without allocating.
///
/// Paths beginning with two separators (UNC and verbatim, e.g. `\\?\C:\x` or
/// `\\server\share`) pass through untouched, including the verbatim disk
/// prefix's drive-letter case: std folds that case too (`VerbatimDisk` goes
/// through the same `parse_drive`), so leaving it alone here is a known,
/// accepted divergence from `Path`'s `Hash`/`Eq` for verbatim paths — the
/// `dunce` dependency already keeps the resolver off UNC/verbatim paths, so
/// this code path isn't reached in practice.
///
/// Platform-independent on purpose. camino's `Utf8Path::components()` picks its
/// separator set at compile time, so it cannot express Windows semantics on a
/// unix host — this would otherwise be untestable off Windows.
// Referenced by `UstrPath::new` only on Windows, but compiled and tested
// everywhere so the normalization rules stay verifiable on a unix host. Also
// excluded under `cfg(test)`: the test module below calls it directly, so on
// a non-Windows test build it is not actually dead and the expectation would
// go unfulfilled.
#[cfg_attr(
  not(any(windows, test)),
  expect(dead_code, reason = "windows-only caller; tested on all platforms")
)]
fn normalize_windows_separators(s: &str) -> Option<String> {
  let bytes = s.as_bytes();

  if bytes.len() >= 2 && is_sep(bytes[0]) && is_sep(bytes[1]) {
    return None;
  }

  // A lowercase drive letter (`c:\...`) is not canonical on its own — std
  // folds it to uppercase (`Prefix::Disk`) before comparing paths — even
  // when every separator is already fine.
  let mut needs_rewrite = bytes.len() >= 2 && bytes[0].is_ascii_lowercase() && bytes[1] == b':';
  let mut prev_was_sep = false;
  for (i, &c) in bytes.iter().enumerate() {
    if is_sep(c) {
      // A trailing separator only counts as canonical when it is the root
      // itself (`\`) or a drive root (`C:\`).
      if c == b'/' || prev_was_sep || (i + 1 == bytes.len() && i > 0 && bytes[i - 1] != b':') {
        needs_rewrite = true;
        break;
      }
      prev_was_sep = true;
    } else {
      prev_was_sep = false;
    }
  }
  if !needs_rewrite {
    return None;
  }

  let fold_drive = bytes.len() >= 2 && bytes[1] == b':';
  let mut out = String::with_capacity(s.len());
  let mut prev_was_sep = false;
  for (i, ch) in s.chars().enumerate() {
    if ch == '/' || ch == '\\' {
      if !prev_was_sep {
        out.push('\\');
      }
      prev_was_sep = true;
    } else if i == 0 && fold_drive && ch.is_ascii_lowercase() {
      out.push(ch.to_ascii_uppercase());
      prev_was_sep = false;
    } else {
      out.push(ch);
      prev_was_sep = false;
    }
  }

  if out.len() > 1 && out.ends_with('\\') && !out.ends_with(":\\") {
    out.pop();
  }

  Some(out)
}

impl UstrPath {
  /// Intern `path` and return a handle to it.
  ///
  /// On Windows the path is first rewritten into canonical form so that every
  /// spelling of the same path (`C:/a/b`, `C:\a\b`, `C:\a\\b`, `C:\a\b\`)
  /// interns to one pointer — preserving the dedup semantics `Path`'s
  /// component-wise `Hash`/`Eq` used to provide.
  #[inline]
  pub fn new(path: &str) -> Self {
    #[cfg(windows)]
    if let Some(normalized) = normalize_windows_separators(path) {
      return Self(Ustr::from(&normalized));
    }
    Self(Ustr::from(path))
  }

  #[inline]
  pub fn as_str(&self) -> &'static str {
    self.0.as_str()
  }

  #[inline]
  pub fn as_utf8_path(&self) -> &'static Utf8Path {
    Utf8Path::new(self.0.as_str())
  }

  #[inline]
  pub fn as_std_path(&self) -> &'static Path {
    Path::new(self.0.as_str())
  }

  /// The `FxHash` of the path bytes, precomputed by the interner.
  ///
  /// Reading it is a single load from the entry header (`char_ptr - 16`).
  #[inline]
  pub fn precomputed_hash(&self) -> u64 {
    self.0.precomputed_hash()
  }
}

impl Default for UstrPath {
  /// The empty path. `Ustr::default()` interns `""`, so this is a handle to a
  /// real interned entry rather than a dangling one.
  #[inline]
  fn default() -> Self {
    Self(Ustr::default())
  }
}

impl Hash for UstrPath {
  #[inline]
  fn hash<H: Hasher>(&self, state: &mut H) {
    state.write_u64(self.0.precomputed_hash());
  }
}

impl Deref for UstrPath {
  type Target = Utf8Path;

  #[inline]
  fn deref(&self) -> &Self::Target {
    self.as_utf8_path()
  }
}

impl AsRef<Utf8Path> for UstrPath {
  #[inline]
  fn as_ref(&self) -> &Utf8Path {
    self.as_utf8_path()
  }
}

impl AsRef<Path> for UstrPath {
  #[inline]
  fn as_ref(&self) -> &Path {
    self.as_std_path()
  }
}

impl AsRef<str> for UstrPath {
  #[inline]
  fn as_ref(&self) -> &str {
    self.as_str()
  }
}

impl fmt::Debug for UstrPath {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.as_utf8_path().fmt(f)
  }
}

impl fmt::Display for UstrPath {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

/// A `HashSet<UstrPath>` keyed by the interner's precomputed hash.
///
/// Uses `ustr::IdentityHasher` rather than the private `IdentityHasher` in
/// `crate::cache` so rspack's `ArcPathSet` is the same concrete type and can
/// take these sets by `mem::take` instead of re-bucketing them.
pub type UstrPathSet = HashSet<UstrPath, BuildHasherDefault<ustr::IdentityHasher>>;

/// Re-exported so downstream crates can spell the same concrete set type
/// without taking a direct `ustr` dependency (and without risking a version
/// split — see the note on the `ustr` dependency in `Cargo.toml`).
///
/// This is deliberately the upstream `ustr` type, not a local newtype: rspack
/// spells its `ArcPathSet` with this exact type, and only that type identity
/// lets rspack `mem::take` our dependency sets instead of re-bucketing every
/// element into its own hasher. Do not replace it with a local hasher.
///
/// Upstream marks it `#[doc(hidden)]` (it is an implementation detail of
/// `ustr-fxhash`), so it is not semver-protected there — a patch release could
/// rename or remove it without notice. The pinned `ustr` version in
/// `Cargo.toml` is what actually protects this re-export; bumping that
/// dependency must re-verify `IdentityHasher` still exists and still behaves
/// like an identity hash.
pub use ustr::IdentityHasher;

/// Convert any path-shaped value into an interned [`UstrPath`].
///
/// The two `std::path` implementations panic on non-UTF-8 input, matching what
/// the resolver already does at every `Path -> Utf8Path` boundary (see the
/// `expect("path should be UTF-8")` calls in `lib.rs` and `cache.rs`).
pub trait ToUstrPath {
  fn to_ustr_path(&self) -> UstrPath;
}

impl ToUstrPath for str {
  #[inline]
  fn to_ustr_path(&self) -> UstrPath {
    UstrPath::new(self)
  }
}

impl ToUstrPath for String {
  #[inline]
  fn to_ustr_path(&self) -> UstrPath {
    UstrPath::new(self)
  }
}

impl ToUstrPath for Utf8Path {
  #[inline]
  fn to_ustr_path(&self) -> UstrPath {
    UstrPath::new(self.as_str())
  }
}

impl ToUstrPath for Utf8PathBuf {
  #[inline]
  fn to_ustr_path(&self) -> UstrPath {
    UstrPath::new(self.as_str())
  }
}

impl ToUstrPath for Path {
  #[inline]
  fn to_ustr_path(&self) -> UstrPath {
    UstrPath::new(self.to_str().expect("path should be UTF-8"))
  }
}

impl ToUstrPath for PathBuf {
  #[inline]
  fn to_ustr_path(&self) -> UstrPath {
    self.as_path().to_ustr_path()
  }
}

impl ToUstrPath for UstrPath {
  #[inline]
  fn to_ustr_path(&self) -> UstrPath {
    *self
  }
}

impl<T: ?Sized + ToUstrPath> From<&T> for UstrPath {
  #[inline]
  fn from(value: &T) -> Self {
    value.to_ustr_path()
  }
}

impl From<Utf8PathBuf> for UstrPath {
  #[inline]
  fn from(value: Utf8PathBuf) -> Self {
    value.to_ustr_path()
  }
}

impl From<PathBuf> for UstrPath {
  #[inline]
  fn from(value: PathBuf) -> Self {
    value.to_ustr_path()
  }
}

#[cfg(test)]
mod tests {
  use std::{
    collections::HashSet,
    hash::{Hash, Hasher},
  };

  use camino::Utf8Path;

  use super::*;

  #[test]
  fn default_is_the_empty_path() {
    assert_eq!(UstrPath::default().as_str(), "");
  }

  #[test]
  fn same_string_is_same_pointer() {
    let a = UstrPath::new("/a/b/c.js");
    let b = UstrPath::new("/a/b/c.js");
    assert_eq!(a.as_str().as_ptr(), b.as_str().as_ptr());
    assert_eq!(a, b);
  }

  #[test]
  fn different_strings_are_not_equal() {
    assert_ne!(UstrPath::new("/a/b"), UstrPath::new("/a/c"));
  }

  #[test]
  fn hash_is_the_precomputed_hash() {
    let p = UstrPath::new("/x/y");
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    p.hash(&mut hasher);
    // The value written into the hasher is the precomputed one, not a
    // re-hash of the bytes.
    let mut expected = std::collections::hash_map::DefaultHasher::new();
    expected.write_u64(p.precomputed_hash());
    assert_eq!(hasher.finish(), expected.finish());
  }

  #[test]
  fn works_in_an_identity_hashed_set() {
    let mut set: UstrPathSet = HashSet::default();
    set.insert(UstrPath::new("/a/b"));
    assert!(set.contains(&UstrPath::new("/a/b")));
    assert!(!set.contains(&UstrPath::new("/a/c")));
  }

  #[test]
  fn derefs_to_utf8_path() {
    let p = UstrPath::new("/a/b/c.js");
    assert_eq!(p.file_name(), Some("c.js"));
    assert_eq!(p.parent(), Some(Utf8Path::new("/a/b")));
    assert_eq!(p.extension(), Some("js"));
    assert_eq!(p.join("d.ts"), Utf8Path::new("/a/b/c.js/d.ts"));
  }

  #[test]
  fn debug_prints_the_path_not_the_ustr_wrapper() {
    // The point of this test is that `Debug`/`Display` render the path rather
    // than `Ustr`'s own `u!("...")` wrapper. The separator is incidental — on
    // Windows `UstrPath::new` canonicalizes `/a/b` to `\a\b`, so the expected
    // text has to follow the platform or the assertion tests normalization by
    // accident instead of formatting.
    let canonical = if cfg!(windows) { r"\a\b" } else { "/a/b" };
    let p = UstrPath::new("/a/b");
    assert_eq!(format!("{p:?}"), format!("{canonical:?}"));
    assert_eq!(format!("{p}"), canonical);
  }

  #[test]
  fn as_ref_targets_compile_and_agree() {
    let p = UstrPath::new("/a/b");
    let as_utf8: &Utf8Path = p.as_ref();
    let std_path: &std::path::Path = p.as_ref();
    let str_ref: &str = p.as_ref();
    assert_eq!(as_utf8.as_str(), str_ref);
    assert_eq!(std_path, std::path::Path::new("/a/b"));
  }

  #[test]
  fn identity_hasher_receives_eight_bytes() {
    // `ustr::IdentityHasher::write` silently produces a 0 hash unless it gets
    // exactly 8 bytes. `UstrPath::hash` goes through the default `write_u64`,
    // which forwards `u64::to_ne_bytes()` — exactly 8. Guard that invariant so
    // a future change to `Hash` cannot silently collapse every key to bucket 0.
    let mut hasher = ustr::IdentityHasher::default();
    UstrPath::new("/some/long/path/that/is/not/eight/bytes").hash(&mut hasher);
    assert_ne!(hasher.finish(), 0);
  }

  #[test]
  fn already_canonical_windows_paths_need_no_rewrite() {
    assert_eq!(normalize_windows_separators(r"C:\a\b"), None);
    assert_eq!(normalize_windows_separators(r"C:\"), None);
    assert_eq!(normalize_windows_separators(r"\"), None);
    assert_eq!(normalize_windows_separators("a"), None);
  }

  #[test]
  fn forward_slashes_become_backslashes() {
    assert_eq!(
      normalize_windows_separators("C:/a/b").as_deref(),
      Some(r"C:\a\b")
    );
    assert_eq!(
      normalize_windows_separators(r"C:/a\b").as_deref(),
      Some(r"C:\a\b")
    );
  }

  #[test]
  fn repeated_separators_collapse() {
    assert_eq!(
      normalize_windows_separators(r"C:\a\\b").as_deref(),
      Some(r"C:\a\b")
    );
    assert_eq!(
      normalize_windows_separators("C://a//b").as_deref(),
      Some(r"C:\a\b")
    );
  }

  #[test]
  fn trailing_separator_is_dropped_but_roots_survive() {
    assert_eq!(
      normalize_windows_separators(r"C:\a\b\").as_deref(),
      Some(r"C:\a\b")
    );
    assert_eq!(normalize_windows_separators("C:/").as_deref(), Some(r"C:\"));
    assert_eq!(normalize_windows_separators(r"C:\"), None);
  }

  #[test]
  fn unc_and_verbatim_paths_pass_through_untouched() {
    // std disables separator normalization for verbatim prefixes, and `dunce`
    // already keeps the resolver off UNC paths — leaving both alone is both
    // correct and the conservative choice.
    assert_eq!(normalize_windows_separators(r"\\?\C:\a/b"), None);
    assert_eq!(normalize_windows_separators("//?/C:/a/b"), None);
    assert_eq!(normalize_windows_separators(r"\\server\share\a"), None);
  }

  #[test]
  fn non_ascii_segments_survive_normalization() {
    assert_eq!(
      normalize_windows_separators("C:/项目/源码").as_deref(),
      Some(r"C:\项目\源码")
    );
  }

  #[test]
  fn lowercase_drive_letter_is_folded_to_uppercase() {
    assert_eq!(
      normalize_windows_separators(r"c:\a\b").as_deref(),
      Some(r"C:\a\b")
    );
    assert_eq!(
      normalize_windows_separators("c:/a/b").as_deref(),
      Some(r"C:\a\b")
    );
    assert_eq!(
      normalize_windows_separators(r"c:\").as_deref(),
      Some(r"C:\")
    );
    // Already uppercase and otherwise canonical: still no rewrite.
    assert_eq!(normalize_windows_separators(r"C:\a\b"), None);
  }

  #[test]
  fn case_is_folded_only_on_the_drive_letter() {
    assert_eq!(
      normalize_windows_separators(r"c:\Foo\BAR.ts").as_deref(),
      Some(r"C:\Foo\BAR.ts")
    );
    // No drive letter at all — nothing to fold.
    assert_eq!(normalize_windows_separators(r"relative\Path"), None);
  }

  #[cfg(windows)]
  #[test]
  fn equivalent_windows_spellings_intern_to_one_pointer() {
    let canonical = UstrPath::new(r"C:\a\b");
    for spelling in [
      r"C:\a\b", "C:/a/b", r"C:/a\b", r"C:\a\\b", r"C:\a\b\", "c:/a/b", r"c:\a\b",
    ] {
      let p = UstrPath::new(spelling);
      assert_eq!(
        p.as_str().as_ptr(),
        canonical.as_str().as_ptr(),
        "spelling {spelling:?} should intern to the canonical pointer"
      );
    }
  }

  #[test]
  fn to_ustr_path_accepts_every_path_flavor() {
    use camino::Utf8PathBuf;

    let expected = UstrPath::new("/a/b");
    assert_eq!("/a/b".to_ustr_path(), expected);
    assert_eq!(String::from("/a/b").to_ustr_path(), expected);
    assert_eq!(Utf8Path::new("/a/b").to_ustr_path(), expected);
    assert_eq!(Utf8PathBuf::from("/a/b").to_ustr_path(), expected);
    assert_eq!(std::path::Path::new("/a/b").to_ustr_path(), expected);
    assert_eq!(std::path::PathBuf::from("/a/b").to_ustr_path(), expected);
    assert_eq!(expected.to_ustr_path(), expected);
  }

  #[test]
  fn from_impls_match_to_ustr_path() {
    use camino::Utf8PathBuf;

    let expected = UstrPath::new("/a/b");
    assert_eq!(UstrPath::from("/a/b"), expected);
    assert_eq!(UstrPath::from(Utf8Path::new("/a/b")), expected);
    assert_eq!(UstrPath::from(Utf8PathBuf::from("/a/b")), expected);
    assert_eq!(UstrPath::from(std::path::Path::new("/a/b")), expected);
    assert_eq!(UstrPath::from(std::path::PathBuf::from("/a/b")), expected);
  }

  #[test]
  #[should_panic(expected = "path should be UTF-8")]
  #[cfg(unix)]
  fn non_utf8_std_path_panics_like_the_rest_of_the_resolver() {
    use std::{ffi::OsStr, os::unix::ffi::OsStrExt};

    let bad = std::path::Path::new(OsStr::from_bytes(b"/a/\xff/b"));
    let _ = bad.to_ustr_path();
  }
}
