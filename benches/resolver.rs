use std::{
  env, fs,
  fs::read_to_string,
  future::Future,
  io::{self, Write},
  path::{Path, PathBuf},
  sync::Arc,
};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rspack_resolver::{FileSystemOptions, FileSystemOs, ResolveOptions, Resolver};
use serde_json::Value;
use tokio::{
  runtime::{self, Builder},
  task::JoinSet,
};

fn symlink<P: AsRef<Path>, Q: AsRef<Path>>(original: P, link: Q) -> io::Result<()> {
  #[cfg(target_family = "unix")]
  {
    std::os::unix::fs::symlink(original, link)
  }

  #[cfg(target_family = "windows")]
  {
    std::os::windows::fs::symlink_file(original, link)
  }
}

fn create_symlinks() -> io::Result<PathBuf> {
  let root = env::current_dir()?.join("fixtures/enhanced_resolve");
  let dirname = root.join("test");
  let temp_path = dirname.join("temp_symlinks");
  let create_symlink_fixtures = || -> io::Result<()> {
    fs::create_dir(&temp_path)?;
    let mut index = fs::File::create(temp_path.join("index.js"))?;
    index.write_all(b"console.log('Hello, World!')")?;
    // create 10000 symlink files pointing to the index.js
    for i in 0..10000 {
      symlink(
        temp_path.join("index.js"),
        temp_path.join(format!("file{i}.js")),
      )?;
    }
    Ok(())
  };
  if !temp_path.exists() {
    if let Err(err) = create_symlink_fixtures() {
      let _ = fs::remove_dir_all(&temp_path);
      return Err(err);
    }
  }
  Ok(temp_path)
}

fn rspack_resolver(enable_pnp: bool) -> rspack_resolver::Resolver {
  use rspack_resolver::{AliasValue, ResolveOptions, Resolver};
  let alias_value = AliasValue::from("./");

  let fs = FileSystemOs::new(FileSystemOptions {
    #[cfg(feature = "yarn_pnp")]
    enable_pnp,
  });

  Resolver::new_with_file_system(
    fs,
    ResolveOptions {
      enable_pnp,
      extensions: vec![".ts".into(), ".js".into(), ".mjs".into()],
      condition_names: vec!["import".into(), "webpack".into(), "require".into()],
      alias_fields: vec![vec!["browser".into()]],
      extension_alias: vec![(".js".into(), vec![".ts".into(), ".js".into()])],
      // Real projects LOVE setting these many aliases.
      // I saw them with my own eyes.
      alias: vec![
        ("/absolute/path".into(), vec![alias_value.clone()]),
        ("aaa".into(), vec![alias_value.clone()]),
        ("bbb".into(), vec![alias_value.clone()]),
        ("ccc".into(), vec![alias_value.clone()]),
        ("ddd".into(), vec![alias_value.clone()]),
        ("eee".into(), vec![alias_value.clone()]),
        ("fff".into(), vec![alias_value.clone()]),
        ("ggg".into(), vec![alias_value.clone()]),
        ("hhh".into(), vec![alias_value.clone()]),
        ("iii".into(), vec![alias_value.clone()]),
        ("jjj".into(), vec![alias_value.clone()]),
        ("kkk".into(), vec![alias_value.clone()]),
        ("lll".into(), vec![alias_value.clone()]),
        ("mmm".into(), vec![alias_value.clone()]),
        ("nnn".into(), vec![alias_value.clone()]),
        ("ooo".into(), vec![alias_value.clone()]),
        ("ppp".into(), vec![alias_value.clone()]),
        ("qqq".into(), vec![alias_value.clone()]),
        ("rrr".into(), vec![alias_value.clone()]),
        ("sss".into(), vec![alias_value.clone()]),
        ("@".into(), vec![alias_value.clone()]),
        ("@@".into(), vec![alias_value.clone()]),
        ("@@@".into(), vec![alias_value]),
      ],
      ..ResolveOptions::default()
    },
  )
}

fn resolver_with_many_extensions() -> rspack_resolver::Resolver {
  Resolver::new(ResolveOptions {
    extensions: vec![
      ".bad0".to_string(),
      ".bad1".to_string(),
      ".bad2".to_string(),
      ".bad3".to_string(),
      ".bad4".to_string(),
      ".bad5".to_string(),
      ".bad6".to_string(),
      ".bad7".to_string(),
      ".bad8".to_string(),
      ".bad9".to_string(),
      ".mtsx".to_string(),
      ".mts".to_string(),
      ".mjs".to_string(),
      ".tsx".to_string(),
      ".ts".to_string(),
      ".jsx".to_string(),
      ".js".to_string(),
    ],
    imports_fields: vec![],
    exports_fields: vec![],
    enable_pnp: false,
    ..Default::default()
  })
}

fn create_async_resolve_task(
  rspack_resolver: Arc<rspack_resolver::Resolver>,
  path: PathBuf,
  request: String,
) -> impl Future<Output = ()> {
  async move {
    let _ = rspack_resolver.resolve(path, &request).await;
  }
}

