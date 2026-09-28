import { existsSync, readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";

const root = process.cwd();
const packageJson = JSON.parse(
  readFileSync(resolve(root, "package.json"), "utf8"),
);
const tauriConfig = JSON.parse(
  readFileSync(resolve(root, "src-tauri/tauri.conf.json"), "utf8"),
);
const cargoToml = readFileSync(
  resolve(root, "src-tauri/Cargo.toml"),
  "utf8",
);

const cargoPackage = cargoToml.match(
  /\[package\][\s\S]*?\nversion\s*=\s*"([^"]+)"/,
);
if (!cargoPackage) {
  throw new Error("Could not read [package] version from src-tauri/Cargo.toml.");
}

const versions = new Map([
  ["package.json", packageJson.version],
  ["src-tauri/tauri.conf.json", tauriConfig.version],
  ["src-tauri/Cargo.toml", cargoPackage[1]],
]);
const uniqueVersions = new Set(versions.values());
if (uniqueVersions.size !== 1) {
  throw new Error(
    `Release versions differ: ${Array.from(versions, ([file, version]) => `${file}=${version}`).join(", ")}`,
  );
}

const version = packageJson.version;
if (!/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error(`Release version is not semver-like: ${version}`);
}

const requestedVersion = process.env.DECC_EXPECTED_VERSION?.trim();
if (requestedVersion) {
  const expectedVersion = requestedVersion.replace(/^v(?=\d)/, "");
  if (version !== expectedVersion) {
    throw new Error(
      `Release version does not match DECC_EXPECTED_VERSION: expected ${expectedVersion}, repository metadata is ${version}`,
    );
  }
}

const identifier = tauriConfig.identifier;
if (
  typeof identifier !== "string" ||
  !/^[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+$/.test(identifier)
) {
  throw new Error(`Tauri bundle identifier is invalid: ${String(identifier)}`);
}
if (
  /(?:tauri|example|localhost)/i.test(identifier) &&
  identifier !== "com.kw0rk.decc"
) {
  throw new Error(
    `Tauri bundle identifier still looks like a development placeholder: ${identifier}`,
  );
}

if (!tauriConfig.productName?.trim()) {
  throw new Error("Tauri productName must be set.");
}
if (tauriConfig.bundle?.active !== true) {
  throw new Error("Tauri bundle.active must be true for release packaging.");
}

const icons = tauriConfig.bundle?.icon;
if (!Array.isArray(icons) || icons.length === 0) {
  throw new Error("Tauri bundle.icon must contain release icons.");
}

for (const icon of icons) {
  const path = resolve(root, "src-tauri", icon);
  if (!existsSync(path)) {
    throw new Error(`Configured release icon does not exist: ${icon}`);
  }
  if (!statSync(path).isFile() || statSync(path).size === 0) {
    throw new Error(`Configured release icon is empty or not a file: ${icon}`);
  }
}

console.log(
  `Release metadata OK · ${tauriConfig.productName} ${version} · ${identifier} · ${icons.length} icons`,
);
