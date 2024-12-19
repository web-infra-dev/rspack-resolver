use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use tokio::runtime;

fn data() -> Vec<(PathBuf, &'static str)> {
    let cwd = env::current_dir().unwrap().join("fixtures/enhanced_resolve");
    let f = cwd.join("test/fixtures");
    vec![
        (cwd.clone(), "./"),
        (cwd.clone(), "./lib/index"),
        (cwd.clone(), "/absolute/path"),
        // query fragment
        (f.clone(), "./main1.js#fragment?query"),
        (f.clone(), "m1/a.js?query#fragment"),
        // browserField
        (f.join("browser-module"), "./lib/replaced"),
        (f.join("browser-module/lib"), "./replaced"),
        // exportsField
        (f.join("exports-field"), "exports-field"),
        (f.join("exports-field"), "exports-field/dist/main.js"),
        (f.join("exports-field"), "exports-field/dist/main.js?foo"),
        (f.join("exports-field"), "exports-field/dist/main.js#foo"),
        (f.join("exports-field"), "@exports-field/core"),
        (f.join("imports-exports-wildcard"), "m/features/f.js"),
        // extensionAlias
        (f.join("extension-alias"), "./index.js"),
        (f.join("extension-alias"), "./dir2/index.mjs"),
        // extensions
        (f.join("extensions"), "./foo"),
        (f.join("extensions"), "."),
        (f.join("extensions"), "./dir"),
        (f.join("extensions"), "module/"),
        // importsField
        (f.join("imports-field"), "#imports-field"),
        (f.join("imports-exports-wildcard/node_modules/m/"), "#internal/i.js"),
        // scoped
        (f.join("scoped"), "@scope/pack1"),
        (f.join("scoped"), "@scope/pack2/lib"),
        // dashed name
        (f.clone(), "dash"),
        (f.clone(), "dash-name"),
        (f.join("node_modules/dash"), "dash"),
        (f.join("node_modules/dash"), "dash-name"),
        (f.join("node_modules/dash-name"), "dash"),
        (f.join("node_modules/dash-name"), "dash-name"),
        // alias
        (cwd.clone(), "aaa"),
        (cwd.clone(), "ggg"),
        (cwd.clone(), "rrr"),
        (cwd.clone(), "@"),
        (cwd, "@@@"),
    ]
}

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
            symlink(temp_path.join("index.js"), temp_path.join(format!("file{i}.js")))?;
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

fn oxc_resolver() -> rspack_resolver::Resolver {
    use rspack_resolver::{AliasValue, ResolveOptions, Resolver};
    let alias_value = AliasValue::from("./");
    Resolver::new(ResolveOptions {
        extensions: vec![".ts".into(), ".js".into()],
        condition_names: vec!["webpack".into(), "require".into()],
        alias_fields: vec![vec!["browser".into()]],
        extension_alias: vec![
            (".js".into(), vec![".ts".into(), ".js".into()]),
            (".mjs".into(), vec![".mts".into()]),
        ],
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
    })
}

fn create_async_resolve_task(
    oxc_resolver: Arc<rspack_resolver::Resolver>,
    path: PathBuf,
    request: String,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let _ = oxc_resolver.resolve(path, &request);
    })
}

fn bench_resolver(c: &mut Criterion) {
    let data = data();

    let symlink_test_dir = create_symlinks().expect("Create symlink fixtures failed");

    let symlinks_range = 0u32..10000;

    let mut group = c.benchmark_group("resolver");

    group.bench_with_input(BenchmarkId::from_parameter("single-thread"), &data, |b, data| {
        let runner =
            runtime::Builder::new_current_thread().build().expect("failed to create tokio runtime");
        b.to_async(runner).iter(|| async {
            let oxc_resolver = oxc_resolver();
            for (path, request) in data {
                _ = oxc_resolver.resolve(path, request).await;
            }
        });
    });

    group.bench_with_input(BenchmarkId::from_parameter("multi-thread"), &data, |b, data| {
        let runner = runtime::Runtime::new().expect("failed to create tokio runtime");
        b.to_async(runner).iter(|| async {
            let oxc_resolver = Arc::new(oxc_resolver());

            let handles = data.iter().map(|(path, request)| {
                create_async_resolve_task(oxc_resolver.clone(), path.clone(), request.to_string())
            });
            for handle in handles {
                let _ = handle.await;
            }
        });
    });

    group.bench_with_input(
        BenchmarkId::from_parameter("resolve-from-symlinks"),
        &symlinks_range,
        |b, data| {
            let runner = runtime::Runtime::new().expect("failed to create tokio runtime");
            b.to_async(runner).iter(|| async {
                let oxc_resolver = oxc_resolver();
                for i in data.clone() {
                    assert!(
                        oxc_resolver
                            .resolve(&symlink_test_dir, &format!("./file{i}"))
                            .await
                            .is_ok(),
                        "file{i}.js"
                    );
                }
            });
        },
    );

    group.bench_with_input(
        BenchmarkId::from_parameter("resolve-from-symlinks-multi-thread"),
        &symlinks_range,
        |b, data| {
            let runner = runtime::Runtime::new().expect("failed to create tokio runtime");
            b.to_async(runner).iter(|| async {
                let oxc_resolver = Arc::new(oxc_resolver());

                let handles = data.clone().map(|i| {
                    create_async_resolve_task(
                        oxc_resolver.clone(),
                        symlink_test_dir.clone(),
                        format!("./file{i}").to_string(),
                    )
                });
                for handle in handles {
                    let _ = handle.await;
                }
            });
        },
    );
}

criterion_group!(resolver, bench_resolver);
criterion_main!(resolver);
