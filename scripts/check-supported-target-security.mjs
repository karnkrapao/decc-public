import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { spawnSync } from "node:child_process";

const root = process.cwd();
const manifestPath = resolve(root, "src-tauri/Cargo.toml");
const lockPath = resolve(root, "src-tauri/Cargo.lock");
const lock = readFileSync(lockPath, "utf8");

const supportedTargets = [
  "x86_64-pc-windows-msvc",
  "aarch64-apple-darwin",
  "x86_64-apple-darwin",
];

const vulnerableGlibVersions = Array.from(
  lock.matchAll(
    /\[\[package\]\]\s+name = "glib"\s+version = "(\d+)\.(\d+)\.(\d+)"/g,
  ),
  (match) => ({
    version: match[1] + "." + match[2] + "." + match[3],
    major: Number(match[1]),
    minor: Number(match[2]),
  }),
).filter(({ major, minor }) => major === 0 && minor >= 15 && minor < 20);

for (const { version } of vulnerableGlibVersions) {
  for (const target of supportedTargets) {
    const result = spawnSync(
      "cargo",
      [
        "tree",
        "--locked",
        "--manifest-path",
        manifestPath,
        "--target",
        target,
        "-i",
        "glib@" + version,
      ],
      {
        cwd: root,
        encoding: "utf8",
      },
    );

    if (result.error || result.status !== 0) {
      throw new Error(
        "Could not inspect Rust dependency graph for " +
          target +
          ": " +
          (result.error?.message ?? result.stderr.trim()),
      );
    }

    if (result.stdout.trim()) {
      throw new Error(
        "GHSA-wrw7-89jp-8q8g boundary violated: glib " +
          version +
          " is reachable on supported target " +
          target +
          ".\n" +
          result.stdout.trim(),
      );
    }
  }
}

console.log(
  "Supported-target Rust security boundary OK · GHSA-wrw7-89jp-8q8g vulnerable glib versions are not reachable on Windows/macOS targets",
);
