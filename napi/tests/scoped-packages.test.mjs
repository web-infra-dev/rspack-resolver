import { describe, it, expect } from "rstack/test";
import { ResolverFactory } from "../index.js";
import * as path from "node:path";

const fixtureDir = path.resolve("fixtures/enhanced_resolve/test/fixtures");
const fixture = path.join(fixtureDir, "scoped");

describe("scoped-packages", () => {
  const resolver = new ResolverFactory({
    aliasFields: ["browser"]
  });

  it("main field should work", () => {
    const result = resolver.sync(fixture, "@scope/pack1");
    expect(result.path).toBe(
      path.resolve(fixture, "./node_modules/@scope/pack1/main.js")
    );
  });

  it("browser field should work", () => {
    const result = resolver.sync(fixture, "@scope/pack2");
    expect(result.path).toBe(
      path.resolve(fixture, "./node_modules/@scope/pack2/main.js")
    );
  });

  it("folder request should work", () => {
    const result = resolver.sync(fixture, "@scope/pack2/lib");
    expect(result.path).toBe(
      path.resolve(fixture, "./node_modules/@scope/pack2/lib/index.js")
    );
  });
});
