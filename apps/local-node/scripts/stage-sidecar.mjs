import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";

// Stage the local-node sidecar binary for Tauri's `externalBin`.
//
// Default: build `--release` for packaging (used by `pretauri:build`). A release
// build has `debug_assertions` false, so the mock Ozon connector is disabled
// (see apps/local-node/src/main.rs) and the installer ships the real-mode sidecar.
//
// With `--debug` (used by `tauri:dev`): build a host DEBUG binary, where
// `debug_assertions` is true and the mock connector is ENABLED. Without this the
// dev app runs in mock mode but the staged release sidecar exits on startup with
// "mock Ozon connector is disabled in non-debug builds", crash-looping the node.
const debug = process.argv.includes("--debug");
const repoRoot = resolve(import.meta.dirname, "../../..");
const targetTriple = process.env.TAURI_TARGET_TRIPLE || process.env.CARGO_BUILD_TARGET || rustHostTriple();
const exe = targetTriple.includes("windows") ? ".exe" : "";

const cargoArgs = ["build", "-p", "ozon-local-node"];
let source;
if (debug) {
  // Host debug build -> target/debug (shares artifacts with cargo test/check).
  source = resolve(repoRoot, "target", "debug", `ozon-local-node${exe}`);
} else {
  cargoArgs.push("--release", "--target", targetTriple);
  source = resolve(repoRoot, "target", targetTriple, "release", `ozon-local-node${exe}`);
}
const fallbackSource = resolve(repoRoot, "target", debug ? "debug" : "release", `ozon-local-node${exe}`);
const destination = resolve(
  repoRoot,
  "apps/local-node/src-tauri/binaries",
  `ozon-local-node-${targetTriple}${exe}`
);

execFileSync("cargo", cargoArgs, {
  cwd: repoRoot,
  stdio: "inherit"
});

mkdirSync(dirname(destination), { recursive: true });
try {
  copyFileSync(source, destination);
} catch {
  copyFileSync(fallbackSource, destination);
}

console.log(`staged ${debug ? "debug" : "release"} sidecar: ${destination}`);

function rustHostTriple() {
  const output = execFileSync("rustc", ["-vV"], { cwd: repoRoot, encoding: "utf8" });
  const line = output.split("\n").find((entry) => entry.startsWith("host:"));
  if (!line) {
    throw new Error("unable to detect rust host triple");
  }
  return line.replace("host:", "").trim();
}
