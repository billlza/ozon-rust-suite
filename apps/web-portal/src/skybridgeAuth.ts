import { getPortalCopy, isPlaceholderDisplayName } from "./i18n";

export type SkybridgeAuthSession = {
  access_token: string;
  refresh_token?: string;
};

type AuthMode = "register" | "login";
type LoginMethod = "email" | "phone" | "nebula";

export type DirectSkybridgeAuthConfig = {
  authBase: string;
  publicAuthKey: string;
  phoneAuthEnabled: boolean;
};

type SkybridgeCurrentUserProfile = {
  id?: string;
  email?: string | null;
  phone?: string | null;
  user_metadata?: Record<string, unknown> | null;
};

const REQUEST_TIMEOUT_MS = 15_000;

export async function skybridgePasswordAuth(
  input: {
    mode: AuthMode;
    method: LoginMethod;
    identifier: string;
    password: string;
    phoneCode?: string;
    name: string;
    captchaToken?: string;
  },
  config: DirectSkybridgeAuthConfig
): Promise<SkybridgeAuthSession> {
  assertDirectAuthConfigured(config);
  if (input.mode === "register" && input.method === "nebula") {
    throw new Error(getPortalCopy().messages.nebulaCannotRegister);
  }
  if (input.method === "nebula") {
    return skybridgeNebulaLogin(input.identifier, input.password, config);
  }
  if (input.method === "phone") {
    assertPhoneAuthEnabled(config);
    return skybridgePhoneOtpLogin(input.identifier, input.phoneCode ?? "", config);
  }

  const endpoint =
    input.mode === "register"
      ? `${config.authBase}/auth/v1/signup`
      : `${config.authBase}/auth/v1/token?grant_type=password`;
  const body = {
    email: input.identifier.trim(),
    password: input.password,
    data: skybridgeMetadata(input.name, "email"),
    gotrue_meta_security: skybridgeCaptchaMetadata(input.captchaToken)
  };
  const response = await fetchWithTimeout(endpoint, {
    method: "POST",
    headers: skybridgeAuthHeaders(config),
    body: JSON.stringify(body)
  });
  const result = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(skybridgeErrorMessage(result, getPortalCopy().messages.authFailed));
  }
  const session = extractSkybridgeSession(result);
  if (!session?.access_token) {
    throw new Error(getPortalCopy().messages.emailVerifyThenLogin);
  }
  return session;
}

export async function skybridgeSendPhoneOtp(
  input: {
    phone: string;
    mode: AuthMode;
    name: string;
    captchaToken?: string;
  },
  config: DirectSkybridgeAuthConfig
) {
  assertPhoneAuthEnabled(config);
  assertDirectAuthConfigured(config);

  const body: Record<string, unknown> = {
    phone: input.phone.trim(),
    gotrue_meta_security: skybridgeCaptchaMetadata(input.captchaToken)
  };
  if (input.mode === "register") {
    body.data = skybridgeMetadata(input.name, "phone");
  }

  const response = await fetchWithTimeout(`${config.authBase}/auth/v1/otp`, {
    method: "POST",
    headers: skybridgeAuthHeaders(config),
    body: JSON.stringify(body)
  });
  const result = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(skybridgeErrorMessage(result, getPortalCopy().messages.smsCodeSendFailedFallback));
  }
}

export async function skybridgeUpdatePhoneRegistrationProfile(
  input: {
    accessToken: string;
    phone: string;
    email: string;
    name: string;
  },
  config: DirectSkybridgeAuthConfig
) {
  assertPhoneAuthEnabled(config);
  assertDirectAuthConfigured(config);

  const displayName = input.name.trim();
  const email = input.email.trim();
  if (!displayName || !email) {
    throw new Error(getPortalCopy().messages.phoneRegisterNeedsProfile);
  }

  const currentProfile = await skybridgeFetchCurrentUserProfile(input.accessToken, config);
  if (!shouldBootstrapSkybridgePhoneProfile(currentProfile)) {
    return;
  }

  const response = await fetchWithTimeout(`${config.authBase}/auth/v1/user`, {
    method: "PUT",
    headers: {
      ...skybridgeAuthHeaders(config),
      Authorization: `Bearer ${input.accessToken}`
    },
    body: JSON.stringify({
      email,
      data: {
        display_name: displayName,
        phone_number: input.phone.trim()
      }
    })
  });
  const result = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(skybridgeErrorMessage(result, getPortalCopy().messages.phoneProfileWriteFailed));
  }
}

function assertDirectAuthConfigured(config: DirectSkybridgeAuthConfig) {
  if (!config.authBase || !config.publicAuthKey.trim()) {
    throw new Error(getPortalCopy().messages.directAuthNotConfigured);
  }
}

function assertPhoneAuthEnabled(config: DirectSkybridgeAuthConfig) {
  if (!config.phoneAuthEnabled) {
    throw new Error(getPortalCopy().auth.phoneAuthUnavailable);
  }
}

