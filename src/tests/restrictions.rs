//! <https://github.com/webpack/enhanced-resolve/blob/main/test/restrictions.test.js>

use std::sync::Arc;

use regex::Regex;

use crate::{ResolveError, ResolveOptions, Resolver, Restriction};

#[tokio::test]
async fn should_respect_regexp_restriction() {
  let f = super::fixture().join("restrictions");

  let re = Regex::new(r"\.(sass|scss|css)$").unwrap();
  let resolver1 = Resolver::new(ResolveOptions {
    extensions: vec![".js".into()],
    restrictions: vec![Restriction::Fn(Arc::new(move |path| {
      path.as_os_str().to_str().map_or(false, |s| re.is_match(s))
    }))],
    ..ResolveOptions::default()
  });

  let resolution = resolver1.resolve(&f, "pck1").await.map(|r| r.full_path());
  assert_eq!(resolution, Err(ResolveError::NotFound("pck1".to_string())));
}

#[tokio::test]
async fn should_try_to_find_alternative_1() {
  let f = super::fixture().join("restrictions");

  let re = Regex::new(r"\.(sass|scss|css)$").unwrap();
  let resolver1 = Resolver::new(ResolveOptions {
    extensions: vec![".js".into(), ".css".into()],
    main_files: vec!["index".into()],
    restrictions: vec![Restriction::Fn(Arc::new(move |path| {
      path.as_os_str().to_str().map_or(false, |s| re.is_match(s))
    }))],
    ..ResolveOptions::default()
  });

  let resolution = resolver1.resolve(&f, "pck1").await.map(|r| r.full_path());
  assert_eq!(resolution, Ok(f.join("node_modules/pck1/index.css")));
}

#[tokio::test]
async fn should_respect_string_restriction() {
  let fixture = super::fixture();
  let f = fixture.join("restrictions");

  let resolver = Resolver::new(ResolveOptions {
    extensions: vec![".js".into()],
    restrictions: vec![Restriction::Path(f.clone())],
    ..ResolveOptions::default()
  });

  let resolution = resolver.resolve(&f, "pck2").await;
  assert_eq!(resolution, Err(ResolveError::NotFound("pck2".to_string())));
}

#[tokio::test]
async fn should_allow_descendant_of_string_restriction() {
  let f = super::fixture().join("restrictions");

  let resolver = Resolver::new(ResolveOptions {
    extensions: vec![".js".into()],
    restrictions: vec![Restriction::Path(f.clone())],
    ..ResolveOptions::default()
  });

  let resolution = resolver.resolve(&f, "pck1").await.map(|r| r.full_path());
  assert_eq!(resolution, Ok(f.join("node_modules/pck1/index.js")));
}

#[tokio::test]
async fn should_reject_sibling_sharing_textual_prefix() {
  let f = super::fixture().join("restrictions");

  let resolver = Resolver::new(ResolveOptions {
    extensions: vec![".js".into()],
    restrictions: vec![Restriction::Path(f.join("node_modules/pck"))],
    ..ResolveOptions::default()
  });

  let resolution = resolver.resolve(&f, "pck1").await.map(|r| r.full_path());
  assert_eq!(resolution, Err(ResolveError::NotFound("pck1".to_string())));
}

#[tokio::test]
async fn should_try_to_find_alternative_2() {
  let f = super::fixture().join("restrictions");

  let re = Regex::new(r"\.(sass|scss|css)$").unwrap();
  let resolver1 = Resolver::new(ResolveOptions {
    extensions: vec![".js".into(), ".css".into()],
    main_fields: vec!["main".into(), "style".into()],
    restrictions: vec![Restriction::Fn(Arc::new(move |path| {
      path.as_os_str().to_str().map_or(false, |s| re.is_match(s))
    }))],
    ..ResolveOptions::default()
  });

  let resolution = resolver1.resolve(&f, "pck2").await.map(|r| r.full_path());
  assert_eq!(resolution, Ok(f.join("node_modules/pck2/index.css")));
}

#[tokio::test]
async fn should_try_to_find_alternative_3() {
  let f = super::fixture().join("restrictions");

  let re = Regex::new(r"\.(sass|scss|css)$").unwrap();
  let resolver1 = Resolver::new(ResolveOptions {
    extensions: vec![".js".into()],
    main_fields: vec!["main".into(), "module".into(), "style".into()],
    restrictions: vec![Restriction::Fn(Arc::new(move |path| {
      path.as_os_str().to_str().map_or(false, |s| re.is_match(s))
    }))],
    ..ResolveOptions::default()
  });

  let resolution = resolver1.resolve(&f, "pck2").await.map(|r| r.full_path());
  assert_eq!(resolution, Ok(f.join("node_modules/pck2/index.css")));
}

#[tokio::test]
async fn should_try_to_find_alternative_4() {
  let f = super::fixture().join("restrictions");

  let re = Regex::new(r"\.(sass|scss|css)$").unwrap();
  let resolver1 = Resolver::new(ResolveOptions {
    extensions: vec![".css".into()],
    main_fields: vec!["main".into()],
    extension_alias: vec![(".js".into(), vec![".js".into(), ".jsx".into()])],
    restrictions: vec![Restriction::Fn(Arc::new(move |path| {
      path.as_os_str().to_str().map_or(false, |s| re.is_match(s))
    }))],
    ..ResolveOptions::default()
  });

  let resolution = resolver1.resolve(&f, "pck2").await.map(|r| r.full_path());
  assert_eq!(resolution, Ok(f.join("node_modules/pck2/index.css")));
}

