import { describe, it, expect } from "rstack/test";
import { ResolverFactory } from "../index.js";
import * as path from "node:path";

const fixtures = path.resolve("fixtures/enhanced_resolve/test/fixtures");

function testResolve(resolver, name, context, moduleName, expected) {
  it(name, () => {
    const result = resolver.sync(context, moduleName);
    expect(result.path).toBe(expected);
  });
}

describe("resolve", () => {
  const resolver = new ResolverFactory({
    extensions: [".js", ".json", ".node"]
  });

  const contextResolver = new ResolverFactory({
    extensions: [".js", ".json", ".node"],
    resolveToContext: true
  });

  testResolve(
    resolver,
    "absolute path",
    fixtures,
    path.join(fixtures, "main1.js"),
    path.join(fixtures, "main1.js")
  );

  testResolve(
    resolver,
    "file with .js",
    fixtures,
    "./main1.js",
    path.join(fixtures, "main1.js")
  );

  testResolve(
    resolver,
    "file without extension",
    fixtures,
    "./main1",
    path.join(fixtures, "main1.js")
  );

  testResolve(
    resolver,
    "another file with .js",
    fixtures,
    "./a.js",
    path.join(fixtures, "a.js")
  );

  testResolve(
    resolver,
    "another file without extension",
    fixtures,
    "./a",
    path.join(fixtures, "a.js")
  );

  testResolve(
    resolver,
    "file in module with .js",
    fixtures,
    "m1/a.js",
    path.join(fixtures, "node_modules", "m1", "a.js")
  );

  testResolve(
    resolver,
    "file in module without extension",
    fixtures,
    "m1/a",
    path.join(fixtures, "node_modules", "m1", "a.js")
  );

  testResolve(
    resolver,
    "another file in module without extension",
    fixtures,
    "complexm/step1",
    path.join(fixtures, "node_modules", "complexm", "step1.js")
  );

  testResolve(
    resolver,
    "from submodule to file in sibling module",
    path.join(fixtures, "node_modules", "complexm"),
    "m2/b.js",
    path.join(fixtures, "node_modules", "m2", "b.js")
  );

  testResolve(
    resolver,
    "from submodule to file in sibling of parent module",
    path.join(fixtures, "node_modules", "complexm", "web_modules", "m1"),
    "m2/b.js",
    path.join(fixtures, "node_modules", "m2", "b.js")
  );

  testResolve(
    resolver,
    "from nested directory to overwritten file in module",
    path.join(fixtures, "multiple_modules"),
    "m1/a.js",
    path.join(fixtures, "multiple_modules", "node_modules", "m1", "a.js")
  );

  testResolve(
    resolver,
    "from nested directory to not overwritten file in module",
    path.join(fixtures, "multiple_modules"),
    "m1/b.js",
    path.join(fixtures, "node_modules", "m1", "b.js")
  );

  // query and fragment tests
  testResolve(
    resolver,
    "file with query",
    fixtures,
    "./main1.js?query",
    `${path.join(fixtures, "main1.js")}?query`
  );

  testResolve(
    resolver,
    "file with fragment",
    fixtures,
    "./main1.js#fragment",
    `${path.join(fixtures, "main1.js")}#fragment`
  );

  testResolve(
    resolver,
    "file with fragment and query",
    fixtures,
    "./main1.js#fragment?query",
    `${path.join(fixtures, "main1.js")}#fragment?query`
  );

  testResolve(
    resolver,
    "file with query and fragment",
    fixtures,
    "./main1.js?#fragment",
    `${path.join(fixtures, "main1.js")}?#fragment`
  );

  testResolve(
    resolver,
    "file in module with query",
    fixtures,
    "m1/a?query",
    `${path.join(fixtures, "node_modules", "m1", "a.js")}?query`
  );

  testResolve(
    resolver,
    "file in module with fragment",
    fixtures,
    "m1/a#fragment",
    `${path.join(fixtures, "node_modules", "m1", "a.js")}#fragment`
  );

  testResolve(
    resolver,
    "file in module with fragment and query",
    fixtures,
    "m1/a#fragment?query",
    `${path.join(fixtures, "node_modules", "m1", "a.js")}#fragment?query`
  );

  testResolve(
    resolver,
    "file in module with query and fragment",
    fixtures,
    "m1/a?#fragment",
    `${path.join(fixtures, "node_modules", "m1", "a.js")}?#fragment`
  );

  // resolveToContext tests
  it("context for fixtures", () => {
    const result = contextResolver.sync(fixtures, "./");
    expect(result.path).toBe(fixtures);
  });

  it("context for fixtures/lib", () => {
    const result = contextResolver.sync(fixtures, "./lib");
    expect(result.path).toBe(path.join(fixtures, "lib"));
  });

  it("context for fixtures with ..", () => {
    const result = contextResolver.sync(
      fixtures,
      "./lib/../../fixtures/./lib/.."
    );
    expect(result.path).toBe(fixtures);
  });

  it("context for fixtures with query", () => {
    const result = contextResolver.sync(fixtures, "./?query");
    expect(result.path).toBe(`${fixtures}?query`);
  });

  // differ between directory and file
  testResolve(
    resolver,
    "differ between directory and file, resolve file",
    fixtures,
    "./dirOrFile",
    path.join(fixtures, "dirOrFile.js")
  );

  testResolve(
    resolver,
    "differ between directory and file, resolve directory",
    fixtures,
    "./dirOrFile/",
    path.join(fixtures, "dirOrFile", "index.js")
  );

  testResolve(
    resolver,
    "find node_modules outside of node_modules",
    path.join(fixtures, "browser-module", "node_modules"),
    "m1/a",
    path.join(fixtures, "node_modules", "m1", "a.js")
  );

  testResolve(
    resolver,
    "don't crash on main field pointing to self",
    fixtures,
    "./main-field-self",
    path.join(fixtures, "main-field-self", "index.js")
  );

  testResolve(
    resolver,
    "don't crash on main field pointing to self #2",
    fixtures,
    "./main-field-self2",
    path.join(fixtures, "main-field-self2", "index.js")
  );

  // issue-238
  it("should correctly resolve (issue-238)", () => {
    const issue238 = path.resolve(fixtures, "issue-238");
    const issue238Resolver = new ResolverFactory({
      extensions: [".js", ".jsx", ".ts", ".tsx"],
      modules: ["src/a", "src/b", "src/common", "node_modules"]
    });
    const result = issue238Resolver.sync(
      path.resolve(issue238, "./src/common"),
      "config/myObjectFile"
    );
    expect(result.path).toBe(
      path.resolve(issue238, "./src/common/config/myObjectFile.js")
    );
  });

  // preferRelative
  it("should correctly resolve with preferRelative", () => {
    const preferRelativeResolver = new ResolverFactory({
      preferRelative: true
    });
    const result = preferRelativeResolver.sync(fixtures, "main1.js");
    expect(result.path).toBe(path.join(fixtures, "main1.js"));
  });

  it("should correctly resolve with preferRelative #2", () => {
    const preferRelativeResolver = new ResolverFactory({
      preferRelative: true
    });
    const result = preferRelativeResolver.sync(fixtures, "m1/a.js");
    expect(result.path).toBe(path.join(fixtures, "node_modules", "m1", "a.js"));
  });
});