async function skybridgePhoneOtpLogin(
  phone: string,
  token: string,
  config: DirectSkybridgeAuthConfig
): Promise<SkybridgeAuthSession> {
  assertPhoneAuthEnabled(config);
  const response = await fetchWithTimeout(`${config.authBase}/auth/v1/token`, {
    method: "POST",
    headers: skybridgeAuthHeaders(config),
    body: JSON.stringify({
      phone: phone.trim(),
      token: token.trim(),
      type: "sms",
      grant_type: "otp"
    })
  });
  const result = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(skybridgeErrorMessage(result, getPortalCopy().messages.phoneOtpFailed));
  }
  const session = extractSkybridgeSession(result);
  if (!session?.access_token) {
    throw new Error(getPortalCopy().messages.phoneOtpInvalid);
  }
  return session;
}

async function skybridgeFetchCurrentUserProfile(
  accessToken: string,
  config: DirectSkybridgeAuthConfig
): Promise<SkybridgeCurrentUserProfile | null> {
  const response = await fetchWithTimeout(`${config.authBase}/auth/v1/user`, {
    headers: {
      apikey: config.publicAuthKey.trim(),
      Authorization: `Bearer ${accessToken}`
    }
  });
  const result = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(skybridgeErrorMessage(result, getPortalCopy().messages.userProfileFailed));
  }
  return typeof result === "object" && result !== null ? (result as SkybridgeCurrentUserProfile) : null;
}

function shouldBootstrapSkybridgePhoneProfile(profile: SkybridgeCurrentUserProfile | null) {
  if (!profile) return false;
  const metadata = profile.user_metadata ?? {};
  const nebulaId = stringFromMetadata(metadata, "nebula_id") ?? stringFromMetadata(metadata, "nebulaId");
  const displayName =
    stringFromMetadata(metadata, "display_name") ??
    stringFromMetadata(metadata, "full_name") ??
    stringFromMetadata(metadata, "name");
  return !nebulaId && !profile.email && isPlaceholderDisplayName(displayName);
}

async function skybridgeNebulaLogin(
  nebulaId: string,
  password: string,
  config: DirectSkybridgeAuthConfig
): Promise<SkybridgeAuthSession> {
  const response = await fetchWithTimeout(`${config.authBase}/functions/v1/nebula-login`, {
    method: "POST",
    headers: skybridgeAuthHeaders(config),
    body: JSON.stringify({
      nebula_id: nebulaId.trim().toUpperCase(),
      password
    })
  });
  const result = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(skybridgeErrorMessage(result, getPortalCopy().messages.nebulaLoginFailed));
  }
  const session = extractSkybridgeSession(result);
  if (!session?.access_token) {
    throw new Error(getPortalCopy().messages.nebulaLoginInvalid);
  }
  return session;
}

function skybridgeAuthHeaders(config: DirectSkybridgeAuthConfig) {
  return {
    "Content-Type": "application/json",
    apikey: config.publicAuthKey.trim(),
    Authorization: `Bearer ${config.publicAuthKey.trim()}`
  };
}

function skybridgeMetadata(name: string, accountType: "email" | "phone") {
  return {
    account_type: accountType,
    full_name: name.trim() || undefined
  };
}

function skybridgeCaptchaMetadata(captchaToken?: string) {
  const token = captchaToken?.trim();
  return token ? { captcha_token: token } : undefined;
}

function stringFromMetadata(metadata: Record<string, unknown>, key: string) {
  const value = metadata[key];
  return typeof value === "string" ? value.trim() : undefined;
}

function extractSkybridgeSession(value: unknown): SkybridgeAuthSession | null {
  const record = value as Record<string, unknown> | null;
  if (!record) return null;
  if (typeof record.access_token === "string") {
    return {
      access_token: record.access_token,
      refresh_token: typeof record.refresh_token === "string" ? record.refresh_token : undefined
    };
  }
  const session = record.session ?? (record.data as Record<string, unknown> | undefined)?.session;
  if (typeof session === "object" && session !== null) {
    return extractSkybridgeSession(session);
  }
  return null;
}

function skybridgeErrorMessage(value: unknown, fallback: string) {
  const record = value as Record<string, unknown> | null;
  if (!record) return fallback;
  if (typeof record.error_description === "string") return record.error_description;
  if (typeof record.msg === "string") return record.msg;
  if (typeof record.message === "string") return record.message;
  if (typeof record.error === "string") return record.error;
  if (typeof record.error === "object" && record.error !== null) {
    const nested = record.error as Record<string, unknown>;
    if (typeof nested.message === "string") return nested.message;
  }
  return fallback;
}

async function fetchWithTimeout(url: string, init: RequestInit = {}, timeoutMs = REQUEST_TIMEOUT_MS) {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(url, {
      ...init,
      signal: init.signal ?? controller.signal
    });
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") {
      throw new Error(getPortalCopy().messages.requestTimeout);
    }
    throw error;
  } finally {
    window.clearTimeout(timeout);
  }
}
