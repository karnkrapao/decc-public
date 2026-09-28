import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { resolve, relative } from "node:path";
import { spawnSync } from "node:child_process";

const root = process.cwd();
const coreRoot = resolve(root, "src-tauri/src/core");
const coreModPath = resolve(coreRoot, "mod.rs");
const coreMod = readFileSync(coreModPath, "utf8");

const declaredModules = Array.from(
  coreMod.matchAll(/^\s*(?:#\[[^\]]+\]\s*)?pub\s+mod\s+([A-Za-z0-9_]+)\s*;/gm),
  (match) => match[1],
);

const missingModules = [];
for (const name of declaredModules) {
  const flat = resolve(coreRoot, `${name}.rs`);
  const nested = resolve(coreRoot, name, "mod.rs");
  if (!existsSync(flat) && !existsSync(nested)) {
    missingModules.push(`src-tauri/src/core/${name}.rs or ${name}/mod.rs`);
  }
}

if (missingModules.length > 0) {
  throw new Error(
    `Declared Rust core modules are missing from the working tree:\n- ${missingModules.join("\n- ")}`,
  );
}

function sourceFiles(directory) {
  if (!existsSync(directory)) return [];
  const files = [];
  for (const entry of readdirSync(directory)) {
    const path = resolve(directory, entry);
    const stat = statSync(path);
    if (stat.isDirectory()) {
      files.push(...sourceFiles(path));
    } else if (stat.isFile()) {
      files.push(path);
    }
  }
  return files;
}

const sourceRoots = [
  resolve(root, "src"),
  resolve(root, "src-tauri/src"),
  resolve(root, "scripts"),
  resolve(root, ".github/workflows"),
];
const sources = sourceRoots.flatMap(sourceFiles).map((path) =>
  relative(root, path).replaceAll("\\", "/"),
);

if (existsSync(resolve(root, ".git")) && sources.length > 0) {
  const tracked = spawnSync("git", ["ls-files", "-z"], {
    cwd: root,
    encoding: "utf8",
  });
  if (tracked.error || tracked.status !== 0) {
    throw new Error(
      `Could not enumerate tracked source files: ${tracked.error?.message ?? tracked.stderr.trim()}`,
    );
  }

  const trackedPaths = new Set(
    tracked.stdout
      .split("\0")
      .filter(Boolean)
      .map((path) => path.replaceAll("\\", "/")),
  );
  const sensitiveTrackedPathPattern =
    /(^|\/)(?:\.env(?:\..+)?|id_rsa|id_ed25519|credentials?|secrets?|tokens?|passwords?)(?:$|\/)|\.(?:pem|key|p12|pfx|jks|keystore|db|sqlite|sqlite3|dmp|core)$/i;
  const publicEnvironmentTemplates = new Set([
    ".env.example",
    ".env.sample",
    ".env.template",
  ]);
  const internalOnlyPaths = new Set(["AGENTS.md"]);
  const sensitiveTrackedPaths = Array.from(trackedPaths).filter((path) => {
    if (internalOnlyPaths.has(path)) return true;
    const baseName = path.split("/").at(-1);
    if (baseName && publicEnvironmentTemplates.has(baseName)) return false;
    return sensitiveTrackedPathPattern.test(path);
  });
  if (sensitiveTrackedPaths.length > 0) {
    throw new Error(
      `Potentially private/local files are tracked by Git:\n- ${sensitiveTrackedPaths.join("\n- ")}`,
    );
  }

  const untrackedSources = sources.filter((path) => !trackedPaths.has(path));
  if (untrackedSources.length > 0) {
    throw new Error(
      `Release-critical source files exist locally but are not tracked by Git:\n- ${untrackedSources.join("\n- ")}`,
    );
  }

  const ignored = spawnSync("git", ["check-ignore", "--stdin"], {
    cwd: root,
    input: sources.join("\n"),
    encoding: "utf8",
  });

  if (ignored.error) {
    throw new Error(`Could not audit source ignore rules: ${ignored.error.message}`);
  }

  if (ignored.status === 0 && ignored.stdout.trim()) {
    throw new Error(
      `Source files are hidden by .gitignore and may disappear from a clone:\n- ${ignored.stdout
        .trim()
        .split(/\r?\n/)
        .join("\n- ")}`,
    );
  }

  if (ignored.status !== 0 && ignored.status !== 1) {
    throw new Error(
      `git check-ignore failed while auditing source files: ${ignored.stderr.trim()}`,
    );
  }

  const trackedDocs = spawnSync("git", ["ls-files", "--", "*.md", "**/*.md"], {
    cwd: root,
    encoding: "utf8",
  });
  if (trackedDocs.error || trackedDocs.status !== 0) {
    throw new Error(
      `Could not enumerate tracked documentation: ${trackedDocs.error?.message ?? trackedDocs.stderr.trim()}`,
    );
  }

  const localHomePattern = /\/Users\/[^/\s]+\/|\/home\/[^/\s]+\/|[A-Za-z]:\\Users\\[^\\\s]+\\/;
  const leakedDocPaths = [];
  for (const relativePath of trackedDocs.stdout.split(/\r?\n/).filter(Boolean)) {
    const content = readFileSync(resolve(root, relativePath), "utf8");
    content.split(/\r?\n/).forEach((line, index) => {
      if (localHomePattern.test(line)) {
        leakedDocPaths.push(`${relativePath}:${index + 1}`);
      }
    });
  }

  if (leakedDocPaths.length > 0) {
    throw new Error(
      `Tracked documentation contains machine-specific user-home paths:\n- ${leakedDocPaths.join("\n- ")}`,
    );
  }
}

console.log(
  `Source integrity OK · ${declaredModules.length} Rust core modules · ${sources.length} release-critical files · all Git-tracked · no source paths ignored · docs free of local user-home paths`,
);
