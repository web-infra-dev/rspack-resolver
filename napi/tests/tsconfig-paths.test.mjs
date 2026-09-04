import { describe, it, expect } from "rstack/test";
import { ResolverFactory } from "../index.js";
import * as path from "node:path";

const fixtureDir = path.resolve("fixtures/enhanced_resolve/test/fixtures");

const baseExampleDir = path.resolve(fixtureDir, "tsconfig-paths", "base");
const extendsExampleDir = path.resolve(
  fixtureDir,
  "tsconfig-paths",
  "extends-base"
);
const extendsNpmDir = path.resolve(fixtureDir, "tsconfig-paths", "extends-npm");
const extendsCircularDir = path.resolve(
  fixtureDir,
  "tsconfig-paths",
  "extends-circular"
);
const referencesProjectDir = path.resolve(
  fixtureDir,
  "tsconfig-paths",
  "references-project"
);

function makeTsconfigResolver(tsconfigPath, extra) {
  return new ResolverFactory({
    extensions: [".ts", ".tsx"],
    mainFields: ["browser", "main"],
    mainFiles: ["index"],
    tsconfig: { configFile: tsconfigPath },
    ...extra
  });
}

describe("TsconfigPathsPlugin", () => {
  it("resolves exact mapped path '@components/*' via tsconfig option", () => {
    const resolver = makeTsconfigResolver(
      path.join(baseExampleDir, "tsconfig.json")
    );
    const result = resolver.sync(baseExampleDir, "@components/button");
    expect(result.path).toBe(
      path.join(baseExampleDir, "src", "components", "button.ts")
    );
  });

  it("when multiple patterns match, the pattern with the longest matching prefix is used", () => {
    const resolver = makeTsconfigResolver(
      path.join(baseExampleDir, "tsconfig.json")
    );
    const result = resolver.sync(baseExampleDir, "longest/bar");
    expect(result.path).toBe(
      path.join(baseExampleDir, "src", "mapped", "longest", "three.ts")
    );
  });

  it("resolves exact mapped path 'foo' via tsconfig option", () => {
    const resolver = makeTsconfigResolver(
      path.join(baseExampleDir, "tsconfig.json")
    );
    const result = resolver.sync(baseExampleDir, "foo");
    expect(result.path).toBe(
      path.join(baseExampleDir, "src", "mapped", "foo", "index.ts")
    );
  });

  it("resolves wildcard mapped path 'bar/*' via tsconfig option", () => {
    const resolver = makeTsconfigResolver(
      path.join(baseExampleDir, "tsconfig.json")
    );
    const result = resolver.sync(baseExampleDir, "bar/file1");
    expect(result.path).toBe(
      path.join(baseExampleDir, "src", "mapped", "bar", "file1.ts")
    );
  });

  it("resolves wildcard mapped path '*/old-file' to specific file", () => {
    const resolver = makeTsconfigResolver(
      path.join(baseExampleDir, "tsconfig.json")
    );
    const result = resolver.sync(baseExampleDir, "utils/old-file");
    expect(result.path).toBe(
      path.join(baseExampleDir, "src", "components", "new-file.ts")
    );
  });

  it("falls through when no mapping exists", () => {
    const resolver = makeTsconfigResolver(
      path.join(baseExampleDir, "tsconfig.json")
    );
    const result = resolver.sync(baseExampleDir, "does-not-exist");
    expect(result.error).toBeTruthy();
  });

  // skip: ${configDir} in tsconfig extends
  it.skip("resolves '@components/*' using extends", () => {
    const resolver = makeTsconfigResolver(
      path.join(extendsExampleDir, "tsconfig.json")
    );
    const result = resolver.sync(extendsExampleDir, "@components/button");
    expect(result.path).toBe(
      path.join(extendsExampleDir, "src", "components", "button.ts")
    );
  });

  describe("Path wildcard patterns", () => {
    it("resolves 'foo/*' wildcard pattern", () => {
      const resolver = makeTsconfigResolver(
        path.join(baseExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(baseExampleDir, "foo/file1");
      expect(result.path).toBe(
        path.join(baseExampleDir, "src", "mapped", "bar", "file1.ts")
      );
    });

    it("resolves '*' catch-all pattern to src/mapped/star/*", () => {
      const resolver = makeTsconfigResolver(
        path.join(baseExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(baseExampleDir, "star-bar/index");
      expect(result.path).toBe(
        path.join(
          baseExampleDir,
          "src",
          "mapped",
          "star",
          "star-bar",
          "index.ts"
        )
      );
    });

    it("resolves package with mainFields", () => {
      const resolver = makeTsconfigResolver(
        path.join(baseExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(baseExampleDir, "main-field-package");
      expect(result.path).toBe(
        path.join(
          baseExampleDir,
          "src",
          "mapped",
          "star",
          "main-field-package",
          "node.ts"
        )
      );
    });

    it("resolves package with browser field", () => {
      const resolver = makeTsconfigResolver(
        path.join(baseExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(baseExampleDir, "browser-field-package");
      expect(result.path).toBe(
        path.join(
          baseExampleDir,
          "src",
          "mapped",
          "star",
          "browser-field-package",
          "browser.ts"
        )
      );
    });

    it("resolves package with default index.ts", () => {
      const resolver = makeTsconfigResolver(
        path.join(baseExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(baseExampleDir, "no-main-field-package");
      expect(result.path).toBe(
        path.join(
          baseExampleDir,
          "src",
          "mapped",
          "star",
          "no-main-field-package",
          "index.ts"
        )
      );
    });
  });

  it("should resolve paths when extending from npm package", () => {
    const resolver = makeTsconfigResolver(
      path.join(extendsNpmDir, "tsconfig.json")
    );
    const result = resolver.sync(extendsNpmDir, "@components/button");
    expect(result.path).toMatch(/src[\\/](utils|components)[\\/]button\.ts$/);
  });

  it("should handle malformed tsconfig.json gracefully", () => {
    const malformedExampleDir = path.resolve(
      fixtureDir,
      "tsconfig-paths",
      "malformed-json"
    );
    const resolver = makeTsconfigResolver(
      path.join(malformedExampleDir, "tsconfig.json")
    );
    const result = resolver.sync(malformedExampleDir, "@components/button");
    expect(result.error).toBeTruthy();
  });

  describe("${configDir} template variable support", () => {
    it("should substitute ${configDir} in path mappings", () => {
      const resolver = makeTsconfigResolver(
        path.join(baseExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(baseExampleDir, "@components/button");
      expect(result.path).toBe(
        path.join(baseExampleDir, "src", "components", "button.ts")
      );
    });

    it("should substitute ${configDir} in multiple path patterns", () => {
      const resolver = makeTsconfigResolver(
        path.join(baseExampleDir, "tsconfig.json")
      );
      const result1 = resolver.sync(baseExampleDir, "@utils/date");
      expect(result1.path).toBe(
        path.join(baseExampleDir, "src", "utils", "date.ts")
      );
      const result2 = resolver.sync(baseExampleDir, "foo");
      expect(result2.path).toBe(
        path.join(baseExampleDir, "src", "mapped", "foo", "index.ts")
      );
    });

    // skip: ${configDir} in tsconfig extends
    it.skip("should handle circular extends without hanging", () => {
      const aDir = path.join(extendsCircularDir, "a");
      const resolver = makeTsconfigResolver(path.join(aDir, "tsconfig.json"));
      const result = resolver.sync(aDir, "@lib/foo");
      expect(result.path).toBe(path.join(aDir, "src", "lib", "foo.ts"));
    });
  });

  it("should use baseUrl from tsconfig", () => {
    const resolver = new ResolverFactory({
      extensions: [".ts", ".tsx"],
      mainFields: ["browser", "main"],
      mainFiles: ["index"],
      tsconfig: {
        configFile: path.join(baseExampleDir, "tsconfig.json")
      }
    });
    const result = resolver.sync(baseExampleDir, "src/utils/date");
    expect(result.path).toBe(
      path.join(baseExampleDir, "src", "utils", "date.ts")
    );
  });

  describe("TypeScript Project References", () => {
    it("should support tsconfig object format with configFile", () => {
      const resolver = new ResolverFactory({
        extensions: [".ts", ".tsx"],
        mainFields: ["browser", "main"],
        mainFiles: ["index"],
        tsconfig: {
          configFile: path.join(baseExampleDir, "tsconfig.json"),
          references: "auto"
        }
      });
      const result = resolver.sync(baseExampleDir, "@components/button");
      expect(result.path).toBe(
        path.join(baseExampleDir, "src", "components", "button.ts")
      );
    });

    // skip: ${configDir} in tsconfig references
    it.skip("should resolve own paths (without cross-project references)", () => {
      const appDir = path.join(referencesProjectDir, "packages", "app");
      const resolver = new ResolverFactory({
        extensions: [".ts", ".tsx"],
        mainFields: ["browser", "main"],
        mainFiles: ["index"],
        tsconfig: {
          configFile: path.join(appDir, "tsconfig.json"),
          references: "auto"
        }
      });
      const result = resolver.sync(appDir, "@app/index");
      expect(result.path).toBe(path.join(appDir, "src", "index.ts"));
    });

    // skip: ${configDir} in tsconfig references
    it.skip("should resolve self-references within a referenced project", () => {
      const appDir = path.join(referencesProjectDir, "packages", "app");
      const sharedDir = path.join(referencesProjectDir, "packages", "shared");
      const resolver = new ResolverFactory({
        extensions: [".ts", ".tsx"],
        mainFields: ["browser", "main"],
        mainFiles: ["index"],
        tsconfig: {
          configFile: path.join(appDir, "tsconfig.json"),
          references: "auto"
        }
      });
      const result = resolver.sync(sharedDir, "@shared/helper");
      expect(result.path).toBe(
        path.join(sharedDir, "src", "utils", "helper.ts")
      );
    });

    // skip: ${configDir} in tsconfig references
    it.skip("should support explicit references array", () => {
      const appDir = path.join(referencesProjectDir, "packages", "app");
      const sharedSrcDir = path.join(
        referencesProjectDir,
        "packages",
        "shared",
        "src"
      );
      const resolver = new ResolverFactory({
        extensions: [".ts", ".tsx"],
        mainFields: ["browser", "main"],
        mainFiles: ["index"],
        tsconfig: {
          configFile: path.join(appDir, "tsconfig.json"),
          references: ["../shared"]
        }
      });
      const result = resolver.sync(sharedSrcDir, "@shared/helper");
      expect(result.path).toBe(path.join(sharedSrcDir, "utils", "helper.ts"));
    });

    // skip: ${configDir} in tsconfig references
    it.skip("should not load references when references option is omitted", () => {
      const appDir = path.join(referencesProjectDir, "packages", "app");
      const resolver = new ResolverFactory({
        extensions: [".ts", ".tsx"],
        mainFields: ["browser", "main"],
        mainFiles: ["index"],
        tsconfig: {
          configFile: path.join(appDir, "tsconfig.json")
        }
      });
      const result = resolver.sync(appDir, "@shared/utils/helper");
      expect(result.error).toBeTruthy();
    });

    // skip: ${configDir} in tsconfig references
    it.skip("should handle nested references", () => {
      const appDir = path.join(referencesProjectDir, "packages", "app");
      const utilsSrcDir = path.join(
        referencesProjectDir,
        "packages",
        "utils",
        "src"
      );
      const resolver = new ResolverFactory({
        extensions: [".ts", ".tsx"],
        mainFields: ["browser", "main"],
        mainFiles: ["index"],
        tsconfig: {
          configFile: path.join(appDir, "tsconfig.json"),
          references: "auto"
        }
      });
      const result = resolver.sync(utilsSrcDir, "@utils/date");
      expect(result.path).toBe(path.join(utilsSrcDir, "core", "date.ts"));
    });

    describe("modules resolution with references", () => {
      // skip: ${configDir} in tsconfig references
      it.skip("should resolve modules from main project's baseUrl", () => {
        const appDir = path.join(referencesProjectDir, "packages", "app");
        const resolver = new ResolverFactory({
          extensions: [".ts", ".tsx"],
          mainFields: ["browser", "main"],
          mainFiles: ["index"],
          tsconfig: {
            configFile: path.join(appDir, "tsconfig.json"),
            references: "auto"
          }
        });
        const result = resolver.sync(appDir, "src/components/Button");
        expect(result.path).toBe(
          path.join(appDir, "src", "components", "Button.ts")
        );
      });

      // skip: ${configDir} in tsconfig references
      it.skip("should resolve modules from referenced project's baseUrl", () => {
        const appDir = path.join(referencesProjectDir, "packages", "app");
        const sharedSrcDir = path.join(
          referencesProjectDir,
          "packages",
          "shared",
          "src"
        );
        const resolver = new ResolverFactory({
          extensions: [".ts", ".tsx"],
          mainFields: ["browser", "main"],
          mainFiles: ["index"],
          tsconfig: {
            configFile: path.join(appDir, "tsconfig.json"),
            references: "auto"
          }
        });
        const result = resolver.sync(sharedSrcDir, "utils/helper");
        expect(result.path).toBe(path.join(sharedSrcDir, "utils", "helper.ts"));
      });

      // skip: ${configDir} in tsconfig references
      it.skip("should resolve components from referenced project's baseUrl", () => {
        const appDir = path.join(referencesProjectDir, "packages", "app");
        const sharedSrcDir = path.join(
          referencesProjectDir,
          "packages",
          "shared",
          "src"
        );
        const resolver = new ResolverFactory({
          extensions: [".ts", ".tsx"],
          mainFields: ["browser", "main"],
          mainFiles: ["index"],
          tsconfig: {
            configFile: path.join(appDir, "tsconfig.json"),
            references: "auto"
          }
        });
        const result = resolver.sync(sharedSrcDir, "components/Input");
        expect(result.path).toBe(
          path.join(sharedSrcDir, "components", "Input.ts")
        );
      });

      // skip: ${configDir} in tsconfig references
      it.skip("should use correct baseUrl based on request context", () => {
        const appDir = path.join(referencesProjectDir, "packages", "app");
        const resolver = new ResolverFactory({
          extensions: [".ts", ".tsx"],
          mainFields: ["browser", "main"],
          mainFiles: ["index"],
          tsconfig: {
            configFile: path.join(appDir, "tsconfig.json"),
            references: "auto"
          }
        });
        const result1 = resolver.sync(appDir, "src/index");
        expect(result1.path).toBe(path.join(appDir, "src", "index.ts"));
      });

      // skip: ${configDir} in tsconfig references
      it.skip("should support explicit references with modules resolution", () => {
        const appDir = path.join(referencesProjectDir, "packages", "app");
        const sharedSrcDir = path.join(
          referencesProjectDir,
          "packages",
          "shared",
          "src"
        );
        const resolver = new ResolverFactory({
          extensions: [".ts", ".tsx"],
          mainFields: ["browser", "main"],
          mainFiles: ["index"],
          tsconfig: {
            configFile: path.join(appDir, "tsconfig.json"),
            references: ["../shared"]
          }
        });
        const result = resolver.sync(sharedSrcDir, "utils/helper");
        expect(result.path).toBe(path.join(sharedSrcDir, "utils", "helper.ts"));
      });
    });
  });

  describe("bug: baseUrl from deep extends chain", () => {
    const deepBaseUrlDir = path.resolve(
      fixtureDir,
      "tsconfig-paths",
      "extends-deep-baseurl"
    );

    it("should resolve paths whose baseUrl comes from a grandparent extends", () => {
      const appDir = path.join(deepBaseUrlDir, "packages", "app");
      const resolver = makeTsconfigResolver(path.join(appDir, "tsconfig.json"));
      const result = resolver.sync(appDir, "@base/utils/format");
      expect(result.path).toBe(
        path.join(deepBaseUrlDir, "tsconfig-base", "src", "utils", "format.ts")
      );
    });
  });

  describe("bug: scoped npm package in extends field", () => {
    const pkgEntryDir = path.resolve(
      fixtureDir,
      "tsconfig-paths",
      "extends-pkg-entry"
    );

    it("should resolve paths inherited from a scoped npm package tsconfig", () => {
      const resolver = makeTsconfigResolver(
        path.join(pkgEntryDir, "tsconfig.json")
      );
      const result = resolver.sync(pkgEntryDir, "@pkg/util");
      expect(result.path).toBe(
        path.join(
          pkgEntryDir,
          "node_modules",
          "@my-tsconfig",
          "base",
          "src",
          "util.ts"
        )
      );
    });
  });

  describe("JSONC support (comments in tsconfig.json)", () => {
    const jsoncExampleDir = path.resolve(
      fixtureDir,
      "tsconfig-paths",
      "jsonc-comments"
    );

    it("should parse tsconfig.json with line comments", () => {
      const resolver = makeTsconfigResolver(
        path.join(jsoncExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(jsoncExampleDir, "@components/button");
      expect(result.path).toBe(
        path.join(jsoncExampleDir, "src", "components", "button.ts")
      );
    });

    it("should parse tsconfig.json with block comments", () => {
      const resolver = makeTsconfigResolver(
        path.join(jsoncExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(jsoncExampleDir, "bar/index");
      expect(result.path).toBe(
        path.join(jsoncExampleDir, "src", "mapped", "bar", "index.ts")
      );
    });

    it("should parse tsconfig.json with mixed comments", () => {
      const resolver = makeTsconfigResolver(
        path.join(jsoncExampleDir, "tsconfig.json")
      );
      const result = resolver.sync(jsoncExampleDir, "foo");
      expect(result.path).toBe(
        path.join(jsoncExampleDir, "src", "mapped", "foo", "index.ts")
      );
    });
  });
});
