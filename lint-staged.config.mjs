export default {
  "*.rs": () => "cargo fmt",
  "*.{ts,tsx,js,mjs,yml,yaml}": "pnpm exec prettier --write",
  "package.json": "pnpm exec prettier --write",
  "*.toml": "pnpm exec taplo format"
};
