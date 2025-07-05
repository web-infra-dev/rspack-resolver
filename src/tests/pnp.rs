//! Not part of enhanced_resolve's test suite
//!
//! enhanced_resolve's test <https://github.com/webpack/enhanced-resolve/blob/main/test/pnp.test.js>
//! cannot be ported over because it uses mocks on `pnpApi` provided by the runtime.

use crate::ResolveError::NotFound;
use crate::{ResolveOptions, Resolver};

#[tokio::test]
async fn pnp1() {
    let fixture = super::fixture_root().join("pnp");

    let resolver = Resolver::new(ResolveOptions {
        extensions: vec![".js".into()],
        condition_names: vec!["import".into()],
        ..ResolveOptions::default()
    });

    assert_eq!(
        resolver.resolve(&fixture, "is-even").await.map(|r| r.full_path()),
        Ok(fixture.join(
            ".yarn/cache/is-even-npm-1.0.0-9f726520dc-2728cc2f39.zip/node_modules/is-even/index.js"
        ))
    );
}

