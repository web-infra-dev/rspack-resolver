// Configuration guide: https://rstack.rs/config
import path from "node:path";
import { define } from "rstack";

define.test({
  projects: [
    {
      name: "resolver",
      testEnvironment: "node",
      include: ["napi/__test__/**/*.test.mjs"],
      output: {
        externals: [
          ({ request }, callback) => {
            // Load native and WASI bindings at runtime instead of bundling them.
            if (
              request === "../index.js" ||
              request === "../resolver.wasi.cjs"
            ) {
              callback(
                undefined,
                `node-commonjs ${path.resolve("napi", path.basename(request))}`
              );
              return;
            }
            callback();
          }
        ]
      }
    },
    {
      name: "enhanced-resolve-compatible",
      testEnvironment: "node",
      include: ["napi/tests/**/*.test.mjs"],
      output: {
        externals: [
          ({ request }, callback) => {
            // Externalize the NAPI binding so native .node files load at runtime.
            if (request === "../index.js") {
              callback(
                undefined,
                `node-commonjs ${path.resolve("napi/index.js")}`
              );
              return;
            }
            callback();
          }
        ]
      }
    }
  ]
});

define.fmt({
  trailingComma: "none",
  arrowParens: "avoid",
  ignorePatterns: [
    "fixtures/tsconfig/tsconfig_broken.json",
    "fixtures/enhanced_resolve/test/fixtures/incorrect-package/pack1/package.json",
    "fixtures/enhanced_resolve/test/fixtures/tsconfig-paths/malformed-json/tsconfig.json",
    "**/.pnp.cjs",
    ".claude/worktrees",
    "bindings"
  ],
  overrides: [{ files: "*.json", options: { parser: "json" } }]
});

define.staged({
  "*.rs": [
    () => "cargo fmt",
    () => "cargo clippy --all-features -- -D warnings"
  ],
  "*.{ts,tsx,mts,js,mjs,yml,yaml}": "rs fmt",
  "package.json": "rs fmt",
  "*.toml": "pnpm exec taplo format"
});
