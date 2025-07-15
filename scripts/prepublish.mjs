import * as path from "node:path";
import fs from "node:fs/promises";
import { getPackageJson } from "./version.mjs";

const CpuToNodeArch = {
  x86_64: "x64",
  aarch64: "arm64",
  i686: "ia32",
  armv7: "arm"
};

const NodeArchToCpu = {
  x64: "x86_64",
  arm64: "aarch64",
  ia32: "i686",
  arm: "armv7"
};

const SysToNodePlatform = {
  linux: "linux",
  freebsd: "freebsd",
  darwin: "darwin",
  windows: "win32"
};

const AbiToNodeLibc = {
  gnu: "glibc",
  musl: "musl"
};

const UniArchsByPlatform = {
  darwin: ["x64", "arm64"]
};

/**
 * A triple is a specific format for specifying a target architecture.
 * Triples may be referred to as a target triple which is the architecture for the artifact produced, and the host triple which is the architecture that the compiler is running on.
 * The general format of the triple is `<arch><sub>-<vendor>-<sys>-<abi>` where:
 *   - `arch` = The base CPU architecture, for example `x86_64`, `i686`, `arm`, `thumb`, `mips`, etc.
 *   - `sub` = The CPU sub-architecture, for example `arm` has `v7`, `v7s`, `v5te`, etc.
 *   - `vendor` = The vendor, for example `unknown`, `apple`, `pc`, `nvidia`, etc.
 *   - `sys` = The system name, for example `linux`, `windows`, `darwin`, etc. none is typically used for bare-metal without an OS.
 *   - `abi` = The ABI, for example `gnu`, `android`, `eabi`, etc.
 */
function parseTriple(rawTriple) {
  const triple = rawTriple.endsWith("eabi")
    ? `${rawTriple.slice(0, -4)}-eabi`
    : rawTriple;
  const triples = triple.split("-");
  let cpu;
  let sys;
  let abi = null;
  if (triples.length === 4) {
    [cpu, , sys, abi = null] = triples;
  } else if (triples.length === 3) {
    [cpu, , sys] = triples;
  } else {
    [cpu, sys] = triples;
  }
  const platformName = SysToNodePlatform[sys] ?? sys;
  const arch = CpuToNodeArch[cpu] ?? cpu;
  return {
    platform: platformName,
    arch,
    abi,
    platformArchABI: abi
      ? `${platformName}-${arch}-${abi}`
      : `${platformName}-${arch}`,
    raw: rawTriple
  };
}

export async function prepublish_handler(options) {
  let root = process.cwd();
  let json = await getPackageJson(root);

  let { napi, version } = json;

  let optionalDependencies = {};
  for (let rawTarget of napi.targets) {
    let target = parseTriple(rawTarget);
    optionalDependencies[`${napi.packageName}-${target.platformArchABI}`] =
      version;
  }

  const packageFile = path.resolve(process.cwd(), "npm/package.json");
  let newPackageJson = {
    ...json,
    optionalDependencies
  };

  await fs.writeFile(
    packageFile,
    `${JSON.stringify(newPackageJson, null, 2)}\n`,
    "utf-8"
  );
}