/// Ported from enhanced-resolve `restrictions > path boundaries`
/// <https://github.com/webpack/enhanced-resolve/commit/d8693b6>
///
/// `MemoryFS` always separates with `/`, so these run on non-Windows only.
#[cfg(not(target_os = "windows"))]
mod path_boundaries {
  use std::path::PathBuf;

  use super::super::memory_fs::MemoryFS;
  use crate::{ResolveOptions, ResolverGeneric, Restriction};

  async fn resolves(restriction: &str, context: &str, request: &str, file: &'static str) -> bool {
    let resolver = ResolverGeneric::<MemoryFS>::new_with_file_system(
      MemoryFS::new(&[(file, "")]),
      ResolveOptions {
        extensions: vec![".js".into()],
        restrictions: vec![Restriction::Path(PathBuf::from(restriction))],
        ..ResolveOptions::default()
      },
    );
    resolver.resolve(context, request).await.is_ok()
  }

  #[tokio::test]
  async fn file_inside_a_restriction() {
    assert!(resolves("/a/b/c", "/a/b/c", "./index.js", "/a/b/c/index.js").await);
  }

  #[tokio::test]
  async fn sibling_of_a_restriction() {
    assert!(!resolves("/a/b/c", "/a/b", "./c-other.js", "/a/b/c-other.js").await);
  }

  #[tokio::test]
  async fn sibling_of_a_restriction_separated_by_a_backslash() {
    assert!(!resolves("/a/b/c", "/a/b", "./c\\sibling.js", "/a/b/c\\sibling.js").await);
  }

  #[tokio::test]
  async fn sibling_of_a_restriction_containing_a_backslash() {
    assert!(!resolves("/a/b\\c", "/a", "./b\\c\\sibling.js", "/a/b\\c\\sibling.js").await);
  }

  #[tokio::test]
  async fn file_inside_a_restriction_ending_with_a_separator() {
    assert!(resolves("/a/b/c/", "/a/b/c", "./index.js", "/a/b/c/index.js").await);
  }

  #[tokio::test]
  async fn file_inside_the_root_restriction() {
    assert!(resolves("/", "/a", "./index.js", "/a/index.js").await);
  }

  #[tokio::test]
  async fn file_inside_a_non_normalized_restriction() {
    assert!(resolves("/a/x/../b/c", "/a/b/c", "./index.js", "/a/b/c/index.js").await);
  }

  #[tokio::test]
  async fn empty_restriction_matches_every_path() {
    assert!(resolves("", "/a/b/c", "./index.js", "/a/b/c/index.js").await);
  }

  #[tokio::test]
  async fn relative_restriction_matches_no_absolute_path() {
    assert!(!resolves(".", "/a/b/c", "./index.js", "/a/b/c/index.js").await);
    assert!(!resolves("..", "/a/b/c", "./index.js", "/a/b/c/index.js").await);
    assert!(!resolves("foo/..", "/a/b/c", "./index.js", "/a/b/c/index.js").await);
  }

  #[tokio::test]
  async fn file_inside_a_restriction_differing_in_case() {
    assert!(!resolves("/A/B/C", "/a/b/c", "./index.js", "/a/b/c/index.js").await);
  }
}

/// Ported from enhanced-resolve `restrictions > windows and posix semantics`
/// <https://github.com/webpack/enhanced-resolve/commit/d8693b6>
///
/// Windows path forms cannot travel through a resolver on a posix host, so the
/// check is driven directly, the way upstream drives its plugin.
#[cfg(target_os = "windows")]
mod windows_path_boundaries {
  use std::path::PathBuf;

  use camino::Utf8Path;

  use crate::{ResolveOptions, Resolver, Restriction};

  fn is_inside(restriction: &str, path: &str) -> bool {
    let resolver = Resolver::new(ResolveOptions {
      restrictions: vec![Restriction::Path(PathBuf::from(restriction))],
      ..ResolveOptions::default()
    });
    resolver.check_restrictions(Utf8Path::new(path))
  }

  #[tokio::test]
  async fn path_using_slashes_under_a_backslash_restriction() {
    assert!(is_inside(r"C:\a\b\c", "C:/a/b/c/index.js"));
  }

  #[tokio::test]
  async fn path_mixing_separators_under_a_slash_restriction() {
    assert!(is_inside("C:/a/b/c", r"C:\a\b/c/index.js"));
  }

  #[tokio::test]
  async fn path_under_a_restriction_ending_with_a_slash() {
    assert!(is_inside(r"C:\a\b\c/", r"C:\a\b\c\index.js"));
  }

  #[tokio::test]
  async fn the_restricted_directory_itself() {
    assert!(is_inside(r"C:\a\b\c\", r"C:\a\b\c"));
  }

  #[tokio::test]
  async fn path_whose_drive_letter_differs_in_case() {
    assert!(is_inside(r"c:\a\b\c", r"C:\a\b\c\index.js"));
  }

  #[tokio::test]
  async fn path_inside_a_unc_restriction() {
    assert!(is_inside(r"\\server\share\a", r"\\server\share\a\index.js"));
  }

  #[tokio::test]
  async fn path_inside_a_dos_device_restriction() {
    assert!(is_inside(r"\\?\C:\a", r"\\?\C:\a\index.js"));
  }

  #[tokio::test]
  async fn path_on_another_share_than_the_unc_restriction() {
    assert!(!is_inside(
      r"\\server\share\a",
      r"\\server\other\a\index.js"
    ));
  }

  #[tokio::test]
  async fn sibling_of_a_restriction_written_with_slashes() {
    assert!(!is_inside(r"C:\a\b\c", "C:/a/b/c-other.js"));
  }
}
