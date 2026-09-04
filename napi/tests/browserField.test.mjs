import { describe, it, expect } from "rstack/test";
import { ResolverFactory } from "../index.js";
import * as path from "node:path";

const fixtureDir = path.resolve("fixtures/enhanced_resolve/test/fixtures");
const browserModule = path.join(fixtureDir, "browser-module");

function p(...args) {
  return path.join(browserModule, ...args);
}

describe("browserField", () => {
  const resolver = new ResolverFactory({
    aliasFields: [
      "browser",
      ["innerBrowser1", "field2", "browser"],
      ["innerBrowser1", "field", "browser"],
      ["innerBrowser2", "browser"]
    ]
  });

  it("should ignore", () => {
    const result = resolver.sync(p(), "./lib/ignore");
    expect(result.path).toBeUndefined();
  });

  it("should ignore #2", () => {
    expect(resolver.sync(p(), "./lib/ignore.js").path).toBeUndefined();
    expect(resolver.sync(p("lib"), "./ignore").path).toBeUndefined();
    expect(resolver.sync(p("lib"), "./ignore.js").path).toBeUndefined();
  });

  it("should replace a file", () => {
    expect(resolver.sync(p(), "./lib/replaced").path).toBe(
      p("lib", "browser.js")
    );
    expect(resolver.sync(p(), "./lib/replaced.js").path).toBe(
      p("lib", "browser.js")
    );
    expect(resolver.sync(p("lib"), "./replaced").path).toBe(
      p("lib", "browser.js")
    );
    expect(resolver.sync(p("lib"), "./replaced.js").path).toBe(
      p("lib", "browser.js")
    );
  });

  it("should replace a module with a file", () => {
    expect(resolver.sync(p(), "module-a").path).toBe(
      p("browser", "module-a.js")
    );
    expect(resolver.sync(p("lib"), "module-a").path).toBe(
      p("browser", "module-a.js")
    );
  });

  it("should replace a module with a module", () => {
    expect(resolver.sync(p(), "module-b").path).toBe(
      p("node_modules", "module-c.js")
    );
    expect(resolver.sync(p("lib"), "module-b").path).toBe(
      p("node_modules", "module-c.js")
    );
  });

  it("should resolve in nested property", () => {
    expect(resolver.sync(p(), "./lib/main1.js").path).toBe(p("lib", "main.js"));
    expect(resolver.sync(p(), "./lib/main2.js").path).toBe(
      p("lib", "browser.js")
    );
  });

  it("should check only alias field properties", () => {
    expect(resolver.sync(p(), "./toString").path).toBe(p("lib", "toString.js"));
  });
});
