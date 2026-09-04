import { describe, it, expect } from "rstack/test";
import { ResolverFactory } from "../index.js";
import * as path from "node:path";

const fixtureDir = path.resolve("fixtures/enhanced_resolve/test/fixtures");
const testDir = path.resolve(fixtureDir, "..");

describe("roots", () => {
  const resolver = new ResolverFactory({
    extensions: [".js"],
    alias: {
      foo: "/fixtures"
    },
    roots: [testDir, fixtureDir]
  });

  it("should respect roots option", () => {
    const result = resolver.sync(fixtureDir, "/fixtures/b.js");
    expect(result.path).toBe(path.resolve(fixtureDir, "b.js"));
  });

  it("should try another root option, if it exists", () => {
    const result = resolver.sync(fixtureDir, "/b.js");
    expect(result.path).toBe(path.resolve(fixtureDir, "b.js"));
  });

  it("should respect extension", () => {
    const result = resolver.sync(fixtureDir, "/fixtures/b");
    expect(result.path).toBe(path.resolve(fixtureDir, "b.js"));
  });

  it("should resolve in directory", () => {
    const result = resolver.sync(fixtureDir, "/fixtures/extensions/dir");
    expect(result.path).toBe(
      path.resolve(fixtureDir, "extensions/dir/index.js")
    );
  });

  it("should respect aliases", () => {
    const result = resolver.sync(fixtureDir, "foo/b");
    expect(result.path).toBe(path.resolve(fixtureDir, "b.js"));
  });

  it("should support roots options with resolveToContext", () => {
    const contextResolver = new ResolverFactory({
      roots: [testDir],
      resolveToContext: true
    });
    const result = contextResolver.sync(fixtureDir, "/fixtures/lib");
    expect(result.path).toBe(path.resolve(fixtureDir, "lib"));
  });

  it("should not work with relative path", () => {
    const result = resolver.sync(fixtureDir, "fixtures/b.js");
    expect(result.error).toBeTruthy();
  });

  it("should resolve an absolute path (prefer absolute)", () => {
    const resolverPreferAbsolute = new ResolverFactory({
      extensions: [".js"],
      alias: {
        foo: "/fixtures"
      },
      roots: [testDir, fixtureDir],
      preferAbsolute: true
    });
    const result = resolverPreferAbsolute.sync(
      fixtureDir,
      path.join(fixtureDir, "b.js")
    );
    expect(result.path).toBe(path.resolve(fixtureDir, "b.js"));
  });
});
