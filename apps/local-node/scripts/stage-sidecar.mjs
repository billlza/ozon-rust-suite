import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";

const repoRoot = resolve(import.meta.dirname, "../../..");
const targetTriple = process.env.TAURI_TARGET_TRIPLE || process.env.CARGO_BUILD_TARGET || rustHostTriple();
const exe = targetTriple.includes("windows") ? ".exe" : "";
const source = resolve(repoRoot, "target", targetTriple, "release", `ozon-local-node${exe}`);
const fallbackSource = resolve(repoRoot, "target", "release", `ozon-local-node${exe}`);
const destination = resolve(
  repoRoot,
  "apps/local-node/src-tauri/binaries",
  `ozon-local-node-${targetTriple}${exe}`
);

execFileSync("cargo", ["build", "-p", "ozon-local-node", "--release", "--target", targetTriple], {
  cwd: repoRoot,
  stdio: "inherit"
});

mkdirSync(dirname(destination), { recursive: true });
try {
  copyFileSync(source, destination);
} catch {
  copyFileSync(fallbackSource, destination);
}

console.log(`staged sidecar: ${destination}`);

function rustHostTriple() {
  const output = execFileSync("rustc", ["-vV"], { cwd: repoRoot, encoding: "utf8" });
  const line = output.split("\n").find((entry) => entry.startsWith("host:"));
  if (!line) {
    throw new Error("unable to detect rust host triple");
  }
  return line.replace("host:", "").trim();
}
