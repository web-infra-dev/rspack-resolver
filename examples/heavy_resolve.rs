use std::{env, fs::read_to_string, sync::Arc};

use rspack_resolver::Resolver;
use serde_json::Value;
use tokio::runtime::Builder;

fn main() {
  unsafe {
    sftrace_setup::setup();
  }

  let cwd = env::current_dir().unwrap().join("benches");

  let pkg_content = read_to_string("./benches/package.json").unwrap();
  let pkg_json: Value = serde_json::from_str(&pkg_content).unwrap();
  // about 1000 npm packages
  let data = pkg_json["dependencies"]
    .as_object()
    .unwrap()
    .keys()
    .map(|name| (&cwd, name))
    .collect::<Vec<_>>();

  let resolver = Resolver::new(Default::default());
  let resolver = Arc::new(resolver);

  let tokio_runtime = Builder::new_multi_thread()
    .max_blocking_threads(256)
    .build()
    .expect("failed to create tokio runtime");

  tokio_runtime.block_on(async move {
    for (path, name) in data {
      let request = format!("{}", name);
      let p = path.clone();
      let r = request.clone();
      let _ = resolver.resolve(p, &r).await;
    }
  });
}
