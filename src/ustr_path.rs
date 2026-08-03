use std::{
  collections::HashSet,
  fmt,
  hash::{BuildHasherDefault, Hash, Hasher},
  ops::Deref,
  path::{Path, PathBuf},
};

use camino::{Utf8Path, Utf8PathBuf};
use rustc_hash::FxHasher;

use crate::interner::{self, Interned};

/// A globally interned UTF-8 path.
///
/// One pointer wide. Every distinct path string is stored once no matter how
/// many consumers hold it, so the same path handed to N downstream stores
/// costs N pointers rather than N copies.
///
/// Equality is a pointer comparison whenever the strings are byte-identical,
/// and hashing is a single `u64` load from the entry header, so `UstrPathSet`
/// lookups cost one `write_u64`. On Windows, spellings that differ only in
/// separators or drive case are distinct entries, so both fall back to
/// `Path`'s component semantics to keep them one key. Paths are stored
/// verbatim — the folding lives in the comparison, not in what gets interned.
///
/// The interner is a static inside this crate, so rspack shares it by using
/// this type rather than by agreeing on a third-party crate version.
///
/// # Lifetime
///
/// Entries are refcounted and **dropped when the last handle goes away**. This
/// is what lets `Cache::clear()` actually return memory: rspack clears the
/// resolver cache on every rebuild, so a leak-forever interner would make RSS
/// climb monotonically in a dev server. The flip side is that `as_str()`
/// borrows from `self` rather than being `'static`.
#[derive(Clone)]
pub struct UstrPath(Interned);

/// Hash a path the way `PartialEq` compares it, so `a == b` implies equal
/// hashes on every platform.
///
/// Unix hashes the raw bytes — `hash_utf8_path` has always compared resolver
/// paths byte-wise there. Windows walks components, matching `Path`'s own
/// `Hash`, so `C:/a/b`, `C:\a\b`, `C:\a\b\` and `c:\a\b` land together.
#[inline]
fn hash_path_str(s: &str) -> u64 {
  let mut hasher = FxHasher::default();
  #[cfg(windows)]
  Path::new(s).hash(&mut hasher);
  #[cfg(not(windows))]
  hasher.write(s.as_bytes());
  hasher.finish()
}

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
// NOT CURRENTLY WIRED UP. `UstrPath::new` used to call this on Windows, but
// `UstrPath` also carries strings that are not canonical filesystem paths —
// rspack stores caller-supplied dependency specifiers such as
// `addBuildDependency("./build.txt")` in path sets — and rewriting those
// changed values observable through the public JS API (`./build.txt` came back
// as `.\build.txt`). Correctness of the stored string wins over dedup for now;
// the function and its tests are kept so re-enabling it behind a separate
// canonicalizing constructor stays cheap.
#[cfg_attr(
  not(test),
  expect(dead_code, reason = "kept for reference; see comment above")
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
  /// Intern `path` verbatim and return a handle to it.
  ///
  /// The string is stored exactly as given — rspack puts caller-supplied
  /// specifiers such as `addBuildDependency("./build.txt")` into path sets and
  /// reads them back through the JS API, so rewriting them here would change
  /// observable values. See the note on `normalize_windows_separators`.
  ///
  /// On Windows this means distinct spellings of one path (`C:/a/b` vs
  /// `C:\a\b`) intern to distinct handles. They still compare and hash equal —
  /// `PartialEq`/`Hash` fold them via `Path`'s component semantics rather than
  /// the stored bytes — so using them as set or map keys still dedups.
  #[inline]
  pub fn new(path: &str) -> Self {
    Self(interner::intern(path, hash_path_str(path)))
  }

  /// Borrows from `self`, not `'static`: the entry is freed once the last
  /// handle drops.
  #[inline]
  pub fn as_str(&self) -> &str {
    self.0.as_str()
  }

  #[inline]
  pub fn as_utf8_path(&self) -> &Utf8Path {
    Utf8Path::new(self.as_str())
  }

  #[inline]
  pub fn as_std_path(&self) -> &Path {
    Path::new(self.as_str())
  }

  /// The hash used for set and map lookups, computed once at construction and
  /// read back from the interner entry header.
  ///
  /// Matches [`PartialEq`] per platform: raw bytes on unix, `Path` components
  /// on Windows.
  #[inline]
  pub fn precomputed_hash(&self) -> u64 {
    self.0.hash()
  }

  /// How many handles currently share this interned string. Test/diagnostic
  /// use — the count is a snapshot and can change concurrently.
  #[cfg(test)]
  pub(crate) fn refcount(&self) -> usize {
    self.0.refcount()
  }
}

