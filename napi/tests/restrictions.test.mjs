import { describe, it, expect } from "rstack/test";
import { ResolverFactory } from "../index.js";
import * as path from "node:path";

const fixtureDir = path.resolve("fixtures/enhanced_resolve/test/fixtures");
const fixture = path.resolve(fixtureDir, "restrictions");

describe("restrictions", () => {
  it("should respect RegExp restriction", () => {
    const resolver = new ResolverFactory({
      extensions: [".js"],
      restrictions: [{ regex: "\\.(sass|scss|css)$" }]
    });
    const result = resolver.sync(fixture, "pck1");
    expect(result.error).toBeTruthy();
  });

  it("should try to find alternative #1", () => {
    const resolver = new ResolverFactory({
      extensions: [".js", ".css"],
      mainFiles: ["index"],
      restrictions: [{ regex: "\\.(sass|scss|css)$" }]
    });
    const result = resolver.sync(fixture, "pck1");
    expect(result.path).toBe(
      path.resolve(fixture, "node_modules/pck1/index.css")
    );
  });

  it("should respect string restriction", () => {
    const resolver = new ResolverFactory({
      extensions: [".js"],
      restrictions: [{ path: fixture }]
    });
    const result = resolver.sync(fixture, "pck2");
    expect(result.error).toBeTruthy();
  });

  // skip: restrictions with multiple mainFields
  it.skip("should try to find alternative #2", () => {
    const resolver = new ResolverFactory({
      extensions: [".js"],
      mainFields: ["main", "style"],
      restrictions: [{ path: fixture }, { regex: "\\.(sass|scss|css)$" }]
    });
    const result = resolver.sync(fixture, "pck2");
    expect(result.path).toBe(
      path.resolve(fixture, "node_modules/pck2/index.css")
    );
  });
});
