//! Not part of enhanced_resolve's test suite
//!
//! enhanced_resolve's test <https://github.com/webpack/enhanced-resolve/blob/main/test/pnp.test.js>
//! cannot be ported over because it uses mocks on `pnpApi` provided by the runtime.

use crate::{path::PathUtil, ResolveError::NotFound, ResolveOptions, Resolver};

#[tokio::test]
async fn pnp1() {
  let fixture = super::fixture_root().join("pnp");

  let resolver = Resolver::new(ResolveOptions {
    extensions: vec![".js".into()],
    condition_names: vec!["import".into()],
    ..ResolveOptions::default()
  });

  assert_eq!(
    resolver
      .resolve(&fixture, "is-even")
      .await
      .map(|r| r.full_path()),
    Ok(fixture.join(
      ".yarn/cache/is-even-npm-1.0.0-9f726520dc-2728cc2f39.zip/node_modules/is-even/index.js"
    ))
  );

  assert_eq!(
    resolver
      .resolve(&fixture, "lodash.zip")
      .await
      .map(|r| r.full_path()),
    Ok(fixture.join(
      ".yarn/cache/lodash.zip-npm-4.2.0-5299417ec8-e596da80a6.zip/node_modules/lodash.zip/index.js"
    ))
  );

  assert_eq!(
    resolver
      .resolve(
        fixture
          .join(".yarn/cache/is-even-npm-1.0.0-9f726520dc-2728cc2f39.zip/node_modules/is-even"),
        "is-odd"
      )
      .await
      .map(|r| r.full_path()),
    Ok(
      fixture.join(
        ".yarn/cache/is-odd-npm-0.1.2-9d980a9da8-7dc6c6fd00.zip/node_modules/is-odd/index.js"
      )
    ),
  );

  assert_eq!(
    resolver
      .resolve(&fixture, "is-odd")
      .await
      .map(|r| r.full_path()),
    Ok(
      fixture.join(
        ".yarn/cache/is-odd-npm-3.0.1-93c3c3f41b-89ee2e353c.zip/node_modules/is-odd/index.js"
      )
    ),
  );

  assert_eq!(
        resolver.resolve(&fixture, "preact").await.map(|r| r.full_path()),
        Ok(fixture.join(
            ".yarn/cache/preact-npm-10.25.4-2dd2c0aa44-33a009d614.zip/node_modules/preact/dist/preact.mjs"
        )),
    );

  assert_eq!(
        resolver.resolve(&fixture, "preact/devtools").await.map(|r| r.full_path()),
        Ok(fixture.join(
            ".yarn/cache/preact-npm-10.25.4-2dd2c0aa44-33a009d614.zip/node_modules/preact/devtools/dist/devtools.mjs"
        )),
    );
}

#[tokio::test]
async fn pnp_resolve_description_file() {
  let fixture = super::fixture_root().join("pnp");

  let resolver = Resolver::new(ResolveOptions {
    extensions: vec![".js".into()],
    condition_names: vec!["import".into()],
    ..ResolveOptions::default()
  });

  let full_path = fixture
    .join(
      ".yarn/cache/preact-npm-10.25.4-2dd2c0aa44-33a009d614.zip/node_modules/preact/dist/preact.js",
    )
    .to_string_lossy()
    .to_string();

  let r = resolver.resolve(&fixture, &full_path).await.unwrap();

  assert_eq!(
    r.package_json.unwrap().path.to_string_lossy().to_string(),
    fixture
      .join(".yarn/cache/preact-npm-10.25.4-2dd2c0aa44-33a009d614.zip/node_modules/preact")
      .join("package.json")
      .normalize()
      .to_string_lossy()
      .to_string()
  );
}

#[tokio::test]
async fn resolve_in_pnp_linked_folder() {
  let fixture = super::fixture_root().join("pnp");

  let resolver = Resolver::new(ResolveOptions {
    extensions: vec![".js".into()],
    condition_names: vec!["import".into()],
    ..ResolveOptions::default()
  });

  assert_eq!(
    resolver
      .resolve(&fixture, "lib/lib.js")
      .await
      .map(|r| r.full_path()),
    Ok(fixture.join("shared/lib.js"))
  );
}

#[tokio::test]
async fn resolve_pnp_pkg_should_failed_while_disable_pnp_mode() {
  let fixture = super::fixture_root().join("pnp");

  let resolver = Resolver::new(ResolveOptions {
    enable_pnp: false,
    ..ResolveOptions::default()
  });

  assert_eq!(
    resolver
      .resolve(&fixture, "is-even")
      .await
      .map(|r| r.full_path()),
    Err(NotFound("is-even".to_string()))
  );
}
