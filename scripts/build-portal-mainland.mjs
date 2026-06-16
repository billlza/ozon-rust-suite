import { spawnSync } from "node:child_process";
import { existsSync, readFileSync, readdirSync, rmSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "..");
const defaultEnvFile = path.join(repoRoot, "deploy", ".env.portal-mainland.example");
const envFile = resolveEnvFile(process.argv);
const envValues = loadEnvFile(envFile);

const buildEnv = {
  ...envValues,
  ...process.env
};
buildEnv.VITE_ENABLE_NEBULA_OAUTH_ENTRY = buildEnv.VITE_ENABLE_NEBULA_OAUTH_ENTRY || "1";
// Email + password login backed by cloud-api (/auth/login, /auth/register). Self-contained,
// so customers can sign in even when the enterprise SSO identity provider is down. Requires
// OZON_SUITE_ALLOW_LOCAL_NEBULA_REGISTRATION=true on cloud-api for new-account registration.
buildEnv.VITE_ENABLE_LOCAL_PASSWORD_AUTH = buildEnv.VITE_ENABLE_LOCAL_PASSWORD_AUTH || "1";
buildEnv.VITE_ENABLE_DIRECT_SKYBRIDGE_AUTH = "0";
buildEnv.VITE_ENABLE_SKYBRIDGE_PHONE_AUTH = "0";
buildEnv.VITE_SKYBRIDGE_PHONE_SMS_PROVIDER_READY = "0";
buildEnv.VITE_ALLOW_LEGACY_SKYBRIDGE_ANON_KEY = "";
buildEnv.VITE_TURNSTILE_SITE_KEY = "";
buildEnv.VITE_TURNSTILE_SCRIPT_URL = "";

requireNonEmpty(buildEnv, "VITE_CLOUD_API");
requireNonEmpty(buildEnv, "VITE_NEBULA_BASE_URL");
requireNonEmpty(buildEnv, "VITE_NEBULA_CLIENT_ID");
requireUrl(buildEnv, "VITE_CLOUD_API");
requireUrl(buildEnv, "VITE_NEBULA_BASE_URL");
rejectNonEmpty(buildEnv, "VITE_SKYBRIDGE_SUPABASE_URL");
rejectNonEmpty(buildEnv, "VITE_SKYBRIDGE_SUPABASE_PUBLISHABLE_KEY");
rejectNonEmpty(buildEnv, "VITE_SKYBRIDGE_SUPABASE_ANON_KEY");
rejectNonEmpty(buildEnv, "VITE_ALLOW_LEGACY_SKYBRIDGE_ANON_KEY");
rejectNonEmpty(buildEnv, "VITE_TURNSTILE_SITE_KEY");
rejectNonEmpty(buildEnv, "VITE_TURNSTILE_SCRIPT_URL");

rmSync(path.join(repoRoot, "apps", "web-portal", "dist"), { force: true, recursive: true });
run("pnpm", ["--dir", "apps/web-portal", "build"], buildEnv);
verifyBundle(path.join(repoRoot, "apps", "web-portal", "dist"));
console.log(`Mainland portal bundle is ready: ${path.join(repoRoot, "apps", "web-portal", "dist")}`);

function resolveEnvFile(argv) {
  const flagIndex = argv.indexOf("--env-file");
  if (flagIndex >= 0) {
    const value = argv[flagIndex + 1];
    if (!value) {
      throw new Error("--env-file requires a path");
    }
    return path.resolve(repoRoot, value);
  }
  const positional = argv.find((arg) => arg.endsWith(".env") || arg.includes(".env."));
  return positional ? path.resolve(repoRoot, positional) : defaultEnvFile;
}

function loadEnvFile(filePath) {
  if (!existsSync(filePath)) {
    throw new Error(`Env file not found: ${filePath}`);
  }
  const values = {};
  const raw = readFileSync(filePath, "utf8");
  for (const [index, line] of raw.split(/\r?\n/).entries()) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith("#")) continue;
    const separator = trimmed.indexOf("=");
    if (separator <= 0) {
      throw new Error(`Invalid env line ${index + 1} in ${filePath}`);
    }
    const key = trimmed.slice(0, separator).trim();
    let value = trimmed.slice(separator + 1).trim();
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    values[key] = value;
  }
  return values;
}

function requireNonEmpty(env, key) {
  if (!env[key]?.trim()) {
    throw new Error(`${key} is required for the mainland portal build`);
  }
}

function requireUrl(env, key) {
  try {
    const url = new URL(env[key]);
    if (url.protocol !== "https:") {
      throw new Error("must use https");
    }
  } catch (error) {
    throw new Error(`${key} must be a valid https URL: ${error.message}`);
  }
}

function rejectNonEmpty(env, key) {
  if (env[key]?.trim()) {
    throw new Error(`${key} must be empty for the mainland portal build`);
  }
}

function run(command, args, env) {
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    env,
    stdio: "inherit"
  });
  if (result.status !== 0) {
    throw new Error(`${command} ${args.join(" ")} failed with exit code ${result.status}`);
  }
}

function verifyBundle(distDir) {
  if (!existsSync(path.join(distDir, "index.html"))) {
    throw new Error(`Portal dist is missing index.html: ${distDir}`);
  }
  const forbidden = [
    "challenges.cloudflare.com",
    "turnstile/v0/api.js",
    "images.unsplash.com",
    "unsplash.com",
    "VITE_SKYBRIDGE_SUPABASE",
    "VITE_ALLOW_LEGACY_SKYBRIDGE_ANON_KEY",
    "VITE_ENABLE_SKYBRIDGE_PHONE_AUTH",
    "VITE_SKYBRIDGE_PHONE_SMS_PROVIDER_READY",
    "auth/v1/otp",
    "VITE_TURNSTILE_SITE_KEY",
    "兼容邮箱/手机号登录"
  ];
  for (const filePath of listFiles(distDir)) {
    if (!/\.(html|js|json)$/i.test(filePath)) continue;
    const content = readFileSync(filePath, "utf8");
    for (const needle of forbidden) {
      if (content.includes(needle)) {
        throw new Error(`Mainland bundle contains forbidden marker "${needle}" in ${filePath}`);
      }
    }
  }
}

function listFiles(dir) {
  const output = [];
  for (const entry of readdirSync(dir)) {
    const entryPath = path.join(dir, entry);
    const stats = statSync(entryPath);
    if (stats.isDirectory()) {
      output.push(...listFiles(entryPath));
    } else if (stats.isFile()) {
      output.push(entryPath);
    }
  }
  return output;
}
