import { describe, it, expect } from "rstack/test";
import { ResolverFactory } from "../index.js";
import * as path from "node:path";

const fixtureDir = path.resolve("fixtures/enhanced_resolve/test/fixtures");

describe("option", () => {
  describe("alias", () => {
    it("should allow alias string", () => {
      const resolver = new ResolverFactory({
        alias: { strAlias: path.join(fixtureDir, "alias/files/a.js") }
      });
      expect(resolver.sync(fixtureDir, "strAlias").path).toBe(
        path.join(fixtureDir, "alias/files/a.js")
      );
    });

    it("should allow alias null", () => {
      const resolver = new ResolverFactory({
        alias: { strAlias: false }
      });
      expect(resolver.sync(fixtureDir, "strAlias").error).toMatch(
        /^Path is ignored/
      );
    });

    it("should allow alias string array", () => {
      const resolver = new ResolverFactory({
        alias: { strAlias: [path.join(fixtureDir, "alias/files/a.js")] }
      });
      expect(resolver.sync(fixtureDir, "strAlias").path).toBe(
        path.join(fixtureDir, "alias/files/a.js")
      );
    });
  });

  describe("aliasFields", () => {
    it("should allow field string ", () => {
      const resolver = new ResolverFactory({ aliasFields: ["browser"] });
      expect(
        resolver.sync(fixtureDir, "./browser-module/lib/replaced.js").path
      ).toBe(path.join(fixtureDir, "./browser-module/lib/browser.js"));
    });
    it("should allow json path array", () => {
      const resolver = new ResolverFactory({
        aliasFields: [["innerBrowser1", "field", "browser"]]
      });

      expect(
        resolver.sync(fixtureDir, "./browser-module/lib/main1.js").path
      ).toBe(path.join(fixtureDir, "./browser-module/lib/main.js"));
    });
  });

  describe("exportsFields", () => {
    const createTest = exportsFields => () => {
      const resolver = new ResolverFactory({ exportsFields });

      expect(
        resolver.sync(
          path.resolve(fixtureDir, "./exports-field3"),
          "exports-field"
        ).path
      ).toBe(
        path.join(
          fixtureDir,
          "exports-field3/node_modules/exports-field/src/index.js"
        )
      );
    };
    it("should allow string as field item", createTest(["broken"]));
    it("should allow json path array as field item", createTest([["broken"]]));
  });

  describe("mainFields", () => {
    const createTest = mainFields => {
      const resolver = new ResolverFactory({ mainFields });
      expect(resolver.sync(fixtureDir, "../..").path).toBe(
        path.join(fixtureDir, "../../", "lib/index.js")
      );
    };
    it("should use `'main'` as default", createTest(undefined));
    it("should allow field string", createTest("main"));
    it("should allow field array", createTest(["main"]));
  });
});
