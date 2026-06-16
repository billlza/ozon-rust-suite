const env = process.env;

const directAuthRequested = isEnabledFlag(env.VITE_ENABLE_DIRECT_SKYBRIDGE_AUTH);
const directAuthDisabled = isDisabledFlag(env.VITE_ENABLE_DIRECT_SKYBRIDGE_AUTH);
const phoneAuthEnabled = isEnabledFlag(env.VITE_ENABLE_SKYBRIDGE_PHONE_AUTH);
const phoneSmsProviderReady = isEnabledFlag(env.VITE_SKYBRIDGE_PHONE_SMS_PROVIDER_READY);
const legacyAnonKeyAllowed = isEnabledFlag(env.VITE_ALLOW_LEGACY_SKYBRIDGE_ANON_KEY);
const skybridgeAuthConfigured = Boolean(env.VITE_SKYBRIDGE_SUPABASE_URL?.trim() && skybridgePublicKeyConfigured());
const directAuthEnabled = skybridgeAuthConfigured && directAuthRequested;
const isVercelProduction = env.VERCEL_ENV === "production";

if (isVercelProduction && skybridgeAuthConfigured && !directAuthRequested && !directAuthDisabled) {
  throw new Error("VITE_ENABLE_DIRECT_SKYBRIDGE_AUTH must be explicitly set to 1 or 0 when SkyBridge auth env vars are present");
}

if (isVercelProduction && directAuthRequested && !skybridgeAuthConfigured) {
  throw new Error("VITE_ENABLE_DIRECT_SKYBRIDGE_AUTH=1 requires SkyBridge auth URL and publishable key");
}

if (isVercelProduction && directAuthEnabled) {
  requireNonEmpty("VITE_SKYBRIDGE_SUPABASE_URL");
  requireSupportedSkybridgeBrowserKey();

  const verificationMode = (env.VITE_DIRECT_AUTH_VERIFICATION_MODE ?? "").trim().toLowerCase();
  if (!["none", "turnstile"].includes(verificationMode)) {
    throw new Error("VITE_DIRECT_AUTH_VERIFICATION_MODE must be either 'none' or 'turnstile'");
  }

  if (verificationMode === "turnstile") {
    requireNonEmpty("VITE_TURNSTILE_SITE_KEY");
    const turnstileUrl = requireHttpsUrl("VITE_TURNSTILE_SCRIPT_URL");
    if (!env.VITE_TURNSTILE_SCRIPT_URL.includes("turnstile/v0/api.js")) {
      throw new Error("VITE_TURNSTILE_SCRIPT_URL must point to the Turnstile API script");
    }
    if (turnstileUrl.hostname !== "challenges.cloudflare.com") {
      throw new Error("VITE_TURNSTILE_SCRIPT_URL must use challenges.cloudflare.com");
    }
  } else {
    rejectNonEmpty("VITE_TURNSTILE_SITE_KEY");
    rejectNonEmpty("VITE_TURNSTILE_SCRIPT_URL");
  }
}

if (isVercelProduction && phoneAuthEnabled && !directAuthEnabled) {
  throw new Error("VITE_ENABLE_SKYBRIDGE_PHONE_AUTH=1 requires configured direct SkyBridge auth");
}

if (isVercelProduction && phoneAuthEnabled && !phoneSmsProviderReady) {
  throw new Error("VITE_ENABLE_SKYBRIDGE_PHONE_AUTH=1 requires VITE_SKYBRIDGE_PHONE_SMS_PROVIDER_READY=1 after SMS provider E2E validation");
}

console.log("Portal production env validation passed");

function skybridgePublicKeyConfigured() {
  return Boolean(
    env.VITE_SKYBRIDGE_SUPABASE_PUBLISHABLE_KEY?.trim() ||
      env.VITE_SKYBRIDGE_PUBLISHABLE_KEY?.trim() ||
      env.VITE_SKYBRIDGE_SUPABASE_ANON_KEY?.trim() ||
      env.VITE_SUPABASE_ANON_KEY?.trim()
  );
}

function requireSupportedSkybridgeBrowserKey() {
  if (env.VITE_SKYBRIDGE_SUPABASE_PUBLISHABLE_KEY?.trim() || env.VITE_SKYBRIDGE_PUBLISHABLE_KEY?.trim()) {
    rejectNonEmpty("VITE_SKYBRIDGE_SUPABASE_ANON_KEY");
    rejectNonEmpty("VITE_SUPABASE_ANON_KEY");
    return;
  }
  if ((env.VITE_SKYBRIDGE_SUPABASE_ANON_KEY?.trim() || env.VITE_SUPABASE_ANON_KEY?.trim()) && legacyAnonKeyAllowed) {
    return;
  }
  throw new Error(
    "Production direct SkyBridge auth requires a publishable key, or VITE_ALLOW_LEGACY_SKYBRIDGE_ANON_KEY=1 during migration"
  );
}

function isEnabledFlag(value) {
  return ["1", "true", "yes"].includes((value ?? "").trim().toLowerCase());
}

function isDisabledFlag(value) {
  return ["0", "false", "no"].includes((value ?? "").trim().toLowerCase());
}

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
  return url;
}

function rejectNonEmpty(key) {
  if (env[key]?.trim()) {
    throw new Error(`${key} must be empty when VITE_DIRECT_AUTH_VERIFICATION_MODE=none`);
  }
}
