use rspack_resolver::{ResolveOptions, Resolver};

fn main() {
    println!("Hello, world!");

    let r = Resolver::new(ResolveOptions {
        condition_names: vec!["import".into()],
        enable_pnp: true,
        ..Default::default()
    });

    let x = r.resolve("/Users/bytedance/git/rspack-resolver/node_modules/.pnpm/preact@10.25.4/node_modules/preact",
        "."
    );

    print!("{:?}", x);
}