fn bench_resolver(c: &mut Criterion) {
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

  // check validity
  runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
        for (path, request) in &data {
            let r = rspack_resolver(false).resolve(path, request).await;
            if !r.is_ok() {
                panic!("resolve failed {path:?} {request},\n\nplease run `pnpm install --ignore-workspace` in `/benches` before running the benchmarks");
            }
        }
    });

  let symlink_test_dir = create_symlinks().expect("Create symlink fixtures failed");

  let symlinks_range = 0u32..10000;

  // check validity
  runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .unwrap()
    .block_on(async {
      for i in symlinks_range.clone() {
        assert!(
          rspack_resolver(false)
            .resolve(&symlink_test_dir, &format!("./file{i}"))
            .await
            .is_ok(),
          "file{i}.js"
        );
      }
    });

  let mut group = c.benchmark_group("resolver");

  // codspeed can only handle to up to 500 threads
  let multi_rt = || {
    Builder::new_multi_thread()
      .max_blocking_threads(256)
      .build()
      .expect("failed to create tokio runtime")
  };

  // force to use four threads
  rayon::ThreadPoolBuilder::new()
    .num_threads(4)
    .build_global()
    .expect("Failed to build global thread pool");

  group.bench_with_input(
    BenchmarkId::from_parameter("single-thread"),
    &data,
    |b, data| {
      let runner = runtime::Builder::new_current_thread()
        .build()
        .expect("failed to create tokio runtime");
      let rspack_resolver = rspack_resolver(false);

      b.to_async(runner).iter_with_setup(
        || {
          rspack_resolver.clear_cache();
        },
        |_| async {
          for (path, request) in data {
            _ = rspack_resolver.resolve(path, request).await;
          }
        },
      );
    },
  );

  group.bench_with_input(
    BenchmarkId::from_parameter("[single-threaded]resolve with many extensions"),
    &data,
    |b, data| {
      let runner = runtime::Builder::new_current_thread()
        .build()
        .expect("failed to create tokio runtime");
      let rspack_resolver = resolver_with_many_extensions();

      b.to_async(runner).iter_with_setup(
        || {
          rspack_resolver.clear_cache();
        },
        |_| async {
          for (path, request) in data {
            _ = rspack_resolver
              .resolve(path, &format!("{}/bad", request))
              .await;
          }
        },
      );
    },
  );

  group.bench_with_input(
    BenchmarkId::from_parameter("multi-thread"),
    &data,
    |b, data| {
      let runner = multi_rt();
      let rspack_resolver = Arc::new(rspack_resolver(false));

      b.iter_with_setup(
        || {
          rspack_resolver.clear_cache();
        },
        |_| {
          runner.block_on(async {
            let mut join_set = JoinSet::new();
            data.iter().for_each(|(path, request)| {
              join_set.spawn(create_async_resolve_task(
                rspack_resolver.clone(),
                path.to_path_buf(),
                request.to_string(),
              ));
            });
            let _ = join_set.join_all().await;
          });
        },
      );
    },
  );

  group.bench_with_input(
    BenchmarkId::from_parameter("resolve from symlinks"),
    &symlinks_range,
    |b, data| {
      let runner = runtime::Runtime::new().expect("failed to create tokio runtime");
      let rspack_resolver = rspack_resolver(false);

      b.to_async(runner).iter_with_setup(
        || {
          rspack_resolver.clear_cache();
        },
        |_| async {
          for i in data.clone() {
            assert!(
              rspack_resolver
                .resolve(&symlink_test_dir, &format!("./file{i}"))
                .await
                .is_ok(),
              "file{i}.js"
            );
          }
        },
      );
    },
  );

  group.bench_with_input(
    BenchmarkId::from_parameter("resolve from symlinks multi thread"),
    &symlinks_range,
    |b, data| {
      let runner = multi_rt();
      let rspack_resolver = Arc::new(rspack_resolver(false));

      let symlink_test_dir = symlink_test_dir.clone();

      b.to_async(runner).iter(|| async {
        let mut join_set = JoinSet::new();

        data.clone().for_each(|i| {
          join_set.spawn(create_async_resolve_task(
            rspack_resolver.clone(),
            symlink_test_dir.clone(),
            format!("./file{i}").to_string(),
          ));
        });
        join_set.join_all().await;
      });
    },
  );

  let pnp_workspace = env::current_dir().unwrap().join("fixtures/pnp");
  let root_range = 1..11;

  group.bench_with_input(
    BenchmarkId::from_parameter("pnp resolve"),
    &root_range,
    |b, data| {
      let runner = runtime::Runtime::new().expect("failed to create tokio runtime");
      let rspack_resolver = Arc::new(rspack_resolver(true));

      b.to_async(runner).iter_with_setup(
        || {
          rspack_resolver.clear_cache();
        },
        |_| async {
          for i in data.clone() {
            let _ = rspack_resolver
              .resolve(pnp_workspace.join(format!("{i}")), "preact")
              .await;
          }
        },
      );
    },
  );
}

criterion_group!(resolver, bench_resolver);
criterion_main!(resolver);
