import { describe, it, expect } from "rstack/test";
import { ResolverFactory } from "../index.js";
import * as path from "node:path";

const fixtureDir = path.resolve("fixtures/enhanced_resolve/test/fixtures");

const fixture = path.resolve(fixtureDir, "imports-field");
const fixture1 = path.resolve(fixtureDir, "imports-field-different");

describe("importsFieldPlugin", () => {
  const resolver = new ResolverFactory({
    extensions: [".js"],
    mainFiles: ["index.js"],
    conditionNames: ["webpack"]
  });

  it("should resolve using imports field instead of self-referencing", () => {
    const result = resolver.sync(fixture, "#imports-field");
    expect(result.path).toBe(path.resolve(fixture, "b.js"));
  });

  it("should resolve using imports field instead of self-referencing for a subpath", () => {
    const result = resolver.sync(
      path.resolve(fixture, "dir"),
      "#imports-field"
    );
    expect(result.path).toBe(path.resolve(fixture, "b.js"));
  });

  it("should disallow resolve out of package scope", () => {
    const result = resolver.sync(fixture, "#b");
    expect(result.error).toBeTruthy();
  });

  it("field name #1", () => {
    const r = new ResolverFactory({
      extensions: [".js"],
      mainFiles: ["index.js"],
      importsFields: [["imports"]],
      conditionNames: ["webpack"]
    });
    const result = r.sync(fixture, "#imports-field");
    expect(result.path).toBe(path.resolve(fixture, "b.js"));
  });

  it("field name #2", () => {
    const r = new ResolverFactory({
      extensions: [".js"],
      mainFiles: ["index.js"],
      importsFields: [["other", "imports"], "imports"],
      conditionNames: ["webpack"]
    });
    const result = r.sync(fixture, "#b");
    expect(result.path).toBe(path.resolve(fixture, "a.js"));
  });

  it("should resolve package #1", () => {
    const result = resolver.sync(fixture, "#a/dist/main.js");
    expect(result.path).toBe(
      path.resolve(fixture, "node_modules/a/lib/lib2/main.js")
    );
  });

  it("should resolve package #2", () => {
    const result = resolver.sync(fixture, "#a");
    expect(result.error).toBeTruthy();
  });

  it("should resolve package #3", () => {
    const result = resolver.sync(fixture, "#ccc/index.js");
    expect(result.path).toBe(path.resolve(fixture, "node_modules/c/index.js"));
  });

  it("should resolve package #4", () => {
    const result = resolver.sync(fixture, "#c");
    expect(result.path).toBe(path.resolve(fixture, "node_modules/c/index.js"));
  });

  it("should resolve with wildcard pattern", () => {
    const wcFixture = path.resolve(
      fixtureDir,
      "imports-exports-wildcard/node_modules/m"
    );
    const result = resolver.sync(wcFixture, "#internal/i.js");
    expect(result.path).toBe(path.resolve(wcFixture, "./src/internal/i.js"));
  });

  // skip: #/ slash pattern (node.js PR #60864) not yet supported
  it.skip("should work and throw an error on invalid imports #1", () => {
    const result = resolver.sync(fixture, "#/dep");
    expect(result.error).toBeTruthy();
  });

  it("should work and throw an error on invalid imports #2", () => {
    const result = resolver.sync(fixture, "#dep/");
    expect(result.error).toBeTruthy();
  });

  // skip: query strings containing ../ treated as invalid targets
  it.skip("should work with invalid imports #1", () => {
    const result = resolver.sync(fixture1, "#dep");
    expect(result.path).toBe(`${path.resolve(fixture1, "./a.js")}?foo=../`);
  });

  // skip: query strings containing ../ treated as invalid targets
  it.skip("should work with invalid imports #2", () => {
    const result = resolver.sync(fixture1, "#dep/foo/a.js");
    expect(result.path).toBe(`${path.resolve(fixture1, "./a.js")}?foo=../#../`);
  });

  it("should work with invalid imports #3", () => {
    const result = resolver.sync(fixture1, "#dep/bar");
    expect(result.error).toBeTruthy();
  });

  it("should work with invalid imports #4", () => {
    const result = resolver.sync(fixture1, "#dep/baz");
    expect(result.error).toBeTruthy();
  });

  it("should work with invalid imports #5", () => {
    const result = resolver.sync(fixture1, "#dep/baz-multi");
    expect(result.error).toBeTruthy();
  });

  // skip: invalid specifier array handling differences
  it.skip("should work with invalid imports #7", () => {
    const result = resolver.sync(fixture1, "#dep/pattern/a.js");
    expect(result.error).toBeTruthy();
  });

  // skip: invalid specifier array handling differences
  it.skip("should work with invalid imports #8", () => {
    const result = resolver.sync(fixture1, "#dep/array");
    expect(result.path).toBe(path.resolve(fixture1, "./a.js"));
  });

  // skip: invalid specifier array handling differences
  it.skip("should work with invalid imports #9", () => {
    const result = resolver.sync(fixture1, "#dep/array2");
    expect(result.error).toBeTruthy();
  });

  it("should work with invalid imports #10", () => {
    const result = resolver.sync(fixture1, "#dep/array3");
    expect(result.path).toBe(path.resolve(fixture1, "./a.js"));
  });

  it("should work with invalid imports #11", () => {
    const result = resolver.sync(fixture1, "#dep/empty");
    expect(result.error).toBeTruthy();
  });

  it("should work with invalid imports #12", () => {
    const result = resolver.sync(fixture1, "#dep/with-bad");
    expect(result.path).toBe(path.resolve(fixture1, "./a.js"));
  });

  it("should work with invalid imports #13", () => {
    const result = resolver.sync(fixture1, "#dep/with-bad2");
    expect(result.path).toBe(path.resolve(fixture1, "./a.js"));
  });

  it("should work with invalid imports #14", () => {
    const result = resolver.sync(fixture1, "#timezones/pdt.mjs");
    expect(result.error).toBeTruthy();
  });

  it("should work with invalid imports #15", () => {
    const result = resolver.sync(fixture1, "#dep/multi1");
    expect(result.error).toBeTruthy();
  });

  it("should work with invalid imports #16", () => {
    const result = resolver.sync(fixture1, "#dep/multi2");
    expect(result.error).toBeTruthy();
  });

  it("should work and resolve with array imports", () => {
    const result = resolver.sync(fixture1, "#dep/multi");
    expect(result.path).toBe(path.resolve(fixture1, "./a.js"));
  });
});
