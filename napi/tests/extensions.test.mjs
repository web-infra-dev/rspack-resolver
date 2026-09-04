import { describe, it, expect } from "rstack/test";
import { ResolverFactory } from "../index.js";
import * as path from "node:path";

const fixtureDir = path.resolve("fixtures/enhanced_resolve/test/fixtures");
const fixture = path.resolve(fixtureDir, "extensions");

describe("extensions", () => {
  const resolver = new ResolverFactory({
    extensions: [".ts", ".js"]
  });

  it("should resolve according to order of provided extensions", () => {
    const result = resolver.sync(fixture, "./foo");
    expect(result.path).toBe(path.resolve(fixture, "foo.ts"));
  });

  it("should resolve according to order of provided extensions (dir index)", () => {
    const result = resolver.sync(fixture, "./dir");
    expect(result.path).toBe(path.resolve(fixture, "dir/index.ts"));
  });

  it("should resolve according to main field in module root", () => {
    const result = resolver.sync(fixture, ".");
    expect(result.path).toBe(path.resolve(fixture, "index.js"));
  });

  it("should resolve single file module before directory", () => {
    const result = resolver.sync(fixture, "module");
    expect(result.path).toBe(path.resolve(fixture, "node_modules/module.js"));
  });

  it("should resolve trailing slash directory before single file", () => {
    const result = resolver.sync(fixture, "module/");
    expect(result.path).toBe(
      path.resolve(fixture, "node_modules/module/index.ts")
    );
  });

  it("should not resolve to file when request has a trailing slash (relative)", () => {
    const result = resolver.sync(fixture, "./foo.js/");
    expect(result.error).toBeTruthy();
  });

  it("should not resolve to file when request has a trailing slash (module)", () => {
    const result = resolver.sync(fixture, "module.js/");
    expect(result.error).toBeTruthy();
  });
});
