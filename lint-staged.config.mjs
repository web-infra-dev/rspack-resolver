export default {
  "*.rs": [
    () => "cargo fmt",
    () => "cargo clippy  --all-features -- -D warnings"
  ],
  "*.{ts,tsx,js,mjs}": "pnpm exec prettier --write",
  "package.json": "pnpm exec prettier --write",
  "*.toml": "pnpm exec taplo format"
};
