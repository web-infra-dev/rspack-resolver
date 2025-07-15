import path from "node:path";
import semver from "semver";

async function getCommitId() {
  const result = await $`git rev-parse --short HEAD`;
  return result.stdout.replace("\n", "");
}

export async function getLastVersion(root) {
  let pkg = await getPackageJson(root);
  return pkg.version;
}

export async function getPackageJson(root) {
  const pkgPath = path.resolve(root, "./npm/package.json");

  try {
    // Node >= 20
    const result = await import(pkgPath, {
      with: {
        type: "json"
      }
    });
    return result.default;
  } catch (e) {
    // Node < 20
    const result = await import(pkgPath, {
      assert: {
        type: "json"
      }
    });
    return result.default;
  }
}

export async function version_handler(version, options) {
  const allowedVersion = ["major", "minor", "patch"];
  const allowPretags = ["alpha", "beta", "rc"];
  const { pre } = options;
  if (!allowedVersion.includes(version)) {
    throw new Error(
      `version must be one of ${allowedVersion}, but you passed ${version}`
    );
  }

  const hasPre = pre && pre !== "none";

  if (hasPre && !allowPretags.includes(pre)) {
    throw new Error(
      `pre tag must be one of ${allowPretags}, but you passed ${pre}`
    );
  }
  const root = process.cwd();

  const lastVersion = await getLastVersion(root);
  let nextVersion;
  if (hasPre) {
    const existsPreTag = allowPretags.find(i => lastVersion.includes(i));
    if (existsPreTag) {
      // has pre tag
      if (existsPreTag === pre) {
        // same pre tag
        nextVersion = semver.inc(lastVersion, "prerelease", pre);
      } else {
        // different pre tag
        nextVersion = `${lastVersion.split(existsPreTag)[0]}${pre}.0`;
      }
    } else {
      nextVersion = semver.inc(lastVersion, `pre${version}`, pre);
    }
  } else {
    nextVersion = semver.inc(lastVersion, version);
  }

  let packageFile = path.resolve(root, "./npm/package.json");
  let packageJson = JSON.parse(await fs.readFile(packageFile));

  let newPackageJson = {
    ...packageJson,
    version: nextVersion
  };

  await fs.writeFile(
    packageFile,
    `${JSON.stringify(newPackageJson, null, 2)}\n`,
    "utf-8"
  );

  console.log(`Update ${newPackageJson.name}: ${newPackageJson.version}`);
}
