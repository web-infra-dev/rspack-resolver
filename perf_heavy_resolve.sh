#!/usr/bin/env  bash

export SFTRACE_DYLIB_DIR=/Users/bytedance/git/sftrace/target/release

cargo clean

env SFTRACE_DYLIB_DIR=/Users/bytedance/git/sftrace/target/release \
    cargo build --profile profiling --example  heavy_resolve

lldb \
    -O "env DYLD_INSERT_LIBRARIES=/Users/bytedance/git/sftrace/target/release/libsftrace.dylib" \
    -O "env SFTRACE_OUTPUT_FILE=/Users/bytedance/git/rspack-resolver/sf.log" \
    target/profiling/examples/heavy_resolve
