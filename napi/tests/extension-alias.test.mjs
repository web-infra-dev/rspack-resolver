import { describe, it, expect } from "rstack/test";
import { ResolverFactory } from "../index.js";
import * as path from "node:path";

const fixtureDir = path.resolve("fixtures/enhanced_resolve/test/fixtures");
const fixture = path.resolve(fixtureDir, "extension-alias");

describe("extension-alias", () => {
  const resolver = new ResolverFactory({
    extensions: [".js"],
    mainFiles: ["index.js"],
    extensionAlias: {
      ".js": [".ts", ".js"],
      ".mjs": [".mts"]
    }
  });

  it("should alias fully specified file", () => {
    const result = resolver.sync(fixture, "./index.js");
    expect(result.path).toBe(path.resolve(fixture, "index.ts"));
  });

  it("should alias fully specified file when there are two alternatives", () => {
    const result = resolver.sync(fixture, "./dir/index.js");
    expect(result.path).toBe(path.resolve(fixture, "dir", "index.ts"));
  });

  it("should also allow the second alternative", () => {
    const result = resolver.sync(fixture, "./dir2/index.js");
    expect(result.path).toBe(path.resolve(fixture, "dir2", "index.js"));
  });

  it("should support alias option without an array", () => {
    const result = resolver.sync(fixture, "./dir2/index.mjs");
    expect(result.path).toBe(path.resolve(fixture, "dir2", "index.mts"));
  });

  it("should not allow to fallback to the original extension or add extensions", () => {
    const result = resolver.sync(fixture, "./index.mjs");
    expect(result.error).toBeTruthy();
  });

  describe("should not apply extension alias to extensions or mainFiles field", () => {
    const resolver2 = new ResolverFactory({
      extensions: [".js"],
      mainFiles: ["index.js"],
      extensionAlias: {
        ".js": []
      }
    });

    it("directory", () => {
      const result = resolver2.sync(fixture, "./dir2");
      expect(result.path).toBe(path.resolve(fixture, "dir2", "index.js"));
    });

    it("file", () => {
      const result = resolver2.sync(fixture, "./dir2/index");
      expect(result.path).toBe(path.resolve(fixture, "dir2", "index.js"));
    });
  });
});
