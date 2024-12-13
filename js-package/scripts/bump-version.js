const fs = require("fs");
const path = require("path");

const pathToPackageJson = path.resolve(__dirname, "../package.json");

const packageJson = JSON.parse(fs.readFileSync(pathToPackageJson, "utf8"));

const currentVersion = packageJson.version;
const versionParts = currentVersion.split(".");

const flag = process.argv[2];

if (!flag) {
  console.error(
    "Please provide a version bump flag: --major, --minor, or --patch"
  );
  process.exit(1);
}

switch (flag) {
  case "--skip":
    console.log("Skipping version bump.");
    process.exit(1);
  case "--major":
    versionParts[0] = parseInt(versionParts[0]) + 1;
    versionParts[1] = 0;
    versionParts[2] = 0;
    break;
  case "--minor":
    versionParts[1] = parseInt(versionParts[1]) + 1;
    versionParts[2] = 0;
    break;
  case "--patch":
    versionParts[2] = parseInt(versionParts[2]) + 1;
    break;
  default:
    console.error("Invalid flag. Use --major, --minor, or --patch");
    process.exit(1);
}

packageJson.version = versionParts.join(".");

fs.writeFileSync(
  pathToPackageJson,
  JSON.stringify(packageJson, null, 2) + "\n",
  "utf8"
);

console.log(`Version bumped to ${packageJson.version}`);
