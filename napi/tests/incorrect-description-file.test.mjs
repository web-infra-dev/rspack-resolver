import { describe, it, expect } from "rstack/test";
import { ResolverFactory } from "../index.js";
import * as path from "node:path";

const fixtureDir = path.resolve("fixtures/enhanced_resolve/test/fixtures");
const fixtures = path.join(fixtureDir, "incorrect-package");

function p(...args) {
  return path.join(fixtures, ...args);
}

describe("incorrect description file", () => {
  const resolver = new ResolverFactory({});

  it("should not resolve main in incorrect description file #1", () => {
    const result = resolver.sync(p("pack1"), ".");
    expect(result.error).toBeTruthy();
  });

  it("should not resolve main in incorrect description file #2", () => {
    const result = resolver.sync(p("pack2"), ".");
    expect(result.error).toBeTruthy();
  });
});