impl Default for UstrPath {
  /// The empty path — a handle to a real interned `""`, not a dangling one.
  #[inline]
  fn default() -> Self {
    Self::new("")
  }
}

impl PartialEq for UstrPath {
  /// Byte-identical strings share one interner entry, so the pointer check
  /// settles them — on unix that is the whole story, matching the byte-wise
  /// comparison `hash_utf8_path` has always used for resolver paths.
  ///
  /// Windows additionally folds spellings: `Path`'s `Eq` walks components, so
  /// `C:/a/b`, `C:\a\b`, `C:\a\\b`, `C:\a\b\` and `c:\a\b` are all one path.
  /// Since [`UstrPath::new`] stores the string verbatim those are distinct
  /// entries, and only the component walk can tell they are equal. The hash
  /// check in front of it is a cheap reject, valid because `hash_path_str`
  /// hashes components on Windows too.
  #[inline]
  fn eq(&self, other: &Self) -> bool {
    if self.0.ptr_eq(&other.0) {
      return true;
    }
    #[cfg(windows)]
    {
      self.precomputed_hash() == other.precomputed_hash()
        && self.as_std_path() == other.as_std_path()
    }
    #[cfg(not(windows))]
    {
      false
    }
  }
}

impl Eq for UstrPath {}

impl Hash for UstrPath {
  /// One `write_u64` of the hash computed at construction.
  ///
  /// The single write is load-bearing, not stylistic: [`UstrPathSet`] is keyed
  /// by [`IdentityHasher`], whose `write` only reads a value when handed
  /// exactly 8 bytes and **silently yields 0** otherwise. Hashing the path
  /// inline here would feed it component-sized writes and collapse every key
  /// into one bucket with no error.
  #[inline]
  fn hash<H: Hasher>(&self, state: &mut H) {
    state.write_u64(self.precomputed_hash());
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
/// Spelled with [`IdentityHasher`] rather than a set-local hasher so rspack's
/// `ArcPathSet` is the same concrete type and can take these sets by
/// `mem::take` instead of re-bucketing every element.
pub type UstrPathSet = HashSet<UstrPath, BuildHasherDefault<IdentityHasher>>;

/// Passes an already-computed hash straight through.
///
/// [`UstrPath::hash`] writes the hash it computed at construction, so re-mixing
/// it here would be wasted work. Downstream spells its own path sets with this
/// exact type so it can `mem::take` ours instead of re-bucketing every element.
///
/// Only `write_u64` is meaningful. Anything else is a misuse — the key type is
/// not `UstrPath` — and would silently produce 0 for every key, so it panics in
/// debug builds rather than quietly degrading the map into a linked list.
#[derive(Default, Clone, Copy)]
pub struct IdentityHasher(u64);

impl Hasher for IdentityHasher {
  #[inline]
  fn write(&mut self, bytes: &[u8]) {
    debug_assert!(
      false,
      "IdentityHasher only accepts write_u64; got a {}-byte write. The key \
       type is probably not UstrPath.",
      bytes.len()
    );
    // Release builds: fold the bytes rather than yielding 0, so a misuse
    // degrades performance instead of collapsing every key into one bucket.
    let mut h = FxHasher::default();
    h.write(bytes);
    self.0 = h.finish();
  }

  #[inline]
  fn write_u64(&mut self, n: u64) {
    self.0 = n;
  }

  #[inline]
  fn finish(&self) -> u64 {
    self.0
  }
}

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
    self.clone()
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

  /// The handle must stay one pointer wide. It is stored by the million
  /// downstream — every dependency set, every `Vec<UstrPath>` — so widening it
  /// (by caching the hash beside the pointer instead of in the interner entry,
  /// say) shows up directly as `memcpy` in the resolver benchmarks.
  #[test]
  fn the_handle_is_one_pointer_wide() {
    assert_eq!(
      std::mem::size_of::<UstrPath>(),
      std::mem::size_of::<usize>()
    );
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

  /// `UstrPathSet` is keyed by [`IdentityHasher`], so this is the hash that
  /// actually decides bucketing. Folding it through that hasher — rather than a
  /// generic one — is what proves `Hash` still delivers exactly 8 bytes: the
  /// identity hasher silently yields 0 for any other width.
  fn set_hash(p: &UstrPath) -> u64 {
    let mut hasher = IdentityHasher::default();
    p.hash(&mut hasher);
    hasher.finish()
  }

  #[cfg(windows)]
  #[test]
  fn windows_spellings_of_one_path_are_equal_and_hash_alike() {
    // Stored verbatim, so these are genuinely distinct interned handles —
    // the folding has to come from `PartialEq`/`Hash`, not from interning.
    let canonical = UstrPath::new(r"C:\a\b");
    for spelling in ["C:/a/b", r"C:/a\b", r"C:\a\\b", r"C:\a\b\", r"c:\a\b"] {
      let p = UstrPath::new(spelling);
      assert_ne!(
        p.as_str(),
        canonical.as_str(),
        "{spelling:?} must be stored verbatim, not rewritten"
      );
      assert_eq!(p, canonical, "{spelling:?} should compare equal");
      assert_eq!(
        set_hash(&p),
        set_hash(&canonical),
        "{spelling:?} hashes differently, which would break set bucketing"
      );
    }
  }

  #[cfg(windows)]
  #[test]
  fn windows_distinct_paths_stay_distinct() {
    assert_ne!(UstrPath::new(r"C:\a\b"), UstrPath::new(r"C:\a\c"));
    assert_ne!(UstrPath::new(r"C:\a\b"), UstrPath::new(r"D:\a\b"));
  }

  #[cfg(windows)]
  #[test]
  fn windows_equal_paths_are_one_key_in_a_set() {
    let mut set: UstrPathSet = HashSet::default();
    set.insert(UstrPath::new(r"C:\a\b"));
    assert!(set.contains(&UstrPath::new("C:/a/b")));
    assert!(set.contains(&UstrPath::new(r"c:\a\b")));
    set.insert(UstrPath::new("C:/a/b"));
    assert_eq!(set.len(), 1, "equal spellings must collapse to one entry");
  }

  #[test]
  fn hash_delivers_eight_bytes_to_the_identity_hasher() {
    // Guards both branches of `Hash`: a regression that forwarded
    // `Path::hash` straight through would write component-sized chunks, and
    // `IdentityHasher` would silently return 0 rather than fail.
    assert_ne!(set_hash(&UstrPath::new("/some/long/path/segment.js")), 0);
  }

  #[test]
  fn equal_paths_always_hash_equal() {
    let a = UstrPath::new("/a/b");
    let b = UstrPath::new("/a/b");
    assert_eq!(a, b);
    assert_eq!(set_hash(&a), set_hash(&b));
  }

  #[test]
  // Unix only: there `Hash` forwards the interner's precomputed byte hash
  // directly. Windows folds spellings through `Path`'s component semantics
  // instead, so the two deliberately differ — that branch is covered by
  // `windows_spellings_of_one_path_are_equal_and_hash_alike` and by
  // `hash_delivers_eight_bytes_to_the_identity_hasher`.
  #[cfg(not(windows))]
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
    // `Debug`/`Display` render the path, not `Ustr`'s own `u!("...")` wrapper.
    // Platform-independent because interning is verbatim — the string comes
    // back exactly as passed in on every platform.
    let p = UstrPath::new("/a/b");
    assert_eq!(format!("{p:?}"), "\"/a/b\"");
    assert_eq!(format!("{p}"), "/a/b");
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
    // `IdentityHasher::write` silently produces a 0 hash unless it gets
    // exactly 8 bytes. `UstrPath::hash` goes through the default `write_u64`,
    // which forwards `u64::to_ne_bytes()` — exactly 8. Guard that invariant so
    // a future change to `Hash` cannot silently collapse every key to bucket 0.
    let mut hasher = IdentityHasher::default();
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

  // `equivalent_windows_spellings_intern_to_one_pointer` used to live here. It
  // asserted every spelling interned to one pointer, which only held while
  // `UstrPath::new` normalized. Interning is verbatim now, so the spellings are
  // distinct handles that compare and hash equal instead — asserted by
  // `windows_spellings_of_one_path_are_equal_and_hash_alike` above, which also
  // checks the strings really are stored distinctly.

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
