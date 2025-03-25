use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use rayon::prelude::*;
use serde_json::Value;
use std::fs::read_to_string;
use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
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
    })
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
    for (path, request) in &data {
        let r = oxc_resolver().resolve(path, request);
        if !r.is_ok() {
            panic!("resolve failed {path:?} {request},\n\nplease run npm install in `/benches` before running the benchmarks");
        }
    }

    let symlink_test_dir = create_symlinks().expect("Create symlink fixtures failed");

    let symlinks_range = 0u32..10000;

    for i in symlinks_range.clone() {
        assert!(
            oxc_resolver().resolve(&symlink_test_dir, &format!("./file{i}")).is_ok(),
            "file{i}.js"
        );
    }

    let mut group = c.benchmark_group("resolver");

    rayon::ThreadPoolBuilder::new().build_global().expect("Failed to build global thread pool");

    group.bench_with_input(BenchmarkId::from_parameter("single-thread"), &data, |b, data| {
        let oxc_resolver = oxc_resolver();
        b.iter(|| {
            for (path, request) in data {
                _ = oxc_resolver.resolve(path, request);
            }
        });
    });

    group.bench_with_input(BenchmarkId::from_parameter("multi-thread"), &data, |b, data| {
        let oxc_resolver = oxc_resolver();

        b.iter(|| {
            data.par_iter().for_each(|(path, request)| {
                _ = oxc_resolver.resolve(path, request);
            });
        });
    });

    group.bench_with_input(
        BenchmarkId::from_parameter("resolve from symlinks"),
        &symlinks_range,
        |b, data| {
            let oxc_resolver = oxc_resolver();
            b.iter(|| {
                for i in data.clone() {
                    assert!(
                        oxc_resolver.resolve(&symlink_test_dir, &format!("./file{i}")).is_ok(),
                        "file{i}.js"
                    );
                }
            });
        },
    );
}

criterion_group!(resolver, bench_resolver);
criterion_main!(resolver);
