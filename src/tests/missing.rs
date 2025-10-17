//! https://github.com/webpack/enhanced-resolve/blob/main/test/missing.test.js

use normalize_path::NormalizePath;

use crate::{AliasValue, ResolveContext, ResolveOptions, Resolver};

#[tokio::test]
async fn test() {
  let f = super::fixture();

  let data = [
    (
      "./missing-file",
      vec![
        f.join("missing-file"),
        f.join("missing-file.js"),
        f.join("missing-file.node"),
      ],
    ),
    (
      "missing-module",
      vec![
        f.join("node_modules/missing-module"),
        f.parent().unwrap().join("node_modules"), // enhanced-resolve is "node_modules/missing-module"
      ],
    ),
    (
      "missing-module/missing-file",
      vec![
        f.join("node_modules/missing-module"),
        // f.parent().unwrap().join("node_modules/missing-module"), // we don't report this
      ],
    ),
    (
      "m1/missing-file",
      vec![
        f.join("node_modules/m1/missing-file"),
        f.join("node_modules/m1/missing-file.js"),
        f.join("node_modules/m1/missing-file.node"),
        // f.parent().unwrap().join("node_modules/m1"), // we don't report this
      ],
    ),
    (
      "m1/",
      vec![
        f.join("node_modules/m1/index"),
        f.join("node_modules/m1/index.js"),
        f.join("node_modules/m1/index.json"),
        f.join("node_modules/m1/index.node"),
      ],
    ),
    ("m1/a", vec![f.join("node_modules/m1/a")]),
  ];

  let resolver = Resolver::default();

  for (specifier, missing_dependencies) in data {
    let mut ctx = ResolveContext::default();
    let _ = resolver.resolve_with_context(&f, specifier, &mut ctx).await;

    for path in ctx.file_dependencies {
      assert_eq!(path, path.normalize(), "{path:?}");
    }

    for path in missing_dependencies {
      assert_eq!(path, path.normalize(), "{path:?}");
      assert!(
        ctx.missing_dependencies.contains(&path),
        "{specifier}: {path:?} not in {:?}",
        &ctx.missing_dependencies
      );
    }
  }
}

#[tokio::test]
async fn alias_and_extensions() {
  let f = super::fixture();

  let resolver = Resolver::new(ResolveOptions {
    alias: vec![
      (
        "@scope-js/package-name/dir$".into(),
        vec![AliasValue::Path(
          f.join("foo/index.js").to_string_lossy().to_string(),
        )],
      ),
      (
        "react-dom".into(),
        vec![AliasValue::Path(
          f.join("foo/index.js").to_string_lossy().to_string(),
        )],
      ),
    ],
    extensions: vec![".server.ts".into()],

    ..ResolveOptions::default()
  });

  let mut ctx = ResolveContext::default();
  let _ = resolver.resolve_with_context(&f, "@scope-js/package-name/dir/router", &mut ctx);
  let _ = resolver.resolve_with_context(&f, "react-dom/client", &mut ctx);

  for path in ctx.file_dependencies {
    assert_eq!(path, path.normalize(), "{path:?}");
  }

  for path in ctx.missing_dependencies {
    assert_eq!(path, path.normalize(), "{path:?}");
    if let Some(path) = path.parent() {
      assert!(!path.is_file(), "{path:?} must not be a file");
    }
  }
}

#[tokio::test]
async fn test_missing_in_symbol_linked_folder() {
  let workspace = super::fixture_root().join("pnpm-workspace");
  let app_path = workspace.join("packages/app");
  let missing_lib = workspace.join("packages/missing");

  let mut ctx = Default::default();
  let _ = Resolver::default()
    .resolve_with_context(&app_path, "@monorepo/missing", &mut ctx)
    .await;

  assert!(
    ctx
      .missing_dependencies
      .contains(&missing_lib.join("dist/cjs/index.js")),
    "real path of missing lib must be in missing dependencies"
  );

  assert!(
    !ctx
      .missing_dependencies
      .contains(&missing_lib.join("dist/cjs/index.js/index")),
    "should not load index when parent does not exist"
  )
}
