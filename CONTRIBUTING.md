# Contributing to the Project

Thanks A lof for your interest in contributing to this project!
We welcome contributions from everyone. Below are some guidelines to help you get started.

Rspack-Resolver is built using [Rust](https://www.rust-lang.org/) and [NAPI-RS](https://napi.rs/),
then released as both npm [package](https://www.npmjs.com/package/@rspack/resolver) and Rust [crate](https://crates.io/crates/rspack_resolver).

## Prerequisites

Rspack is built using [Rust](https://rust-lang.org/) and [NAPI-RS](https://napi.rs/), then released as [Node.js](https://nodejs.org/) packages.

### Setup Rust

- Install Rust using [rustup](https://rustup.rs/).
- If you are using VS Code, we recommend installing the [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer) extension.

### Setup Node.js

#### Install Node.js

We recommend using the LTS version of [Node.js 22](https://nodejs.org/en/about/previous-releases).

Check the current Node.js version with the following command:

```bash
node -v
```

If you do not have Node.js installed in your current environment, you can use [nvm](https://github.com/nvm-sh/nvm) or [fnm](https://github.com/Schniz/fnm) to install it.

Here is an example of how to install via nvm:

```bash
# Install Node.js LTS
nvm install 22 --lts

# Switch to Node.js LTS
nvm use 22
```

## Building and Testing

```bash
# Build @rspack/resolver's node release binding.
npm run build
# or
npm run build:release
```

You can switch to `profiling` and `debug` profile by `npm run build:profiling` and `npm run build:debug` respectively.

```bash
# Run all Rust tests
cargo test
```

```bash
# Run all Node.js tests
npm run test
```

## Releasing

### Publish Crate

1. create a release branch as `release/x.y.z`
2. Bump version in `Cargo.toml`
3. Commit and push the release branch
4. Run the `release-plz.yml` workflow in GitHub Actions to publish the crate to [crates.io](https://crates.io/crates/rspack_resolver).

### Publish NPM Package

In most cases, We publish @rspack/resolver npm package after rspack-resolver crate.

1. Bump version by `./x version <major|minor|patch>` in the root directory
2. Commit and push the release branch
3. Run the `release-npm.yml` workflow in GitHub Actions to publish the package to [npm](https://www.npmjs.com/package/@rspack/resolver).
