use std::{env, fs::read_to_string, sync::Arc};

use rspack_resolver::Resolver;
use serde_json::Value;
use tokio::task::JoinSet;

#[tokio::main]
async fn main() {
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

  let mut join_set = JoinSet::new();

  for (path, name) in data {
    let request = format!("{}", name);
    let p = path.clone();
    let r = request.clone();
    let fut = async move {
      let resolver = Resolver::default();
      resolver.resolve(p, &r).await
    };

    join_set.spawn(fut);
    // println!("{:?}", r);
  }

  _ = join_set.join_all().await;
}
