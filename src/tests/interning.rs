//! End-to-end guarantees for `UstrPath` interning.
//!
//! These assert on **pointer identity**, never on `ustr::num_entries()` or
//! `total_allocated()` — the interner is process-global and shared with every
//! other test running in parallel, so any global-count assertion is flaky by
//! construction.

use std::path::PathBuf;

use crate::{ResolveContext, ResolveOptions, Resolver};

/// `fixtures/enhanced_resolve/test/fixtures/extensions` — the same fixture and
/// options `src/tests/extensions.rs` resolves `./foo` against.
fn fixture_and_resolver() -> (PathBuf, Resolver) {
  let f = super::fixture().join("extensions");
  let resolver = Resolver::new(ResolveOptions {
    extensions: vec![".ts".into(), String::new(), ".js".into()],
    ..ResolveOptions::default()
  });
  (f, resolver)
}

#[tokio::test]
async fn repeated_resolves_share_one_pointer_per_dependency() {
  let (f, resolver) = fixture_and_resolver();

  let mut first: Option<Vec<(String, *const u8)>> = None;

  for _ in 0..3 {
    let mut ctx = ResolveContext::default();
    resolver
      .resolve_with_context(&f, "./foo", &mut ctx)
      .await
      .expect("should resolve");

    let mut snapshot: Vec<(String, *const u8)> = ctx
      .file_dependencies
      .iter()
      .map(|p| (p.as_str().to_owned(), p.as_str().as_ptr()))
      .collect();
    snapshot.sort_by(|a, b| a.0.cmp(&b.0));

    assert!(
      !snapshot.is_empty(),
      "the fixture must produce at least one file dependency, \
       otherwise this test passes without proving anything"
    );

    match &first {
      None => first = Some(snapshot),
      Some(baseline) => assert_eq!(
        baseline, &snapshot,
        "every dependency path must be the same interned pointer on every \
         resolve — a differing pointer means a fresh copy was allocated"
      ),
    }
  }
}

#[tokio::test]
async fn equal_paths_from_different_sources_are_one_pointer() {
  let (f, resolver) = fixture_and_resolver();

  let mut ctx = ResolveContext::default();
  resolver
    .resolve_with_context(&f, "./foo", &mut ctx)
    .await
    .expect("should resolve");

  assert!(
    !ctx.file_dependencies.is_empty(),
    "the fixture must produce at least one file dependency"
  );

  for dep in &ctx.file_dependencies {
    let reinterned = crate::UstrPath::new(dep.as_str());
    assert_eq!(
      dep.as_str().as_ptr(),
      reinterned.as_str().as_ptr(),
      "re-interning {dep} must hit the existing entry"
    );
  }
}

/// One-off memory evidence for the plan's headline claim: repeated pushes of
/// the same dependency path cost one interner entry, not one allocation per
/// push. Not run on normal `cargo test` — global counts are shared with every
/// other test in the binary, so they are only meaningful in isolation.
///
/// Run with: `cargo test --all-features -- --ignored --nocapture interning`
#[tokio::test]
#[ignore = "prints process-global interner stats; run in isolation with --ignored"]
async fn report_interner_memory_usage() {
  let (f, resolver) = fixture_and_resolver();

  for _ in 0..100 {
    let mut ctx = ResolveContext::default();
    resolver
      .resolve_with_context(&f, "./foo", &mut ctx)
      .await
      .expect("should resolve");
  }

  eprintln!(
    "interner: {} entries, {} bytes allocated",
    ustr::num_entries(),
    ustr::total_allocated()
  );
}
