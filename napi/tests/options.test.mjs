import { describe, it } from "node:test";
import { ResolverFactory } from "../index.js";
import * as assert from "node:assert";
import * as path from "node:path";
import { fileURLToPath } from "url";

const fixtureDir = fileURLToPath(
  new URL("../../fixtures/enhanced_resolve/test/fixtures", import.meta.url)
);

describe("option", () => {
  describe("alias", () => {
    it("should allow alias string", () => {
      const resolver = new ResolverFactory({
        alias: { strAlias: path.join(fixtureDir, "alias/files/a.js") }
      });
      assert.equal(
        resolver.sync(fixtureDir, "strAlias").path,
        path.join(fixtureDir, "alias/files/a.js")
      );
    });

    it("should allow alias null", () => {
      const resolver = new ResolverFactory({
        alias: { strAlias: false }
      });
      assert.match(
        resolver.sync(fixtureDir, "strAlias").error,
        /^Path is ignored/
      );
    });

    it("should allow alias string array", () => {
      const resolver = new ResolverFactory({
        alias: { strAlias: [path.join(fixtureDir, "alias/files/a.js")] }
      });
      assert.equal(
        resolver.sync(fixtureDir, "strAlias").path,
        path.join(fixtureDir, "alias/files/a.js")
      );
    });
  });

  describe("aliasFields", () => {
    it("should allow field string ", () => {
      const resolver = new ResolverFactory({ aliasFields: ["browser"] });
      assert.equal(
        resolver.sync(fixtureDir, "./browser-module/lib/replaced.js").path,
        path.join(fixtureDir, "./browser-module/lib/browser.js")
      );
    });
    it("should allow json path array", () => {
      const resolver = new ResolverFactory({
        aliasFields: [["innerBrowser1", "field", "browser"]]
      });

      assert.equal(
        resolver.sync(fixtureDir, "./browser-module/lib/main1.js").path,
        path.join(fixtureDir, "./browser-module/lib/main.js")
      );
    });
  });

  describe("exportsFields", () => {
    const createTest = exportsFields => () => {
      const resolver = new ResolverFactory({ exportsFields });

      assert.equal(
        resolver.sync(
          path.resolve(fixtureDir, "./exports-field3"),
          "exports-field"
        ).path,
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
      assert.equal(
        resolver.sync(fixtureDir, "../..").path,
        path.join(fixtureDir, "../../", "lib/index.js")
      );
    };
    it("should use `'main'` as default", createTest(undefined));
    it("should allow field string", createTest("main"));
    it("should allow field array", createTest(["main"]));
  });
});
