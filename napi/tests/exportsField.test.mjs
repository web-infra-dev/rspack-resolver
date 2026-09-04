import { describe, it, expect } from "rstack/test";
import { ResolverFactory } from "../index.js";
import * as path from "node:path";

const fixtureDir = path.resolve("fixtures/enhanced_resolve/test/fixtures");

const fixture = path.resolve(fixtureDir, "exports-field");
const fixture2 = path.resolve(fixtureDir, "exports-field2");
const fixture3 = path.resolve(fixtureDir, "exports-field3");
const fixture4 = path.resolve(fixtureDir, "exports-field-error");
const fixture5 = path.resolve(
  fixtureDir,
  "exports-field-invalid-package-target"
);
const fixture6 = path.resolve(fixtureDir, "exports-field-nested-version");

describe("exportsFieldPlugin", () => {
  const resolver = new ResolverFactory({
    extensions: [".js"],
    fullySpecified: true,
    conditionNames: ["webpack"]
  });

  const commonjsResolver = new ResolverFactory({
    extensions: [".js"],
    conditionNames: ["webpack"]
  });

  it("resolve root using exports field, not a main field", () => {
    const result = resolver.sync(fixture, "exports-field");
    expect(result.path).toBe(
      path.resolve(fixture, "node_modules/exports-field/x.js")
    );
  });

  // rspack fixture uses webpack condition with ["./lib/lib2/", "./lib/"] (lib2 first)
  it("resolve using exports field, not a browser field #1", () => {
    const r = new ResolverFactory({
      aliasFields: ["browser"],
      conditionNames: ["webpack"],
      extensions: [".js"]
    });
    const result = r.sync(fixture, "exports-field/dist/main.js");
    expect(result.path).toBe(
      path.resolve(fixture, "node_modules/exports-field/lib/lib2/main.js")
    );
  });

  it("resolve using exports field and a browser alias field #2", () => {
    const r = new ResolverFactory({
      aliasFields: ["browser"],
      conditionNames: ["node"],
      extensions: [".js"]
    });
    const result = r.sync(fixture2, "exports-field/dist/main.js");
    expect(result.path).toBe(
      path.resolve(fixture2, "node_modules/exports-field/lib/browser.js")
    );
  });

  it("throw error if extension not provided", () => {
    const result = resolver.sync(fixture2, "exports-field/dist/main");
    expect(result.error).toBeTruthy();
  });

  it("should resolve extension without fullySpecified", () => {
    const result = commonjsResolver.sync(fixture2, "exports-field/dist/main");
    expect(result.path).toBe(
      path.resolve(fixture2, "node_modules/exports-field/lib/lib2/main.js")
    );
  });

  it("resolver should respect condition names", () => {
    const result = resolver.sync(fixture, "exports-field/dist/main.js");
    expect(result.path).toBe(
      path.resolve(fixture, "node_modules/exports-field/lib/lib2/main.js")
    );
  });

  // skip: array fallback in exports field directory mappings
  it.skip("resolver should respect fallback", () => {
    const result = resolver.sync(fixture2, "exports-field/dist/browser.js");
    expect(result.path).toBe(
      path.resolve(fixture2, "node_modules/exports-field/lib/browser.js")
    );
  });

  // skip: array fallback in exports field directory mappings
  it.skip("resolver should respect query parameters #1", () => {
    const result = resolver.sync(fixture2, "exports-field/dist/browser.js?foo");
    expect(result.path).toBe(
      path.resolve(fixture2, "node_modules/exports-field/lib/browser.js?foo")
    );
  });

  it("resolver should respect query parameters #2. Direct matching", () => {
    const result = resolver.sync(fixture2, "exports-field?foo");
    expect(result.error).toBeTruthy();
  });

  // skip: array fallback in exports field directory mappings
  it.skip("resolver should respect fragment parameters #1", () => {
    const result = resolver.sync(fixture2, "exports-field/dist/browser.js#foo");
    expect(result.path).toBe(
      path.resolve(fixture2, "node_modules/exports-field/lib/browser.js#foo")
    );
  });

  it("resolver should respect fragment parameters #2. Direct matching", () => {
    const result = resolver.sync(fixture2, "exports-field#foo");
    expect(result.error).toBeTruthy();
  });

  it("relative path should work, if relative path as request is used", () => {
    const result = resolver.sync(
      fixture,
      "./node_modules/exports-field/lib/main.js"
    );
    expect(result.path).toBe(
      path.resolve(fixture, "node_modules/exports-field/lib/main.js")
    );
  });

  it("relative path should not work with exports field", () => {
    const result = resolver.sync(
      fixture,
      "./node_modules/exports-field/dist/main.js"
    );
    expect(result.error).toBeTruthy();
  });

  it("backtracking should not work for request", () => {
    const result = resolver.sync(fixture, "exports-field/dist/../../../a.js");
    expect(result.error).toBeTruthy();
  });

  it("backtracking should not work for exports field target", () => {
    const result = resolver.sync(fixture, "exports-field/dist/a.js");
    expect(result.error).toBeTruthy();
  });

  it("self-resolving root", () => {
    const result = resolver.sync(fixture, "@exports-field/core");
    expect(result.path).toBe(path.resolve(fixture, "./a.js"));
  });

  it("not exported error", () => {
    const result = resolver.sync(fixture, "exports-field/anything/else");
    expect(result.error).toBeTruthy();
  });

  it("field name path #1", () => {
    const r = new ResolverFactory({
      aliasFields: ["browser"],
      exportsFields: [["exportsField", "exports"]],
      extensions: [".js"]
    });
    const result = r.sync(fixture3, "exports-field");
    expect(result.path).toBe(
      path.resolve(fixture3, "node_modules/exports-field/main.js")
    );
  });

  it("field name path #2", () => {
    const r = new ResolverFactory({
      aliasFields: ["browser"],
      exportsFields: [["exportsField", "exports"], "exports"],
      extensions: [".js"]
    });
    const result = r.sync(fixture3, "exports-field");
    expect(result.path).toBe(
      path.resolve(fixture3, "node_modules/exports-field/main.js")
    );
  });

  it("field name path #3", () => {
    const r = new ResolverFactory({
      aliasFields: ["browser"],
      exportsFields: ["exports", ["exportsField", "exports"]],
      extensions: [".js"]
    });
    const result = r.sync(fixture3, "exports-field");
    expect(result.path).toBe(
      path.resolve(fixture3, "node_modules/exports-field/main.js")
    );
  });

  it("field name path #4", () => {
    const r = new ResolverFactory({
      aliasFields: ["browser"],
      exportsFields: [["exports"]],
      extensions: [".js"]
    });
    const result = r.sync(fixture2, "exports-field");
    expect(result.path).toBe(
      path.resolve(fixture2, "node_modules/exports-field/index.js")
    );
  });

  it("field name path #5", () => {
    const r = new ResolverFactory({
      aliasFields: ["browser"],
      exportsFields: ["ex", ["exportsField", "exports"]],
      extensions: [".js"]
    });
    const result = r.sync(fixture3, "exports-field");
    expect(result.path).toBe(
      path.resolve(fixture3, "node_modules/exports-field/index")
    );
  });

  it("request ending with slash #1", () => {
    const result = resolver.sync(fixture, "exports-field/");
    expect(result.error).toBeTruthy();
  });

  it("request ending with slash #2", () => {
    const result = resolver.sync(fixture, "exports-field/dist/");
    expect(result.error).toBeTruthy();
  });

  it("request ending with slash #3", () => {
    const result = resolver.sync(fixture, "exports-field/lib/");
    expect(result.error).toBeTruthy();
  });

  it("should throw error if target is invalid", () => {
    const result = resolver.sync(fixture4, "exports-field");
    expect(result.error).toBeTruthy();
  });

  it("throw error if exports field is invalid", () => {
    const result = resolver.sync(fixture, "invalid-exports-field");
    expect(result.error).toBeTruthy();
  });

  // Wildcard pattern tests
  it("should resolve with wildcard pattern #1", () => {
    const wcFixture = path.resolve(fixtureDir, "imports-exports-wildcard");
    const result = resolver.sync(wcFixture, "m/features/f.js");
    expect(result.path).toBe(
      path.resolve(wcFixture, "./node_modules/m/src/features/f.js")
    );
  });

  it("should resolve with wildcard pattern #2", () => {
    const wcFixture = path.resolve(fixtureDir, "imports-exports-wildcard");
    const result = resolver.sync(wcFixture, "m/features/y/y.js");
    expect(result.path).toBe(
      path.resolve(wcFixture, "./node_modules/m/src/features/y/y.js")
    );
  });

  it("should resolve with wildcard pattern #4", () => {
    const wcFixture = path.resolve(fixtureDir, "imports-exports-wildcard");
    const result = resolver.sync(wcFixture, "m/features-no-ext/y/y.js");
    expect(result.path).toBe(
      path.resolve(wcFixture, "./node_modules/m/src/features/y/y.js")
    );
  });

  it("should resolve with wildcard pattern #5", () => {
    const wcFixture = path.resolve(fixtureDir, "imports-exports-wildcard");
    const result = resolver.sync(wcFixture, "m/middle/nested/f.js");
    expect(result.path).toBe(
      path.resolve(wcFixture, "./node_modules/m/src/middle/nested/f.js")
    );
  });

  it("should resolve with wildcard pattern #6", () => {
    const wcFixture = path.resolve(fixtureDir, "imports-exports-wildcard");
    const result = resolver.sync(wcFixture, "m/middle-1/nested/f.js");
    expect(result.path).toBe(
      path.resolve(wcFixture, "./node_modules/m/src/middle-1/nested/f.js")
    );
  });

  it("should resolve with wildcard pattern #7", () => {
    const wcFixture = path.resolve(fixtureDir, "imports-exports-wildcard");
    const result = resolver.sync(wcFixture, "m/middle-2/nested/f.js");
    expect(result.path).toBe(
      path.resolve(wcFixture, "./node_modules/m/src/middle-2/nested/f.js")
    );
  });

  it("should resolve with wildcard pattern #8", () => {
    const wcFixture = path.resolve(fixtureDir, "imports-exports-wildcard");
    const result = resolver.sync(wcFixture, "m/middle-3/nested/f");
    expect(result.path).toBe(
      path.resolve(
        wcFixture,
        "./node_modules/m/src/middle-3/nested/f/nested/f.js"
      )
    );
  });

  it("should resolve with wildcard pattern #9", () => {
    const wcFixture = path.resolve(fixtureDir, "imports-exports-wildcard");
    const result = resolver.sync(wcFixture, "m/middle-4/f/nested");
    expect(result.path).toBe(
      path.resolve(wcFixture, "./node_modules/m/src/middle-4/f/f.js")
    );
  });

  it("should resolve with wildcard pattern #10", () => {
    const wcFixture = path.resolve(fixtureDir, "imports-exports-wildcard");
    const result = resolver.sync(wcFixture, "m/middle-5/f$/$");
    expect(result.path).toBe(
      path.resolve(wcFixture, "./node_modules/m/src/middle-5/f$/$.js")
    );
  });

  it("should throw error if target is 'null'", () => {
    const wcFixture = path.resolve(fixtureDir, "imports-exports-wildcard");
    const result = resolver.sync(wcFixture, "m/features/internal/file.js");
    expect(result.error).toBeTruthy();
  });

  // extensionAlias with exports field
  it("should resolve with the extensionAlias option", () => {
    const r = new ResolverFactory({
      extensions: [".js"],
      extensionAlias: { ".js": [".ts", ".js"] },
      fullySpecified: true,
      conditionNames: ["webpack", "default"]
    });
    const eaFixture = path.resolve(
      fixtureDir,
      "exports-field-and-extension-alias"
    );
    const result = r.sync(eaFixture, "@org/pkg/string.js");
    expect(result.path).toBe(
      path.resolve(eaFixture, "./node_modules/@org/pkg/dist/string.js")
    );
  });

  it("should resolve with the extensionAlias option #2", () => {
    const r = new ResolverFactory({
      extensions: [".js"],
      extensionAlias: { ".js": [".ts", ".js"] },
      fullySpecified: true,
      conditionNames: ["webpack", "default"]
    });
    const eaFixture = path.resolve(
      fixtureDir,
      "exports-field-and-extension-alias"
    );
    const result = r.sync(eaFixture, "pkg/string.js");
    expect(result.path).toBe(
      path.resolve(eaFixture, "./node_modules/pkg/dist/string.js")
    );
  });

  it("should resolve with the extensionAlias option #3", () => {
    const r = new ResolverFactory({
      extensions: [".js"],
      extensionAlias: { ".js": [".foo", ".baz", ".baz", ".ts", ".js"] },
      fullySpecified: true,
      conditionNames: ["webpack", "default"]
    });
    const eaFixture = path.resolve(
      fixtureDir,
      "exports-field-and-extension-alias"
    );
    const result = r.sync(eaFixture, "pkg/string.js");
    expect(result.path).toBe(
      path.resolve(eaFixture, "./node_modules/pkg/dist/string.js")
    );
  });

  it("should throw error with the extensionAlias option", () => {
    const r = new ResolverFactory({
      extensions: [".js"],
      extensionAlias: { ".js": [".ts"] },
      fullySpecified: true,
      conditionNames: ["webpack", "default"]
    });
    const eaFixture = path.resolve(
      fixtureDir,
      "exports-field-and-extension-alias"
    );
    const result = r.sync(eaFixture, "pkg/string.js");
    expect(result.error).toBeTruthy();
  });

  it("should throw error with the extensionAlias option #2", () => {
    const r = new ResolverFactory({
      extensions: [".js"],
      extensionAlias: { ".js": [".ts"] },
      fullySpecified: true,
      conditionNames: ["webpack", "default"]
    });
    const eaFixture = path.resolve(
      fixtureDir,
      "exports-field-and-extension-alias"
    );
    const result = r.sync(eaFixture, "pkg/string.js");
    expect(result.error).toBeTruthy();
  });

  // skip: query strings containing ../ treated as invalid targets
  it.skip("invalid package target #1", () => {
    const result = resolver.sync(fixture5, "@exports-field/bad-specifier");
    expect(result.path).toBe(`${path.resolve(fixture5, "./a.js")}?foo=../`);
  });

  // skip: query strings containing ../ treated as invalid targets
  it.skip("invalid package target #2", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/foo/file.js"
    );
    expect(result.path).toBe(`${path.resolve(fixture5, "./a.js")}?foo=../#../`);
  });

  it("invalid package target #3", () => {
    const result = resolver.sync(fixture5, "@exports-field/bad-specifier/bar");
    expect(result.error).toBeTruthy();
  });

  it("invalid package target #4", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/baz-multi"
    );
    expect(result.error).toBeTruthy();
  });

  it("invalid package target #5", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/pattern/a.js"
    );
    expect(result.path).toBe(path.resolve(fixture5, "./a.js"));
  });

  it("invalid package target #6", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/slash"
    );
    expect(result.path).toBe(path.resolve(fixture5, "./a.js"));
  });

  it("invalid package target #7", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/no-slash"
    );
    expect(result.path).toBe(path.resolve(fixture5, "./a.js"));
  });

  it("invalid package target #8", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/utils/index.mjs"
    );
    expect(result.error).toBeTruthy();
  });

  it("invalid package target #9", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/utils1/index.mjs"
    );
    expect(result.error).toBeTruthy();
  });

  it("invalid package target #10", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/utils2/index"
    );
    expect(result.error).toBeTruthy();
  });

  it("invalid package target #11", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/utils3/index"
    );
    expect(result.error).toBeTruthy();
  });

  it("invalid package target #12", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/utils4/index"
    );
    expect(result.error).toBeTruthy();
  });

  it("invalid package target #13", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/utils5/index"
    );
    expect(result.error).toBeTruthy();
  });

  it("invalid package target #14", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/timezones/pdt.mjs"
    );
    expect(result.error).toBeTruthy();
  });

  // skip: array fallback in exports field when first valid target file not found
  it.skip("invalid package target #15", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/non-existent.js"
    );
    expect(result.error).toBeTruthy();
  });

  it("invalid package target #16", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/dep/multi1"
    );
    expect(result.error).toBeTruthy();
  });

  it("invalid package target #17", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/dep/multi2"
    );
    expect(result.error).toBeTruthy();
  });

  it("invalid package target #18", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/dep/multi4"
    );
    expect(result.error).toBeTruthy();
  });

  it("invalid package target #19", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/dep/multi5"
    );
    expect(result.error).toBeTruthy();
  });

  it("should resolve the valid thing in array of export #1", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/bad-specifier.js"
    );
    expect(result.path).toBe(path.resolve(fixture5, "./a.js"));
  });

  it("should resolve the valid thing in array of export #2", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/bad-specifier1.js"
    );
    expect(result.path).toBe(path.resolve(fixture5, "./a.js"));
  });

  it("should resolve the valid thing in array of export #3", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/dep/multi"
    );
    expect(result.path).toBe(path.resolve(fixture5, "./a.js"));
  });

  it("should resolve the valid thing in array of export #4", () => {
    const result = resolver.sync(
      fixture5,
      "@exports-field/bad-specifier/dep/multi3"
    );
    expect(result.path).toBe(path.resolve(fixture5, "./a.js"));
  });

  it("should not fall back to parent node_modules when exports field maps to a missing file (issue #399)", () => {
    const r = new ResolverFactory({
      extensions: [".js"],
      conditionNames: ["node"],
      fullySpecified: true
    });
    const result = r.sync(
      path.resolve(fixture6, "workspace"),
      "pkg/src/index.js"
    );
    expect(result.error).toBeTruthy();
  });
});
