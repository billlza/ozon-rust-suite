const env = process.env;

const directAuthDisabled = env.VITE_ENABLE_DIRECT_SKYBRIDGE_AUTH === "0";
const skybridgeAuthConfigured = Boolean(
  env.VITE_SKYBRIDGE_SUPABASE_URL?.trim() && env.VITE_SKYBRIDGE_SUPABASE_ANON_KEY?.trim()
);
const directAuthEnabled = skybridgeAuthConfigured && !directAuthDisabled;
const isVercelProduction = env.VERCEL_ENV === "production";

if (isVercelProduction && directAuthEnabled) {
  const verificationMode = (env.VITE_DIRECT_AUTH_VERIFICATION_MODE ?? "").trim().toLowerCase();
  if (!["none", "turnstile"].includes(verificationMode)) {
    throw new Error("VITE_DIRECT_AUTH_VERIFICATION_MODE must be either 'none' or 'turnstile'");
  }

  if (verificationMode === "turnstile") {
    requireNonEmpty("VITE_TURNSTILE_SITE_KEY");
    requireHttpsUrl("VITE_TURNSTILE_SCRIPT_URL");
    if (!env.VITE_TURNSTILE_SCRIPT_URL.includes("turnstile/v0/api.js")) {
      throw new Error("VITE_TURNSTILE_SCRIPT_URL must point to the Turnstile API script");
    }
  } else {
    rejectNonEmpty("VITE_TURNSTILE_SITE_KEY");
    rejectNonEmpty("VITE_TURNSTILE_SCRIPT_URL");
  }
}

console.log("Portal production env validation passed");

function requireNonEmpty(key) {
  if (!env[key]?.trim()) {
    throw new Error(`${key} is required when production direct SkyBridge auth is enabled`);
  }
}

function requireHttpsUrl(key) {
  const value = env[key]?.trim();
  if (!value) {
    throw new Error(`${key} is required when production direct SkyBridge auth is enabled`);
  }
  let url;
  try {
    url = new URL(value);
  } catch (error) {
    throw new Error(`${key} must be a valid URL: ${error.message}`);
  }
  if (url.protocol !== "https:") {
    throw new Error(`${key} must use https`);
  }
}

function rejectNonEmpty(key) {
  if (env[key]?.trim()) {
    throw new Error(`${key} must be empty when VITE_DIRECT_AUTH_VERIFICATION_MODE=none`);
  }
}
