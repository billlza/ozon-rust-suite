import React, { useEffect, useMemo, useReducer, useRef, useState } from "react";
import { createRoot, type Root } from "react-dom/client";
import {
  AlertCircle,
  ArrowRight,
  Bot,
  Boxes,
  CheckCircle2,
  Clipboard,
  Download,
  ExternalLink,
  KeyRound,
  LogIn,
  LogOut,
  Mail,
  MonitorCheck,
  Orbit,
  PackageCheck,
  Radio,
  RefreshCcw,
  ShieldCheck,
  Smartphone,
  UserPlus,
  X
} from "lucide-react";
import { QRCodeSVG } from "qrcode.react";
import { CustomerGuide } from "./CustomerGuide";
import {
  applyDocumentLocale,
  getCurrentLocale,
  getPortalCopy,
  portalMessageTone,
  useI18n
} from "./i18n";
import { LanguageSwitch } from "./LanguageSwitch";
import "./styles.css";

const API_BASE = normalizeBaseUrl(import.meta.env.VITE_CLOUD_API ?? defaultCloudApiBase());
const LOCAL_NODE_API = normalizeBaseUrl(import.meta.env.VITE_LOCAL_NODE_API ?? "http://127.0.0.1:8790");
const LOCAL_CONSOLE_URL = normalizeOptionalUrl(
  import.meta.env.VITE_LOCAL_CONSOLE_URL ?? (import.meta.env.DEV ? "http://127.0.0.1:5173" : "")
);
const SESSION_KEY = "ozon-rust-suite.portal.session";
const NEBULA_OAUTH_STORAGE_KEY = "ozon-rust-suite.nebula.oauth";
const DEFAULT_NEBULA_SCOPE = "openid profile email offline_access";
const DEFAULT_SKYBRIDGE_SUPABASE_URL = "https://hloqytmhjludmuhwyyzb.supabase.co";
const LOCAL_DEV_AUTH_ENABLED =
  import.meta.env.DEV || ["1", "true", "yes"].includes((import.meta.env.VITE_ENABLE_LOCAL_DEV_AUTH ?? "").toLowerCase());
const SKYBRIDGE_AUTH_BASE = normalizeBaseUrl(
  import.meta.env.VITE_SKYBRIDGE_SUPABASE_URL ??
    import.meta.env.VITE_SUPABASE_URL ??
    (import.meta.env.DEV ? DEFAULT_SKYBRIDGE_SUPABASE_URL : "")
);
const SKYBRIDGE_PUBLIC_AUTH_KEY =
  import.meta.env.VITE_SKYBRIDGE_SUPABASE_PUBLISHABLE_KEY ??
  import.meta.env.VITE_SKYBRIDGE_PUBLISHABLE_KEY ??
  import.meta.env.VITE_SKYBRIDGE_SUPABASE_ANON_KEY ??
  import.meta.env.VITE_SUPABASE_ANON_KEY ??
  "";
const SKYBRIDGE_AUTH_CONFIGURED = Boolean(SKYBRIDGE_AUTH_BASE && SKYBRIDGE_PUBLIC_AUTH_KEY.trim());
const DIRECT_SKYBRIDGE_AUTH_ENABLED =
  SKYBRIDGE_AUTH_CONFIGURED &&
  ["1", "true", "yes"].includes((import.meta.env.VITE_ENABLE_DIRECT_SKYBRIDGE_AUTH ?? "").toLowerCase());
const SKYBRIDGE_PHONE_SMS_PROVIDER_READY = ["1", "true", "yes"].includes(
  (import.meta.env.VITE_SKYBRIDGE_PHONE_SMS_PROVIDER_READY ?? "").toLowerCase()
);
const SKYBRIDGE_PHONE_AUTH_ENABLED =
  DIRECT_SKYBRIDGE_AUTH_ENABLED &&
  ["1", "true", "yes"].includes((import.meta.env.VITE_ENABLE_SKYBRIDGE_PHONE_AUTH ?? "").toLowerCase()) &&
  SKYBRIDGE_PHONE_SMS_PROVIDER_READY;
const DEV_NEBULA_OAUTH_BASE = import.meta.env.DEV ? "http://127.0.0.1:8788" : "";
const DEV_NEBULA_CLIENT_ID = import.meta.env.DEV ? "ozon_rust_suite_portal" : "";
const NEBULA_OAUTH_BASE = normalizeBaseUrl(
  import.meta.env.VITE_NEBULA_BASE_URL ?? import.meta.env.VITE_SKYBRIDGE_AUTH_BASEURL ?? DEV_NEBULA_OAUTH_BASE
);
const NEBULA_CLIENT_ID = (import.meta.env.VITE_NEBULA_CLIENT_ID ?? DEV_NEBULA_CLIENT_ID).trim();
const NEBULA_SCOPE = (import.meta.env.VITE_NEBULA_SCOPE ?? DEFAULT_NEBULA_SCOPE).trim();
const NEBULA_OAUTH_CONFIGURED = Boolean(NEBULA_OAUTH_BASE && NEBULA_CLIENT_ID);
const NEBULA_OAUTH_ENTRY_ENABLED =
  NEBULA_OAUTH_CONFIGURED &&
  !["0", "false", "no"].includes((import.meta.env.VITE_ENABLE_NEBULA_OAUTH_ENTRY ?? "").toLowerCase());
const SKYBRIDGE_TURNSTILE_SITE_KEY = (import.meta.env.VITE_TURNSTILE_SITE_KEY ?? "").trim();
const SKYBRIDGE_TURNSTILE_CONFIGURED = Boolean(SKYBRIDGE_TURNSTILE_SITE_KEY);
const SKYBRIDGE_TURNSTILE_SCRIPT_URL = (SKYBRIDGE_TURNSTILE_CONFIGURED
  ? import.meta.env.VITE_TURNSTILE_SCRIPT_URL ?? ""
  : ""
).trim();
const REQUEST_TIMEOUT_MS = 15_000;

type User = {
  id: string;
  tenant_id: string;
  nebula_id: string;
  nebula_source: "skybridge" | "local_dev";
  skybridge_user_id?: string | null;
  email?: string | null;
  phone?: string | null;
  name?: string | null;
  role: "user" | "admin";
  email_verified: boolean;
  phone_verified: boolean;
};

type Entitlement = {
  id: string;
  plan_code: string;
  features: string[];
  expires_at: string;
  revoked_at?: string | null;
};

type Order = {
  id: string;
  status: string;
  plan_code: string;
  payment_provider: string;
  payment_reference: string;
  amount_minor: number;
  currency: string;
  checkout_session_id?: string | null;
  payment_intent_id?: string | null;
  paid_at?: string | null;
  created_at: string;
  confirmed_at?: string | null;
};

type PaymentSession = {
  provider: string;
  checkout_url?: string | null;
  checkout_session_id?: string | null;
  native_code_url?: string | null;
  payment_reference: string;
  amount_minor: number;
  currency: string;
  message: string;
};

type OrderApiResponse = {
  order: Order;
  payment?: PaymentSession | null;
};

type Device = {
  id: string;
  name: string;
  status: string;
  last_seen_at?: string | null;
};

type Lease = {
  lease_id: string;
  device_id: string;
  entitlement_id: string;
  features: string[];
  expires_at: string;
};

type ReleaseArtifact = {
  url: string;
  sha256: string;
};

type LocalNodeReleaseManifest = {
  version: string;
  commit: string;
  msi: ReleaseArtifact;
  exe: ReleaseArtifact;
  macos_aarch64_dmg?: ReleaseArtifact;
  dmg?: ReleaseArtifact;
};

type Downloads = {
  release_manifest_url: string;
  release_manifest: LocalNodeReleaseManifest;
  local_node?: string;
  local_node_msi?: string;
  local_node_exe?: string;
  local_node_macos_dmg?: string;
  local_node_msi_sha256?: string;
  local_node_exe_sha256?: string;
  local_node_macos_dmg_sha256?: string;
  version?: string;
  checksum?: string;
  checksum_sha256?: string;
  openclaw_plugin?: string;
  openclaw_manifest?: string;
  local_manifest_url?: string;
};

type Session = {
  token: string;
  user: User;
};

type LocalNodePhase = "idle" | "checking" | "online" | "degraded" | "offline" | "blocked";

type LocalNodeHealth = {
  service: string;
  status: string;
  skill_port: number;
  agent_port: number;
  protocol_version?: string;
  build_commit?: string;
  package_version?: string;
  supervisor?: string;
  features: string[];
  real_ozon_enabled: boolean;
};

type LocalNodeManifest = {
  name: string;
  version: string;
  base_url: string;
  auth: {
    header: string;
    source: string;
  };
  tools: Array<{
    name: string;
    path: string;
    risk: string;
    approval_required: boolean;
  }>;
  safety_rules: string[];
};

type LocalPortalStatus = {
  service: string;
  status: string;
  checked_at: string;
  skill_api: string;
  agent_api: string;
  manifest_url: string;
  bridge_auth_header: string;
  protocol_version?: string;
  build_commit?: string;
  package_version?: string;
  real_ozon_enabled: boolean;
  device_fingerprint: string;
  ozon?: {
    configured: boolean;
    issue?: string | null;
  };
  openai?: {
    configured: boolean;
    image_model: string;
    issue?: string | null;
  };
  poster_generation?: {
    preferred: string;
    openclaw_bridge_ready: boolean;
    handoff_path: string;
    manifest_url: string;
    api_fallback_configured: boolean;
    api_fallback_model: string | null;
    api_fallback_issue: string | null;
    message: string;
  };
  lease: {
    configured: boolean;
    valid: boolean;
    lease_id: string | null;
    device_id: string | null;
    features: string[];
    expires_at: string | null;
    issue: string | null;
  };
  features: string[];
};

type LocalNodeProbe = {
  phase: LocalNodePhase;
  message: string;
  checkedAt?: string;
  health?: LocalNodeHealth;
  manifest?: LocalNodeManifest;
  portal?: LocalPortalStatus;
};

type LocalLeaseStatus = LocalPortalStatus["lease"];

type PortalLeaseResponse = {
  accepted: boolean;
  lease: LocalLeaseStatus;
  saved_at: string;
};

type ApiError = {
  error: string;
};

type AuthMode = "register" | "login";
type LoginMethod = "email" | "phone" | "nebula";
type AuthPhase =
  | "idle"
  | "authenticating_skybridge"
  | "creating_service_session"
  | "authenticating_local_dev"
  | "refreshing"
  | "authenticated"
  | "failed"
  | "signed_out";

type AuthState = {
  phase: AuthPhase;
  message: string;
  requestId: number;
};

type AuthAction =
  | { type: "begin"; phase: AuthPhase; message: string; requestId: number }
  | { type: "success"; message: string; requestId: number }
  | { type: "failure"; message: string; requestId: number }
  | { type: "signed_out"; message: string; requestId: number };

type SkybridgeAuthSession = {
  access_token: string;
  refresh_token?: string;
};

type NebulaOAuthFlow = "login" | "register";

type NebulaOAuthSession = {
  baseUrl: string;
  clientId: string;
  redirectUri: string;
  codeVerifier: string;
  state: string;
  flow: NebulaOAuthFlow;
};

type TurnstileRenderOptions = {
  sitekey: string;
  action?: string;
  callback?: (token: string) => void;
  "expired-callback"?: () => void;
  "error-callback"?: (error?: string) => void;
};

type TurnstileApi = {
  render: (container: HTMLElement | string, options: TurnstileRenderOptions) => string;
  reset: (widgetId?: string) => void;
  remove: (widgetId?: string) => void;
};

declare global {
  interface Window {
    turnstile?: TurnstileApi;
  }
}

function App() {
  const { copy, locale, setLocale } = useI18n();
  const portalCopy = copy.portal;
  const [authDialogMode, setAuthDialogMode] = useState<AuthMode | null>(null);
  const [authMode, setAuthMode] = useState<AuthMode>("login");
  const [authMethod, setAuthMethod] = useState<LoginMethod>("email");
  const [skybridgeIdentifier, setSkybridgeIdentifier] = useState("");
  const [skybridgePassword, setSkybridgePassword] = useState("");
  const [skybridgePhoneCode, setSkybridgePhoneCode] = useState("");
  const [skybridgePhoneEmail, setSkybridgePhoneEmail] = useState("");
  const [skybridgeOtpBusy, setSkybridgeOtpBusy] = useState(false);
  const [skybridgeOtpStatus, setSkybridgeOtpStatus] = useState("");
  const [skybridgeName, setSkybridgeName] = useState("");
  const [skybridgeAccessToken, setSkybridgeAccessToken] = useState("");
  const [turnstileToken, setTurnstileToken] = useState("");
  const [turnstileStatus, setTurnstileStatus] = useState(
    SKYBRIDGE_TURNSTILE_CONFIGURED ? portalCopy.messages.turnstileWaiting : portalCopy.messages.turnstileNotRequired
  );
  const turnstileContainerRef = useRef<HTMLDivElement | null>(null);
  const turnstileWidgetId = useRef<string | null>(null);
  const turnstileTokenRef = useRef("");

  const [localMode, setLocalMode] = useState<AuthMode>("register");
  const [localMethod, setLocalMethod] = useState<LoginMethod>("email");
  const [localIdentifier, setLocalIdentifier] = useState("");
  const [localName, setLocalName] = useState("");
  const [localPassword, setLocalPassword] = useState("");

  const [session, setSession] = useState<Session | null>(() => loadSession());
  const [authState, dispatchAuth] = useReducer(authReducer, {
    phase: session ? "authenticated" : "idle",
    message: session ? portalCopy.messages.restoredSession : portalCopy.messages.chooseLogin,
    requestId: 0
  });
  const [operationStatus, setOperationStatus] = useState<string | null>(null);
  const [statusUpdated, setStatusUpdated] = useState(false);
  const authRequestId = useRef(0);
  const checkoutNoticeHandled = useRef(false);
  const previousStatusMessage = useRef("");

  const [order, setOrder] = useState<Order | null>(null);
  const [paymentSession, setPaymentSession] = useState<PaymentSession | null>(null);
  const [cardKey, setCardKey] = useState("");
  const [entitlements, setEntitlements] = useState<Entitlement[]>([]);
  const [deviceName, setDeviceName] = useState(portalCopy.defaults.deviceName);
  const [deviceFingerprint, setDeviceFingerprint] = useState(() => defaultFingerprint());
  const [device, setDevice] = useState<Device | null>(null);
  const [lease, setLease] = useState<Lease | null>(null);
  const [downloads, setDownloads] = useState<Downloads | null>(null);
  const [localNode, setLocalNode] = useState<LocalNodeProbe>({
    phase: "idle",
    message: portalCopy.messages.localNodeIdle
  });

  const activeEntitlement = useMemo(
    () =>
      entitlements.find(
        (entitlement) => !entitlement.revoked_at && new Date(entitlement.expires_at).getTime() > Date.now()
      ) ?? null,
    [entitlements]
  );
  const authBusy = isAuthBusy(authState.phase);
  const statusMessage = authBusy ? authState.message : operationStatus ?? authState.message;
  const canUseProtectedActions = Boolean(session) && !authBusy;
  const captchaBlocked = isCaptchaProtectionMessage(authState.message);
  const directAuthNeedsTurnstile = SKYBRIDGE_TURNSTILE_CONFIGURED && !turnstileToken;
  const authDialogOpen = authDialogMode !== null;
  const localLeaseStatus = localNode.portal?.lease ?? null;
  const localPairingStatus = localNodePairingStatus(localNode, activeEntitlement, device, localLeaseStatus, Boolean(lease));
  const releaseManifest = downloads?.release_manifest;
  const localNodeMsiUrl = releaseManifest?.msi.url ?? "";
  const localNodeExeUrl = releaseManifest?.exe.url ?? "";
  const localNodeMacDmgUrl =
    releaseManifest?.macos_aarch64_dmg?.url ?? releaseManifest?.dmg?.url ?? downloads?.local_node_macos_dmg ?? "";
  const localPlatform = useMemo(() => detectLocalPlatform(), []);
  const localNodeDownloadOptions = useMemo(
    () => localNodeDownloads(localPlatform, localNodeMacDmgUrl, localNodeMsiUrl, localNodeExeUrl),
    [localPlatform, localNodeMacDmgUrl, localNodeMsiUrl, localNodeExeUrl]
  );
  const primaryDownloadOption = localNodeDownloadOptions[0] ?? null;
  const releaseVersion = releaseManifest?.version ?? portalCopy.defaults.releasePending;
  const releaseCommit = releaseManifest?.commit ? shortCommit(releaseManifest.commit) : portalCopy.defaults.releasePending;
  const releaseChecksum = releaseManifest ? releaseChecksumLabel(releaseManifest) : portalCopy.defaults.releaseChecksumPending;
  const openclawPluginUrl = downloads?.openclaw_plugin ? absolutePortalUrl(downloads.openclaw_plugin) : "";
  const localManifestUrl = localNode.portal?.manifest_url ?? `${LOCAL_NODE_API}/openclaw/manifest`;
  const canOpenLocalConsole = Boolean(LOCAL_CONSOLE_URL);
  const canCopyLocalManifest = localNode.phase === "online";
  const canDownloadOpenClawPlugin = Boolean(openclawPluginUrl);
  const canBindLocalDevice = canUseProtectedActions && localNode.phase === "online" && Boolean(localNode.portal?.device_fingerprint);
  const computerHelperOnline = localNode.phase === "online";
  const computerAuthorized = localLeaseStatus?.valid === true;
  const storeCredentialsReady = localNode.portal?.ozon?.configured === true;
  const posterConfigReady = localNode.portal?.openai?.configured === true;
  const directAuthUnavailableMessage = portalCopy.messages.directAuthUnavailable;
  const directAuthContext = SKYBRIDGE_PHONE_AUTH_ENABLED
    ? portalCopy.auth.contextDirect
    : portalCopy.auth.contextDirectEmailOnly;
  const directAuthText = SKYBRIDGE_PHONE_AUTH_ENABLED ? portalCopy.auth.directText : portalCopy.auth.directTextEmailOnly;
  const directAuthTitle =
    authMode === "register"
      ? SKYBRIDGE_PHONE_AUTH_ENABLED
        ? portalCopy.auth.directTitleRegister
        : portalCopy.auth.directTitleRegisterEmailOnly
      : SKYBRIDGE_PHONE_AUTH_ENABLED
        ? portalCopy.auth.directTitleLogin
        : portalCopy.auth.directTitleLoginEmailOnly;
  const heroMeta = SKYBRIDGE_PHONE_AUTH_ENABLED ? portalCopy.hero.meta : portalCopy.hero.emailOnlyMeta;
  const authSubmitText =
    authMethod === "phone"
      ? authMode === "register"
        ? portalCopy.auth.submit.phoneRegister
        : portalCopy.auth.submit.phoneLogin
      : authMethod === "nebula"
        ? portalCopy.auth.submit.nebulaLogin
      : authMode === "register"
        ? portalCopy.auth.submit.emailRegister
        : portalCopy.auth.submit.emailLogin;
  const authDialogTitle = authMode === "register" ? portalCopy.auth.titleRegister : portalCopy.auth.titleLogin;
  const authDialogDescription =
    authMode === "register"
      ? portalCopy.auth.descriptionRegister
      : portalCopy.auth.descriptionLogin;
  const shouldShowAuthDialogStatus =
    authBusy || authState.phase === "failed" || authState.phase === "authenticated" || Boolean(operationStatus);
  const statusTone = statusMessageTone(statusMessage, authState.phase, authBusy);
  const StatusLineIcon = authBusy ? RefreshCcw : statusTone === "danger" ? AlertCircle : CheckCircle2;
  const setupStatus = setupStatusModel({
    activeEntitlement,
    computerAuthorized,
    device,
    localLeaseIssue: localLeaseStatus?.issue ?? null,
    localNode,
    order,
    storeCredentialsReady
  });
  const canStartWorkspace = computerHelperOnline && computerAuthorized && canOpenLocalConsole;
  const readyForWorkspace = computerHelperOnline && computerAuthorized;
  const currentSetupStep = setupStepNumber({
    activeEntitlement,
    computerAuthorized,
    computerHelperOnline,
    storeCredentialsReady
  });

  useEffect(() => {
    if (previousStatusMessage.current === statusMessage) return;
    previousStatusMessage.current = statusMessage;
    setStatusUpdated(false);
    const frameId = window.requestAnimationFrame(() => setStatusUpdated(true));
    const timeoutId = window.setTimeout(() => setStatusUpdated(false), 520);
    return () => {
      window.cancelAnimationFrame(frameId);
      window.clearTimeout(timeoutId);
    };
  }, [statusMessage]);

  useEffect(() => {
    const revealElements = Array.from(document.querySelectorAll<HTMLElement>("[data-reveal]"));
    if (revealElements.length === 0) return;
    if (!("IntersectionObserver" in window)) {
      revealElements.forEach((element) => element.classList.add("is-visible"));
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((entry) => {
          if (!entry.isIntersecting) return;
          entry.target.classList.add("is-visible");
          observer.unobserve(entry.target);
        });
      },
      { rootMargin: "0px 0px -12% 0px", threshold: 0.14 }
    );
    revealElements.forEach((element) => observer.observe(element));
    return () => observer.disconnect();
  }, [session]);

  async function api<T>(path: string, init: RequestInit = {}, token = session?.token): Promise<T> {
    const response = await fetchWithTimeout(`${API_BASE}${path}`, {
      ...init,
      headers: {
        "Content-Type": "application/json",
        ...(token ? { Authorization: `Bearer ${token}` } : {}),
        ...(init.headers ?? {})
      }
    });
    return parseResponse<T>(response);
  }

  async function fetchDownloads() {
    const response = await fetchWithTimeout(`${API_BASE}/downloads`);
    return parseResponse<Downloads>(response);
  }

  function beginAuth(phase: AuthPhase, message: string) {
    const requestId = authRequestId.current + 1;
    authRequestId.current = requestId;
    setOperationStatus(null);
    dispatchAuth({ type: "begin", phase, message, requestId });
    return requestId;
  }

  function isCurrentAuth(requestId: number) {
    return authRequestId.current === requestId;
  }

  function commitSession(nextSession: Session) {
    localStorage.setItem(SESSION_KEY, JSON.stringify(nextSession));
    setSession(nextSession);
    setAuthDialogMode(null);
  }

  function resetSessionState() {
    localStorage.removeItem(SESSION_KEY);
    setSession(null);
    setEntitlements([]);
    setOrder(null);
    setPaymentSession(null);
    setDevice(null);
    setLease(null);
  }

  function expireSession(requestId: number) {
    resetSessionState();
    setAuthMode("login");
    setAuthDialogMode("login");
    setOperationStatus(null);
    dispatchAuth({ type: "failure", message: portalCopy.messages.sessionExpired, requestId });
  }

  function openAuthDialog(mode: AuthMode) {
    setAuthMode(mode);
    setAuthDialogMode(mode);
    setOperationStatus(null);
    if (mode === "register" && authMethod === "nebula") {
      setAuthMethod("email");
    }
  }

  function switchAuthMode(mode: AuthMode) {
    setAuthMode(mode);
    setAuthDialogMode(mode);
    if (mode === "register" && authMethod === "nebula") {
      setAuthMethod("email");
    }
  }

  function resetTurnstileWidget() {
    if (turnstileWidgetId.current && window.turnstile) {
      window.turnstile.reset(turnstileWidgetId.current);
    }
    turnstileTokenRef.current = "";
    setTurnstileStatus(SKYBRIDGE_TURNSTILE_CONFIGURED ? portalCopy.messages.turnstileWaiting : portalCopy.messages.turnstileNotRequired);
  }

  async function startNebulaOAuth(flow: NebulaOAuthFlow) {
    const requestId = beginAuth(
      "authenticating_skybridge",
      portalCopy.messages.openingUnified(flow)
    );
    try {
      await redirectToNebulaOAuth(flow);
    } catch (error) {
      if (isCurrentAuth(requestId)) {
        dispatchAuth({
          type: "failure",
          message: portalCopy.messages.unifiedStartFailed(errorMessage(error)),
          requestId
        });
      }
    }
  }

  async function completeNebulaOAuthCallback() {
    const currentUrl = new URL(window.location.href);
    const storedSession = readNebulaOAuthSession();
    const isCallbackRoute = currentUrl.pathname === "/auth/callback";

    if (!storedSession && !isCallbackRoute) {
      return false;
    }

    const requestId = beginAuth("creating_service_session", portalCopy.messages.completingUnifiedCallback);
    const authError = currentUrl.searchParams.get("error_description") ?? currentUrl.searchParams.get("error");
    if (!storedSession) {
      replaceLocationPath("/");
      dispatchAuth({
        type: "failure",
        message: portalCopy.messages.unifiedContextExpired,
        requestId
      });
      return true;
    }

    if (authError) {
      clearNebulaOAuthSession();
      replaceLocationPath("/");
      dispatchAuth({
        type: "failure",
        message: portalCopy.messages.unifiedAuthFailed(authError),
        requestId
      });
      return true;
    }

    const code = currentUrl.searchParams.get("code");
    const state = currentUrl.searchParams.get("state");
    if (!code || !state) {
      clearNebulaOAuthSession();
      replaceLocationPath("/");
      dispatchAuth({
        type: "failure",
        message: portalCopy.messages.unifiedCallbackMissing,
        requestId
      });
      return true;
    }

    if (state !== storedSession.state) {
      clearNebulaOAuthSession();
      replaceLocationPath("/");
      dispatchAuth({
        type: "failure",
        message: portalCopy.messages.unifiedStateFailed,
        requestId
      });
      return true;
    }

    try {
      const nebulaSession = await exchangeNebulaOAuthCode(code, storedSession);
      clearNebulaOAuthSession();
      replaceLocationPath("/");
      await createSkybridgeServiceSession(nebulaSession.access_token, requestId);
    } catch (error) {
      clearNebulaOAuthSession();
      replaceLocationPath("/");
      if (isCurrentAuth(requestId)) {
        dispatchAuth({
          type: "failure",
          message: portalCopy.messages.unifiedLoginFailed(errorMessage(error)),
          requestId
        });
      }
    }
    return true;
  }

  async function authenticateWithSkybridge() {
    if (!SKYBRIDGE_AUTH_CONFIGURED) {
      const requestId = beginAuth("failed", directAuthUnavailableMessage);
      dispatchAuth({
        type: "failure",
        message: directAuthUnavailableMessage,
        requestId
      });
      return;
    }
    if (authMethod === "phone" && !SKYBRIDGE_PHONE_AUTH_ENABLED) {
      const message = portalCopy.auth.phoneAuthUnavailable;
      const requestId = beginAuth("failed", message);
      dispatchAuth({
        type: "failure",
        message,
        requestId
      });
      return;
    }
    if (!skybridgeIdentifier.trim() || (authMethod === "phone" ? !skybridgePhoneCode.trim() : !skybridgePassword)) {
      const message =
        authMethod === "phone"
          ? portalCopy.messages.fillPhoneAndCode
          : portalCopy.messages.fillMethodAndPassword(methodLabel(authMethod));
      const requestId = beginAuth("failed", message);
      dispatchAuth({
        type: "failure",
        message,
        requestId
      });
      return;
    }

    const requestId = beginAuth(
      "authenticating_skybridge",
      portalCopy.messages.authenticatingMethod(authMode, methodLabel(authMethod))
    );
    try {
      const normalizedIdentifier =
        authMethod === "phone" ? normalizePhoneForSkybridge(skybridgeIdentifier) : skybridgeIdentifier.trim();
      if (authMethod === "phone" && !isValidSkybridgePhone(normalizedIdentifier)) {
        const message = portalCopy.messages.invalidPhone;
        dispatchAuth({ type: "failure", message, requestId });
        return;
      }

      if (authMode === "register" && authMethod === "phone") {
        if (!skybridgeName.trim() || !skybridgePhoneEmail.trim()) {
          const message = portalCopy.messages.phoneRegisterNeedsProfile;
          dispatchAuth({ type: "failure", message, requestId });
          return;
        }
      }

      const skybridgeSession = await skybridgePasswordAuth({
        mode: authMode,
        method: authMethod,
        identifier: normalizedIdentifier,
        password: skybridgePassword,
        phoneCode: skybridgePhoneCode,
        name: skybridgeName,
        captchaToken: turnstileToken
      });
      if (authMode === "register" && authMethod === "phone") {
        await skybridgeUpdatePhoneRegistrationProfile({
          accessToken: skybridgeSession.access_token,
          phone: normalizedIdentifier,
          email: skybridgePhoneEmail,
          name: skybridgeName
        });
      }
      if (!isCurrentAuth(requestId)) return;
      await createSkybridgeServiceSession(skybridgeSession.access_token, requestId);
      if (isCurrentAuth(requestId)) {
        turnstileTokenRef.current = "";
        setSkybridgePassword("");
        setSkybridgePhoneCode("");
        setSkybridgePhoneEmail("");
        setTurnstileToken("");
        resetTurnstileWidget();
      }
    } catch (error) {
      if (isCurrentAuth(requestId)) {
        turnstileTokenRef.current = "";
        setSkybridgePassword("");
        setSkybridgePhoneCode("");
        setSkybridgePhoneEmail("");
        setTurnstileToken("");
        resetTurnstileWidget();
        dispatchAuth({
          type: "failure",
          message: portalCopy.messages.accountLoginFailed(skybridgeDirectAuthFailureMessage(error)),
          requestId
        });
      }
    }
  }

  function handleSkybridgePasswordSubmit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (authBusy) {
      return;
    }
    authenticateWithSkybridge();
  }

  async function sendSkybridgePhoneCode() {
    if (!SKYBRIDGE_PHONE_AUTH_ENABLED) {
      setSkybridgeOtpStatus(portalCopy.auth.phoneAuthUnavailable);
      return;
    }
    if (!SKYBRIDGE_AUTH_CONFIGURED) {
      setSkybridgeOtpStatus(directAuthUnavailableMessage);
      return;
    }
    if (!skybridgeIdentifier.trim()) {
      setSkybridgeOtpStatus(portalCopy.messages.fillPhoneFirst);
      return;
    }
    const normalizedPhone = normalizePhoneForSkybridge(skybridgeIdentifier);
    if (!isValidSkybridgePhone(normalizedPhone)) {
      setSkybridgeOtpStatus(portalCopy.messages.invalidPhone);
      return;
    }
    if (authMode === "register" && (!skybridgeName.trim() || !skybridgePhoneEmail.trim())) {
      setSkybridgeOtpStatus(portalCopy.messages.phoneRegisterNeedsProfileBeforeCode);
      return;
    }
    if (directAuthNeedsTurnstile) {
      setSkybridgeOtpStatus(portalCopy.messages.completeSecurityBeforeCode);
      return;
    }

    setSkybridgeOtpBusy(true);
    setSkybridgeOtpStatus(portalCopy.messages.requestingSmsCode);
    try {
      await skybridgeSendPhoneOtp({
        phone: normalizedPhone,
        mode: authMode,
        name: skybridgeName,
        captchaToken: turnstileToken
      });
      setSkybridgeOtpStatus(portalCopy.messages.smsCodeSent);
    } catch (error) {
      setSkybridgeOtpStatus(portalCopy.messages.smsCodeFailed(skybridgeDirectAuthFailureMessage(error)));
    } finally {
      setSkybridgeOtpBusy(false);
    }
  }

  async function createManualSkybridgeServiceSession() {
    if (!skybridgeAccessToken.trim()) {
      const requestId = beginAuth("failed", portalCopy.messages.pasteSessionToken);
      dispatchAuth({ type: "failure", message: portalCopy.messages.pasteSessionToken, requestId });
      return;
    }
    const requestId = beginAuth("creating_service_session", portalCopy.messages.creatingFromIdentityToken);
    try {
      await createSkybridgeServiceSession(skybridgeAccessToken.trim(), requestId);
      if (isCurrentAuth(requestId)) {
        setSkybridgeAccessToken("");
      }
    } catch (error) {
      if (isCurrentAuth(requestId)) {
        dispatchAuth({
          type: "failure",
          message: portalCopy.messages.identityExchangeFailed(errorMessage(error)),
          requestId
        });
      }
    }
  }

  async function createSkybridgeServiceSession(accessToken: string, requestId: number) {
    dispatchAuth({
      type: "begin",
      phase: "creating_service_session",
      message: portalCopy.messages.creatingServiceSession,
      requestId
    });
    const data = await api<{ token: string; user: User }>("/auth/skybridge", {
      method: "POST",
      body: JSON.stringify({ access_token: accessToken })
    });
    if (!isCurrentAuth(requestId)) return;
    const nextSession = { token: data.token, user: data.user };
    commitSession(nextSession);
    dispatchAuth({
      type: "success",
      message: portalCopy.messages.accountLoggedIn(displayLoginAlias(data.user)),
      requestId
    });
    await refreshAccount(nextSession.token, requestId);
  }

  async function authenticateLocalDev() {
    const requestId = beginAuth("authenticating_local_dev", portalCopy.messages.authenticatingLocalDev);
    const path = localMode === "register" ? "/auth/register" : "/auth/login";
    const body = buildLocalAuthBody(localMode, localMethod, localIdentifier, localPassword, localName);
    try {
      const data = await api<{ token: string; user: User }>(path, {
        method: "POST",
        body: JSON.stringify(body)
      });
      if (!isCurrentAuth(requestId)) return;
      const nextSession = { token: data.token, user: data.user };
      commitSession(nextSession);
      dispatchAuth({
        type: "success",
        message: portalCopy.messages.localDevSessionCreated(data.user.nebula_id),
        requestId
      });
      await refreshAccount(nextSession.token, requestId);
    } catch (error) {
      if (isCurrentAuth(requestId)) {
        dispatchAuth({
          type: "failure",
          message: portalCopy.messages.localDevFailed(errorMessage(error)),
          requestId
        });
      }
    }
  }

  async function refreshAccount(token = session?.token, requestId?: number) {
    if (!token) {
      const nextRequestId = beginAuth("failed", portalCopy.messages.loginRequired);
      dispatchAuth({ type: "failure", message: portalCopy.messages.loginRequired, requestId: nextRequestId });
      return;
    }
    const currentRequestId = requestId ?? beginAuth("refreshing", portalCopy.messages.refreshingAccount);
    if (requestId) {
      dispatchAuth({
        type: "begin",
        phase: "refreshing",
        message: portalCopy.messages.refreshingAccount,
        requestId
      });
    }
    try {
      const [me, downloadData] = await Promise.all([
        fetchWithTimeout(`${API_BASE}/me`, {
          headers: { Authorization: `Bearer ${token}` }
        }).then(async (response) => parseResponse<{ user: User; entitlements: Entitlement[] }>(response)),
        fetchDownloads().catch(() => null)
      ]);
      if (!isCurrentAuth(currentRequestId)) return;
      setSession((current) => (current ? { ...current, user: me.user } : { token, user: me.user }));
      setEntitlements(me.entitlements);
      if (downloadData) {
        setDownloads(downloadData);
      }
      dispatchAuth({ type: "success", message: portalCopy.messages.accountRefreshed, requestId: currentRequestId });
    } catch (error) {
      if (isCurrentAuth(currentRequestId)) {
        if (isInvalidSessionError(error)) {
          expireSession(currentRequestId);
          return;
        }
        dispatchAuth({
          type: "failure",
          message: portalCopy.messages.accountRefreshFailed(errorMessage(error)),
          requestId: currentRequestId
        });
      }
    }
  }

  function logout() {
    const requestId = authRequestId.current + 1;
    authRequestId.current = requestId;
    resetSessionState();
    setOperationStatus(null);
    dispatchAuth({ type: "signed_out", message: portalCopy.messages.signedOut, requestId });
  }

  async function createOrder() {
    if (!session) {
      setOperationStatus(portalCopy.messages.loginRequired);
      openAuthDialog("login");
      return;
    }
    try {
      const data = await api<OrderApiResponse>("/orders", {
        method: "POST",
        body: JSON.stringify({ plan_code: "standard_30d" })
      });
      setOrder(data.order);
      setPaymentSession(data.payment ?? null);
      if (data.payment?.checkout_url) {
        setOperationStatus(portalCopy.messages.orderCreatedOpeningPayment);
        window.location.assign(data.payment.checkout_url);
        return;
      }
      setOperationStatus(data.payment?.message ?? portalCopy.messages.orderCreatedManual);
    } catch (error) {
      setOperationStatus(portalCopy.messages.createOrderFailed(errorMessage(error)));
    }
  }

  async function copyOrderInfo() {
    if (!order) {
      setOperationStatus(portalCopy.messages.noOrderToCopy);
      return;
    }
    try {
      await copyText(
        `${portalCopy.console.orderDetails.id}: ${order.id}\n${portalCopy.console.orderDetails.provider}: ${paymentProviderLabel(order.payment_provider)}\n${portalCopy.console.orderDetails.paymentReference}: ${order.payment_reference}`
      );
      setOperationStatus(portalCopy.messages.orderInfoCopied);
    } catch {
      setOperationStatus(portalCopy.messages.copyFailed);
    }
  }

  async function refreshOrder() {
    if (!order) {
      setOperationStatus(portalCopy.messages.noOrderToRefresh);
      return;
    }
    try {
      const data = await api<OrderApiResponse>(`/orders/${order.id}`);
      setOrder(data.order);
      setPaymentSession(data.payment ?? null);
      setOperationStatus(
        data.order.status === "confirmed"
          ? confirmedOrderMessage(data.order)
          : portalCopy.messages.orderStatusRefreshed(orderStatusLabel(data.order.status))
      );
    } catch (error) {
      setOperationStatus(portalCopy.messages.refreshOrderFailed(errorMessage(error)));
    }
  }

  async function redeem() {
    if (!session || !cardKey.trim()) {
      setOperationStatus(portalCopy.messages.redeemNeedsLoginAndCode);
      return;
    }
    try {
      await api<{ entitlement: Entitlement }>("/card-keys/redeem", {
        method: "POST",
        body: JSON.stringify({ card_key: cardKey.trim() })
      });
      setCardKey("");
      setOperationStatus(portalCopy.messages.cardRedeemed);
      await refreshAccount();
    } catch (error) {
      setOperationStatus(portalCopy.messages.redeemFailed(errorMessage(error)));
    }
  }

  async function activateDevice() {
    if (!session) {
      setOperationStatus(portalCopy.messages.loginRequired);
      openAuthDialog("login");
      return;
    }
    try {
      const data = await api<{ device: Device }>("/devices/activate", {
        method: "POST",
        body: JSON.stringify({ name: deviceName, fingerprint: deviceFingerprint })
      });
      setDevice(data.device);
      setLease(null);
      setOperationStatus(portalCopy.messages.deviceActivated);
    } catch (error) {
      setOperationStatus(portalCopy.messages.deviceActivateFailed(errorMessage(error)));
    }
  }

  async function issueLease() {
    if (!session || !device) {
      setOperationStatus(portalCopy.messages.needsLoginAndDevice);
      return;
    }
    try {
      const data = await api<{ lease: Lease }>("/entitlements/lease", {
        method: "POST",
        body: JSON.stringify({ device_id: device.id })
      });
      try {
        const localLease = await localNodePost<PortalLeaseResponse>("/portal/lease", { lease: data.lease });
        setLease(data.lease);
        applyLocalLeaseStatus(localLease.lease);
        setOperationStatus(portalCopy.messages.computerAuthorized);
        await probeLocalNode();
      } catch (localError) {
        setLease(null);
        applyLocalLeaseIssue(errorMessage(localError));
        setOperationStatus(localLeaseWriteFailureMessage(localError));
      }
    } catch (error) {
      setOperationStatus(portalCopy.messages.computerAuthorizeFailed(errorMessage(error)));
    }
  }

  function applyLocalLeaseStatus(nextLease: LocalLeaseStatus) {
    setLocalNode((current) => {
      if (!current.portal) return current;
      return {
        ...current,
        portal: {
          ...current.portal,
          lease: nextLease
        }
      };
    });
  }

  function applyLocalLeaseIssue(issue: string) {
    setLocalNode((current) => {
      if (!current.portal) return current;
      return {
        ...current,
        portal: {
          ...current.portal,
          lease: {
            configured: current.portal.lease.configured,
            valid: false,
            lease_id: current.portal.lease.lease_id,
            device_id: current.portal.lease.device_id,
            features: current.portal.lease.features,
            expires_at: current.portal.lease.expires_at,
            issue
          }
        }
      };
    });
  }

  async function probeLocalNode() {
    setLocalNode({
      phase: "checking",
      message: portalCopy.messages.checkingLocalNode
    });
    try {
      const health = await localNodeJson<LocalNodeHealth>("/health").catch((error) => {
        throw new Error(`health: ${errorMessage(error)}`);
      });
      const manifest = await localNodeJson<LocalNodeManifest>("/openclaw/manifest").catch((error) => {
        throw new Error(`manifest: ${errorMessage(error)}`);
      });
      const portalResult = await localNodeJson<LocalPortalStatus>("/portal/status")
        .then((portal) => ({ ok: true as const, portal }))
        .catch((error) => ({ ok: false as const, error }));
      const portal = portalResult.ok ? portalResult.portal : null;
      if (portal?.device_fingerprint) {
        setDeviceFingerprint(portal.device_fingerprint);
      }
      setLocalNode({
        phase: "online",
        message: localNodeOnlineMessage(health, portalResult.ok ? null : portalResult.error),
        checkedAt: new Date().toISOString(),
        health,
        manifest,
        portal: portal ?? undefined
      });
    } catch (error) {
      const endpoint = localNodeFailedEndpoint(error);
      setLocalNode({
        phase: endpoint === "manifest" ? "degraded" : isLocalNodeBrowserBlock(error) ? "blocked" : "offline",
        message: localNodeFailureMessage(error, endpoint),
        checkedAt: new Date().toISOString()
      });
    }
  }

  async function copyLocalManifestUrl() {
    if (!canCopyLocalManifest) {
      setOperationStatus(portalCopy.messages.copyManifestAfterConnect);
      return;
    }
    await copyText(localManifestUrl);
    setOperationStatus(portalCopy.messages.manifestCopied);
  }

  useEffect(() => {
    let cancelled = false;
    async function bootPortal() {
      fetchDownloads()
        .then((downloadData) => {
          if (!cancelled) setDownloads(downloadData);
        })
        .catch(() => {
          // /downloads is the single package source; keep account flow usable when it is unavailable.
        });
      const handledCallback = await completeNebulaOAuthCallback();
      if (cancelled || handledCallback) return;

      const restoredSession = loadSession();
      if (restoredSession?.token) {
        const requestId = beginAuth("refreshing", portalCopy.messages.restoringAccount);
        refreshAccount(restoredSession.token, requestId);
      }
    }
    bootPortal();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (session) {
      probeLocalNode();
    } else {
      setLocalNode({
        phase: "idle",
        message: portalCopy.messages.localNodeIdle
      });
    }
  }, [session?.token]);

  useEffect(() => {
    if (checkoutNoticeHandled.current) return;
    const checkout = new URL(window.location.href).searchParams.get("checkout");
    if (!checkout) return;
    checkoutNoticeHandled.current = true;
    if (checkout === "success") {
      setOperationStatus(portalCopy.messages.checkoutSuccess);
      if (session?.token) {
        refreshAccount();
      }
    } else if (checkout === "cancelled") {
      setOperationStatus(portalCopy.messages.checkoutCancelled);
    }
  }, [session?.token]);

  useEffect(() => {
    if (authMode === "register" && authMethod === "nebula") {
      setAuthMethod("email");
    }
    if (!SKYBRIDGE_PHONE_AUTH_ENABLED && authMethod === "phone") {
      setAuthMethod("email");
    }
  }, [authMode, authMethod]);

  useEffect(() => {
    if (!DIRECT_SKYBRIDGE_AUTH_ENABLED || !SKYBRIDGE_TURNSTILE_CONFIGURED || !authDialogOpen) {
      return;
    }

    let cancelled = false;
    turnstileTokenRef.current = "";
    setTurnstileToken("");
    setTurnstileStatus(portalCopy.messages.loadingTurnstile);
    loadTurnstileScript()
      .then(() => {
        if (cancelled || !turnstileContainerRef.current || !window.turnstile || turnstileWidgetId.current) {
          return;
        }
        try {
          turnstileWidgetId.current = window.turnstile.render(turnstileContainerRef.current, {
            sitekey: SKYBRIDGE_TURNSTILE_SITE_KEY,
            action: "auth",
            callback: (token) => {
              turnstileTokenRef.current = token;
              setTurnstileToken(token);
              setTurnstileStatus(portalCopy.messages.turnstilePassed);
            },
            "expired-callback": () => {
              turnstileTokenRef.current = "";
              setTurnstileToken("");
              setTurnstileStatus(portalCopy.messages.turnstileExpired);
            },
            "error-callback": (error) => {
              turnstileTokenRef.current = "";
              setTurnstileToken("");
              setTurnstileStatus(turnstileFailureMessage(error));
              return true;
            }
          });
        } catch (error) {
          setTurnstileStatus(portalCopy.messages.turnstileRenderFailed(errorMessage(error)));
          return;
        }
        setTurnstileStatus(portalCopy.messages.turnstileLoaded);
        window.setTimeout(() => {
          if (!cancelled && !turnstileTokenRef.current) {
            setTurnstileStatus(portalCopy.messages.turnstileHiddenHint);
          }
        }, 4_000);
      })
      .catch((error) => {
        if (!cancelled) {
          setTurnstileStatus(portalCopy.messages.turnstileLoadFailed(errorMessage(error)));
        }
      });

    return () => {
      cancelled = true;
      if (turnstileWidgetId.current && window.turnstile) {
        window.turnstile.remove(turnstileWidgetId.current);
        turnstileWidgetId.current = null;
      }
    };
  }, [authDialogOpen]);

  useEffect(() => {
    if (localMode === "register" && localMethod === "nebula") {
      setLocalMethod("email");
    }
  }, [localMode, localMethod]);

  useEffect(() => {
    if (!authDialogOpen) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setAuthDialogMode(null);
      }
    };
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      document.body.style.overflow = previousOverflow;
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [authDialogOpen]);

  return (
    <main className={session ? "motion-stage motion-stage-console" : "motion-stage"}>
      <header className="site-nav">
        <a className="brand-mark" href="#top" aria-label={copy.common.brand}>
          <span className="brand-icon">
            <Orbit size={18} />
          </span>
          <span>{copy.common.brand}</span>
        </a>
        <nav aria-label={portalCopy.nav.primaryAria}>
          {session ? (
            <>
              <a href="#console">{portalCopy.nav.loggedIn.guide}</a>
              <a href="/customer-guide.html">{portalCopy.nav.loggedIn.manual}</a>
              <a href="#advanced">{portalCopy.nav.loggedIn.troubleshoot}</a>
            </>
          ) : (
            <>
              <a href="#capabilities">{portalCopy.nav.public.capabilities}</a>
              <a href="#workflow">{portalCopy.nav.public.workflow}</a>
              <a href="#pricing">{portalCopy.nav.public.pricing}</a>
              <a href="/customer-guide.html">{portalCopy.nav.public.manual}</a>
            </>
          )}
        </nav>
        <div className="nav-actions">
          <LanguageSwitch locale={locale} setLocale={setLocale} />
          {session ? (
            <>
              <button className="quiet" disabled={authBusy} onClick={() => refreshAccount()}>
                <RefreshCcw size={18} /> {portalCopy.nav.actions.refresh}
              </button>
              <button className="quiet" onClick={logout}>
                <LogOut size={18} /> {portalCopy.nav.actions.logout}
              </button>
            </>
          ) : (
            <>
              <button className="quiet" onClick={() => openAuthDialog("login")}>
                <LogIn size={18} /> {portalCopy.nav.actions.login}
              </button>
              <button onClick={() => openAuthDialog("register")}>
                <UserPlus size={18} /> {portalCopy.nav.actions.register}
              </button>
            </>
          )}
        </div>
      </header>

      {!session && (
        <>
      <section className="hero-section motion-hero motion-stage-entry" id="top">
        <div className="hero-copy">
          <p className="eyebrow">{portalCopy.hero.eyebrow}</p>
          <h1 aria-label={`${portalCopy.hero.titleLine1}${portalCopy.hero.titleLine2}`}>
            <span>{portalCopy.hero.titleLine1}</span>
            <span>{portalCopy.hero.titleLine2}</span>
          </h1>
          <p>{portalCopy.hero.description}</p>
          <div className="hero-actions">
            {session ? (
              <>
                <a className="download" href="#console">
                  <ArrowRight size={18} /> {portalCopy.hero.continueSetup}
                </a>
                {canOpenLocalConsole && (
                  <a className="download secondary" href={LOCAL_CONSOLE_URL} target="_blank" rel="noreferrer">
                    <MonitorCheck size={18} /> {portalCopy.hero.openWorkspace}
                  </a>
                )}
                <button className="secondary" disabled={authBusy} onClick={() => refreshAccount()}>
                  <RefreshCcw size={18} /> {portalCopy.hero.refreshStatus}
                </button>
              </>
            ) : (
              <>
                <button onClick={() => openAuthDialog("login")}>
                  <LogIn size={18} /> {portalCopy.hero.loginWorkspace}
                </button>
                <button className="secondary" onClick={() => openAuthDialog("register")}>
                  <UserPlus size={18} /> {portalCopy.hero.createAccount}
                </button>
              </>
            )}
          </div>
          <div className="hero-meta">
            {heroMeta.map((item) => (
              <span key={item}>{item}</span>
            ))}
          </div>
        </div>
        <div className="hero-visual motion-card-flow" aria-label={portalCopy.hero.visualAria}>
          <div className="showcase-toolbar">
            <span>{portalCopy.hero.visual.toolbarLabel}</span>
            <strong>{portalCopy.hero.visual.toolbarItem}</strong>
          </div>
          <div className="poster-stage">
            <article className="poster-card product-card">
              <span className="poster-label">{portalCopy.hero.visual.sourceLabel}</span>
              <div className="product-photo">
                <span className="lighter-shape" />
              </div>
              <strong>{portalCopy.hero.visual.sourceTitle}</strong>
              <p>{portalCopy.hero.visual.sourceBody}</p>
            </article>
            <article className="poster-card output-card">
              <span className="poster-label">{portalCopy.hero.visual.outputLabel}</span>
              <div className="poster-art">
                <span className="poster-product" />
                <span className="poster-road" />
              </div>
              <strong>{portalCopy.hero.visual.outputTitle}</strong>
              <p>{portalCopy.hero.visual.outputBody}</p>
            </article>
            <article className="poster-card brief-card">
              <span className="poster-label">{portalCopy.hero.visual.briefLabel}</span>
              <ul>
                {portalCopy.hero.visual.briefItems.map((item) => (
                  <li key={item}>{item}</li>
                ))}
              </ul>
            </article>
          </div>
          <div className="template-marquee" aria-hidden="true">
            <div>
              {[...portalCopy.hero.visual.marquee, ...portalCopy.hero.visual.marquee].map((item, index) => (
                <span key={`${item}-${index}`}>{item}</span>
              ))}
            </div>
          </div>
        </div>
      </section>

      <section className="capability-band motion-reveal" data-reveal id="capabilities">
        <div className="band-title">
          <p className="eyebrow">{portalCopy.capabilities.eyebrow}</p>
          <h2>{portalCopy.capabilities.title}</h2>
        </div>
        <div className="capability-grid">
          {portalCopy.capabilities.cards.map((card, index) => {
            const Icon = index === 0 ? PackageCheck : index === 1 ? Boxes : Bot;
            return (
              <article key={card.title}>
                <Icon />
                <h3>{card.title}</h3>
                <p>{card.text}</p>
              </article>
            );
          })}
        </div>
      </section>

      <section className="workflow-band motion-reveal" data-reveal id="workflow">
        <div className="workflow-copy">
          <p className="eyebrow">{portalCopy.workflow.eyebrow}</p>
          <h2>{portalCopy.workflow.title}</h2>
          <p>{portalCopy.workflow.text}</p>
        </div>
        <div className="workflow-steps">
          {portalCopy.workflow.steps.map((step, index) => (
            <div key={step.title}>
              <span>{String(index + 1).padStart(2, "0")}</span>
              <strong>{step.title}</strong>
              <p>{step.text}</p>
            </div>
          ))}
        </div>
      </section>
        </>
      )}

      {session && (
        <section className="account-console account-console-simple" id="console">
          <aside className="identity-panel">
            <p className="eyebrow">{portalCopy.console.identity.eyebrow}</p>
            <h2>{displayLoginAlias(session.user)}</h2>
            <div className="rail-status">
              <span>{portalCopy.console.identity.serviceStatus}</span>
              <strong>
                {activeEntitlement
                  ? portalCopy.console.identity.serviceOpen
                  : order
                    ? portalCopy.console.identity.servicePending
                    : portalCopy.console.identity.serviceClosed}
              </strong>
            </div>
            <div className="rail-status">
              <span>{portalCopy.console.identity.helper}</span>
              <strong>{computerHelperOnline ? portalCopy.console.identity.helperConnected : portalCopy.console.identity.helperDisconnected}</strong>
            </div>
            <div className="rail-status">
              <span>{portalCopy.console.identity.computerAuth}</span>
              <strong>{computerAuthorized ? portalCopy.console.identity.authComplete : portalCopy.console.identity.authIncomplete}</strong>
            </div>
            <div className="rail-status">
              <span>{portalCopy.console.identity.storeAuth}</span>
              <strong>
                {storeCredentialsReady
                  ? portalCopy.console.identity.saved
                  : readyForWorkspace
                    ? portalCopy.console.identity.pendingFill
                    : portalCopy.console.identity.notStarted}
              </strong>
            </div>
          </aside>

          <section className="workspace">
            {LOCAL_DEV_AUTH_ENABLED && (
            <details className="local-dev-panel">
              <summary>{portalCopy.console.localDev.summary}</summary>
              <div className="auth-strip local-dev-strip">
                <div className="section-title">
                  <Orbit />
                  <div>
                    <h2>{portalCopy.console.localDev.tokenTitle}</h2>
                    <p>{portalCopy.console.localDev.tokenText}</p>
                  </div>
                </div>
              </div>
              <div className="form-grid skybridge-grid">
                <label>
                  {portalCopy.console.localDev.tokenTitle}
                  <input
                    autoComplete="off"
                    placeholder={portalCopy.console.localDev.tokenPlaceholder}
                    type="password"
                    value={skybridgeAccessToken}
                    onChange={(event) => setSkybridgeAccessToken(event.target.value)}
                  />
                </label>
                <button disabled={authBusy} onClick={createManualSkybridgeServiceSession}>
                  <Orbit size={18} /> {portalCopy.console.localDev.createServiceSession}
                </button>
              </div>

              <div className="auth-strip local-dev-strip">
                <div className="section-title">
                  <ShieldCheck />
                  <div>
                    <h2>{portalCopy.console.localDev.localAccountTitle}</h2>
                    <p>{portalCopy.console.localDev.localAccountText}</p>
                  </div>
                </div>
                <div className="mode-switch" aria-label={portalCopy.console.localDev.modeAria}>
                  <button className={localMode === "register" ? "active" : ""} onClick={() => setLocalMode("register")}>
                    <UserPlus size={18} /> {portalCopy.console.localDev.register}
                  </button>
                  <button className={localMode === "login" ? "active" : ""} onClick={() => setLocalMode("login")}>
                    <LogIn size={18} /> {portalCopy.console.localDev.login}
                  </button>
                </div>
              </div>

              <div className="method-switch" aria-label={portalCopy.console.localDev.methodAria}>
                <button className={localMethod === "email" ? "active" : ""} onClick={() => setLocalMethod("email")}>
                  <Mail size={18} /> {portalCopy.console.localDev.email}
                </button>
                <button className={localMethod === "phone" ? "active" : ""} onClick={() => setLocalMethod("phone")}>
                  <Smartphone size={18} /> {portalCopy.console.localDev.phone}
                </button>
                {localMode === "login" && (
                  <button className={localMethod === "nebula" ? "active" : ""} onClick={() => setLocalMethod("nebula")}>
                    <KeyRound size={18} /> {portalCopy.console.localDev.nebula}
                  </button>
                )}
              </div>

              <div className="form-grid auth-grid">
                <label>
                  {identifierLabel(localMode, localMethod)}
                  <input
                    autoComplete={identifierAutocomplete(localMethod)}
                    placeholder={identifierPlaceholder(localMode, localMethod)}
                    value={localIdentifier}
                    onChange={(event) => setLocalIdentifier(event.target.value)}
                  />
                </label>
                {localMode === "register" && (
                  <label>
                    {portalCopy.console.localDev.name}
                    <input
                      autoComplete="name"
                      placeholder={portalCopy.console.localDev.namePlaceholder}
                      value={localName}
                      onChange={(event) => setLocalName(event.target.value)}
                    />
                  </label>
                )}
                <label>
                  {portalCopy.console.localDev.localPassword}
                  <input
                    autoComplete={localMode === "register" ? "new-password" : "current-password"}
                    placeholder={portalCopy.console.localDev.localPasswordPlaceholder}
                    value={localPassword}
                    onChange={(event) => setLocalPassword(event.target.value)}
                    type="password"
                  />
                </label>
                <button disabled={authBusy} onClick={authenticateLocalDev}>
                  {localMode === "register" ? <UserPlus size={18} /> : <LogIn size={18} />}
                  {localMode === "register" ? portalCopy.console.localDev.createLocalDev : portalCopy.console.localDev.loginLocalDev}
                </button>
              </div>
            </details>
            )}

          <div
            className={`status-line ${statusTone}-line ${authBusy ? "busy-line" : ""} ${statusUpdated ? "is-updated" : ""}`}
            role="status"
            aria-live="polite"
            aria-atomic="true"
            aria-busy={authBusy}
          >
            <StatusLineIcon size={18} /> {statusMessage}
          </div>

          <section className="operations setup-wizard motion-card-flow" data-current-step={currentSetupStep}>
            <div className={`setup-panel ${setupStatus.kind} motion-current-step`}>
              <div>
                <span>{portalCopy.console.setup.nextStep}</span>
                <h2>{setupStatus.title}</h2>
                <p>{setupStatus.message}</p>
              </div>
              <div className="setup-actions">
                {!activeEntitlement && order && (
                  <>
                    <button disabled={authBusy} onClick={refreshOrder}>
                      <RefreshCcw size={18} /> {portalCopy.console.setup.actions.refreshOrder}
                    </button>
                    <button className="secondary" onClick={copyOrderInfo}>
                      {portalCopy.console.setup.actions.copyOrder}
                    </button>
                  </>
                )}
                {!activeEntitlement && !order && (
                  <button disabled={!canUseProtectedActions} onClick={createOrder}>
                    <ArrowRight size={18} /> {portalCopy.console.setup.actions.openService}
                  </button>
                )}
                {activeEntitlement && !computerHelperOnline && primaryDownloadOption && (
                  <a className="download" href={primaryDownloadOption.url}>
                    <Download size={18} /> {primaryDownloadOption.shortLabel}
                  </a>
                )}
                {activeEntitlement && !computerHelperOnline && (
                  <button className="secondary" disabled={localNode.phase === "checking"} onClick={probeLocalNode}>
                    <RefreshCcw size={18} /> {portalCopy.console.setup.actions.openedProbe}
                  </button>
                )}
                {activeEntitlement && computerHelperOnline && !device && !computerAuthorized && (
                  <button disabled={!canBindLocalDevice} onClick={activateDevice}>
                    <MonitorCheck size={18} /> {portalCopy.console.setup.actions.authorizeComputer}
                  </button>
                )}
                {activeEntitlement && device && !computerAuthorized && (
                  <button onClick={issueLease} disabled={authBusy}>
                    <Radio size={18} /> {portalCopy.console.setup.actions.completeComputerAuth}
                  </button>
                )}
                {canStartWorkspace && (
                  <a className="download" href={LOCAL_CONSOLE_URL} target="_blank" rel="noreferrer">
                    <MonitorCheck size={18} /> {portalCopy.console.setup.actions.openWorkspace}
                  </a>
                )}
                {readyForWorkspace && !canOpenLocalConsole && (
                  <span className="inline-next-step">
                    <MonitorCheck size={18} /> {portalCopy.console.setup.actions.continueOnLocal}
                  </span>
                )}
              </div>
            </div>

            <div className="wizard-steps" aria-label={portalCopy.console.setup.stepsAria}>
              <article
                className={wizardStepClass(Boolean(activeEntitlement), currentSetupStep === 1)}
                aria-current={currentSetupStep === 1 ? "step" : undefined}
              >
                <span className="step-number">1</span>
                <div>
                  <h3>{portalCopy.console.setup.step1.title}</h3>
                  <p>
                    {activeEntitlement
                      ? portalCopy.console.setup.step1.done
                      : order
                        ? portalCopy.console.setup.step1.pending
                        : portalCopy.console.setup.step1.todo}
                  </p>
                  {!activeEntitlement && (
                    <div className="step-actions">
                      {order ? (
                        <>
                          <button disabled={authBusy} onClick={refreshOrder}>
                            <RefreshCcw size={18} /> {portalCopy.console.setup.actions.refreshOrder}
                          </button>
                          <button className="secondary" onClick={copyOrderInfo}>
                            {portalCopy.console.setup.actions.copyOrder}
                          </button>
                        </>
                      ) : (
                        <button disabled={!canUseProtectedActions} onClick={createOrder}>
                          <ArrowRight size={18} /> {portalCopy.console.setup.actions.openService}
                        </button>
                      )}
                    </div>
                  )}
                </div>
              </article>

              <article
                className={wizardStepClass(computerHelperOnline, currentSetupStep === 2)}
                aria-current={currentSetupStep === 2 ? "step" : undefined}
              >
                <span className="step-number">2</span>
                <div>
                  <h3>{portalCopy.console.setup.step2.title}</h3>
                  <p>
                    {computerHelperOnline
                      ? portalCopy.console.setup.step2.done
                      : activeEntitlement
                        ? portalCopy.console.setup.step2.todo
                        : portalCopy.console.setup.step2.locked}
                  </p>
                  {activeEntitlement && !computerHelperOnline && (
                    <div className="step-actions">
                      {primaryDownloadOption ? (
                        <a className="download" href={primaryDownloadOption.url}>
                          <Download size={18} /> {primaryDownloadOption.shortLabel}
                        </a>
                      ) : (
                        <button className="secondary" disabled>
                          <Download size={18} /> {portalCopy.console.setup.actions.packagePreparing}
                        </button>
                      )}
                      <button disabled={localNode.phase === "checking"} onClick={probeLocalNode}>
                        <RefreshCcw size={18} /> {portalCopy.console.setup.actions.openedProbe}
                      </button>
                    </div>
                  )}
                </div>
              </article>

              <article
                className={wizardStepClass(computerAuthorized, currentSetupStep === 3)}
                aria-current={currentSetupStep === 3 ? "step" : undefined}
              >
                <span className="step-number">3</span>
                <div>
                  <h3>{portalCopy.console.setup.step3.title}</h3>
                  <p>
                    {computerAuthorized
                      ? portalCopy.console.setup.step3.done
                      : portalCopy.console.setup.step3.todo}
                  </p>
                  {activeEntitlement && computerHelperOnline && !computerAuthorized && (
                    <div className="step-actions">
                      {!device ? (
                        <button disabled={!canBindLocalDevice} onClick={activateDevice}>
                          <MonitorCheck size={18} /> {portalCopy.console.setup.actions.authorizeComputer}
                        </button>
                      ) : (
                        <button disabled={authBusy} onClick={issueLease}>
                          <Radio size={18} /> {portalCopy.console.setup.actions.completeComputerAuth}
                        </button>
                      )}
                    </div>
                  )}
                </div>
              </article>

              <article
                className={wizardStepClass(storeCredentialsReady, currentSetupStep === 4)}
                aria-current={currentSetupStep === 4 ? "step" : undefined}
              >
                <span className="step-number">4</span>
                <div>
                  <h3>{storeCredentialsReady ? portalCopy.console.setup.step4.readyTitle : portalCopy.console.setup.step4.connectTitle}</h3>
                  <p>
                    {!readyForWorkspace
                      ? portalCopy.console.setup.step4.locked
                      : storeCredentialsReady
                        ? portalCopy.console.setup.step4.ready
                        : portalCopy.console.setup.step4.todo}
                  </p>
                  {readyForWorkspace && (
                    <div className="step-actions">
                      {canStartWorkspace ? (
                        <a className="download" href={LOCAL_CONSOLE_URL} target="_blank" rel="noreferrer">
                          <MonitorCheck size={18} /> {portalCopy.console.setup.actions.openWorkspace}
                        </a>
                      ) : (
                        <span className="inline-next-step">
                          <MonitorCheck size={18} /> {portalCopy.console.setup.actions.openLocalApp}
                        </span>
                      )}
                      <button className="secondary" disabled={localNode.phase === "checking"} onClick={probeLocalNode}>
                        <RefreshCcw size={18} /> {portalCopy.console.setup.actions.refreshAfterHandled}
                      </button>
                    </div>
                  )}
                </div>
              </article>
            </div>

            {readyForWorkspace && (
              <div className={`handoff-card ${storeCredentialsReady ? "online" : "warn"}`}>
                <div>
                  <span>{storeCredentialsReady ? portalCopy.console.setup.handoff.readyBadge : portalCopy.console.setup.handoff.todoBadge}</span>
                  <h3>{storeCredentialsReady ? portalCopy.console.setup.handoff.readyTitle : portalCopy.console.setup.handoff.todoTitle}</h3>
                  <p>
                    {storeCredentialsReady
                      ? portalCopy.console.setup.handoff.readyText
                      : portalCopy.console.setup.handoff.todoText}
                  </p>
                </div>
                <div className="handoff-actions">
                  <button className="secondary" disabled={localNode.phase === "checking"} onClick={probeLocalNode}>
                    <RefreshCcw size={18} /> {portalCopy.console.setup.handoff.refreshCheck}
                  </button>
                </div>
              </div>
            )}

            {paymentSession && (
              <div className="payment-note">
                <CheckCircle2 size={18} />
                <div className="payment-note-body">
                  <span>{paymentSession.message}</span>
                  {paymentSession.native_code_url && (
                    <div className="wechat-pay-box">
                      <QRCodeSVG value={paymentSession.native_code_url} size={148} marginSize={2} />
                      <div>
                        <strong>{portalCopy.console.payment.wechatTitle}</strong>
                        <p>
                          {portalCopy.console.payment.paymentReference}: {paymentSession.payment_reference} ·{" "}
                          {formatMoney(paymentSession.amount_minor, paymentSession.currency)}
                        </p>
                      </div>
                    </div>
                  )}
                  {paymentSession.checkout_url && (
                    <a href={paymentSession.checkout_url}>
                      <ExternalLink size={16} /> {portalCopy.console.payment.openCheckout}
                    </a>
                  )}
                </div>
              </div>
            )}

            <details className="op-section support-section">
              <summary>
                <KeyRound size={20} />
                <div>
                  <strong>{portalCopy.console.supportCode.summaryTitle}</strong>
                  <span>{portalCopy.console.supportCode.summaryText}</span>
                </div>
              </summary>
              <div className="form-grid">
                <label>
                  {portalCopy.console.supportCode.code}
                  <input value={cardKey} onChange={(event) => setCardKey(event.target.value)} placeholder="ORS-..." />
                </label>
                <label>
                  {portalCopy.console.supportCode.serviceStatus}
                  <input value={activeEntitlement ? portalCopy.console.identity.serviceOpen : portalCopy.console.identity.serviceClosed} readOnly />
                </label>
                <label>
                  {portalCopy.console.supportCode.expiry}
                  <input value={activeEntitlement?.expires_at ?? portalCopy.console.supportCode.pendingExpiry} readOnly />
                </label>
              </div>
              <div className="command-row">
                <button disabled={!canUseProtectedActions} onClick={redeem}>
                  <KeyRound size={18} /> {portalCopy.console.supportCode.redeem}
                </button>
              </div>
            </details>

            <details className="op-section support-section">
              <summary>
                <Clipboard size={20} />
                <div>
                  <strong>{portalCopy.console.orderDetails.summaryTitle}</strong>
                  <span>{portalCopy.console.orderDetails.summaryText}</span>
                </div>
              </summary>
              {order ? (
                <div className="form-grid">
                  <label>
                    {portalCopy.console.orderDetails.id}
                    <input value={order.id} readOnly />
                  </label>
                  <label>
                    {portalCopy.console.orderDetails.paymentReference}
                    <input value={order.payment_reference} readOnly />
                  </label>
                  <label>
                    {portalCopy.console.orderDetails.status}
                    <input value={orderStatusLabel(order.status)} readOnly />
                  </label>
                  <label>
                    {portalCopy.console.orderDetails.provider}
                    <input value={paymentProviderLabel(order.payment_provider)} readOnly />
                  </label>
                  <label>
                    {portalCopy.console.orderDetails.amount}
                    <input value={formatMoney(order.amount_minor, order.currency)} readOnly />
                  </label>
                </div>
              ) : (
                <p className="section-hint">{portalCopy.console.orderDetails.empty}</p>
              )}
              <div className="command-row">
                <button className="secondary" onClick={copyOrderInfo} disabled={!order}>
                  {portalCopy.console.orderDetails.copy}
                </button>
                <button className="secondary" onClick={refreshOrder} disabled={!order || authBusy}>
                  <RefreshCcw size={18} /> {portalCopy.console.orderDetails.refresh}
                </button>
              </div>
            </details>

            <details className="op-section support-section local-node-section" id="advanced">
              <summary>
                <MonitorCheck size={20} />
                <div>
                  <strong>{portalCopy.console.advanced.summaryTitle}</strong>
                  <span>{portalCopy.console.advanced.summaryText}</span>
                </div>
              </summary>
              <div className="local-node-grid">
                <div className={`local-node-card ${localNode.phase}`}>
                  <span>{portalCopy.console.advanced.helper}</span>
                  <strong>{localNodeStatusLabel(localNode.phase)}</strong>
                  <p>{localNode.message}</p>
                </div>
                <div className={`local-node-card ${localPairingStatus.kind}`}>
                  <span>{portalCopy.console.advanced.computerAuth}</span>
                  <strong>{localPairingStatus.title}</strong>
                  <p>{localPairingStatus.message}</p>
                </div>
                <div className={`local-node-card ${storeCredentialsReady ? "online" : "warn"}`}>
                  <span>{portalCopy.console.advanced.storeAuth}</span>
                  <strong>{storeCredentialsReady ? portalCopy.console.advanced.storeReady : portalCopy.console.advanced.storePending}</strong>
                  <p>
                    {storeCredentialsReady
                      ? portalCopy.console.advanced.storeReadyText
                      : portalCopy.console.advanced.storePendingText}
                  </p>
                </div>
                <div className={`local-node-card ${posterConfigReady ? "online" : "warn"}`}>
                  <span>{portalCopy.console.advanced.posterApi}</span>
                  <strong>{posterConfigReady ? portalCopy.console.advanced.posterReady : portalCopy.console.advanced.posterOptional}</strong>
                  <p>
                    {posterConfigReady
                      ? portalCopy.console.advanced.posterReadyText(localNode.portal?.openai?.image_model ?? portalCopy.defaults.currentModel)
                      : portalCopy.console.advanced.posterOptionalText}
                  </p>
                </div>
                <div className="local-node-card">
                  <span>{portalCopy.console.advanced.packageVersion}</span>
                  <strong>{releaseVersion}</strong>
                  <p>{downloads?.release_manifest_url ?? portalCopy.defaults.downloadSyncPending}</p>
                </div>
                <div className="local-node-card">
                  <span>{portalCopy.console.advanced.releaseCheck}</span>
                  <strong>{releaseCommit}</strong>
                  <p>{releaseChecksum}</p>
                </div>
              </div>
              <div className="command-row">
                <button disabled={localNode.phase === "checking"} onClick={probeLocalNode}>
                  <RefreshCcw size={18} /> {portalCopy.console.advanced.probe}
                </button>
                {localNodeDownloadOptions.length === 0 && (
                  <button className="secondary" disabled>
                    <Download size={18} /> {portalCopy.console.setup.actions.packagePreparing}
                  </button>
                )}
                {localNodeDownloadOptions.map((option, index) => (
                  <a className={`download ${index > 0 ? "secondary" : ""}`} href={option.url} key={option.key}>
                    <Download size={18} /> {option.label}
                  </a>
                ))}
                {canOpenLocalConsole && (
                  <a className="download secondary" href={LOCAL_CONSOLE_URL} target="_blank" rel="noreferrer">
                    <MonitorCheck size={18} /> {portalCopy.console.setup.actions.openWorkspace}
                  </a>
                )}
                {canDownloadOpenClawPlugin && (
                  <a className="download secondary" href={openclawPluginUrl} target="_blank" rel="noreferrer">
                    <Download size={18} /> {portalCopy.console.advanced.pluginPackage}
                  </a>
                )}
                <button className="secondary" disabled={!canCopyLocalManifest} onClick={copyLocalManifestUrl}>
                  <Clipboard size={18} /> {portalCopy.console.advanced.copyPluginUrl}
                </button>
              </div>
              {localNode.manifest && (
                <div className="manifest-tools">
                  {localNode.manifest.tools.map((tool) => (
                    <div key={tool.name}>
                      <span>{tool.risk}</span>
                      <strong>{tool.name}</strong>
                      <em>{tool.approval_required ? portalCopy.console.advanced.approvalRequired : portalCopy.console.advanced.readOnly}</em>
                    </div>
                  ))}
                </div>
              )}
            </details>

            <details className="op-section support-section">
              <summary>
                <ShieldCheck size={20} />
                <div>
                  <strong>{portalCopy.console.device.summaryTitle}</strong>
                  <span>{portalCopy.console.device.summaryText}</span>
                </div>
              </summary>
              <div className="form-grid">
                <label>
                  {portalCopy.console.device.name}
                  <input value={deviceName} onChange={(event) => setDeviceName(event.target.value)} />
                </label>
                <label>
                  {portalCopy.console.device.code}
                  <input
                    value={deviceFingerprint}
                    readOnly
                    placeholder={portalCopy.console.device.codePlaceholder}
                    title={portalCopy.console.device.codeTitle}
                  />
                </label>
                <label>
                  {portalCopy.console.device.authStatus}
                  <input
                    value={
                      computerAuthorized
                        ? portalCopy.console.device.complete
                        : device
                          ? portalCopy.console.device.almostDone
                          : portalCopy.console.device.unauthorized
                    }
                    readOnly
                  />
                </label>
              </div>
              <div className="command-row">
                <button disabled={!canBindLocalDevice} onClick={activateDevice}>
                  <MonitorCheck size={18} /> {portalCopy.console.device.authorize}
                </button>
                <button className="secondary" onClick={issueLease} disabled={!device || computerAuthorized || authBusy}>
                  <Radio size={18} /> {portalCopy.console.device.completeAuth}
                </button>
              </div>
              {lease && (
                <div className="lease-line">
                  <span>{portalCopy.console.device.lease}</span>
                  <strong>{lease.lease_id}</strong>
                  <em>{portalCopy.console.device.expiresAt(lease.expires_at)}</em>
                </div>
              )}
            </details>
          </section>
        </section>
        </section>
      )}

      {!session && (
      <section className="pricing-band motion-reveal" data-reveal id="pricing">
        <div>
          <p className="eyebrow">{portalCopy.pricing.eyebrow}</p>
          <h2>{portalCopy.pricing.title}</h2>
          <p>{portalCopy.pricing.text}</p>
        </div>
        {session ? (
          <a className="download" href="#console">
            <ArrowRight size={18} /> {portalCopy.pricing.accountAuth}
          </a>
        ) : (
          <button onClick={() => openAuthDialog("register")}>
            <UserPlus size={18} /> {portalCopy.pricing.start}
          </button>
        )}
      </section>
      )}

      {authDialogOpen && (
        <div className="auth-backdrop motion-enter" role="presentation" onMouseDown={() => setAuthDialogMode(null)}>
          <section
            aria-labelledby="auth-dialog-title"
            aria-modal="true"
            className="auth-dialog motion-enter"
            role="dialog"
            onMouseDown={(event) => event.stopPropagation()}
          >
            <button className="icon-button close-button" aria-label={portalCopy.auth.closeAria} onClick={() => setAuthDialogMode(null)}>
              <X size={18} />
            </button>
            <div className="dialog-heading">
              <p className="eyebrow">{copy.common.brand}</p>
              <h2 id="auth-dialog-title">{authDialogTitle}</h2>
              <p>{authDialogDescription}</p>
            </div>

            <div className="auth-context-line">
              <ShieldCheck size={17} />
              <span>
                {DIRECT_SKYBRIDGE_AUTH_ENABLED
                  ? directAuthContext
                  : NEBULA_OAUTH_CONFIGURED
                    ? portalCopy.auth.contextSsoOnly
                    : portalCopy.auth.contextMaintenance}
              </span>
            </div>

            {DIRECT_SKYBRIDGE_AUTH_ENABLED && (
              <section className="direct-auth-panel">
                <div className="section-title compact-title">
                  {authMethod === "phone" ? <Smartphone /> : <Mail />}
                  <div>
                    <h2>{directAuthTitle}</h2>
                    <p>{directAuthText}</p>
                  </div>
                </div>
                <form className="form-grid skybridge-auth-grid auth-card-form" onSubmit={handleSkybridgePasswordSubmit}>
                  <div className="method-switch auth-methods" aria-label={portalCopy.auth.methodsAria}>
                    <button className={authMethod === "email" ? "active" : ""} type="button" onClick={() => setAuthMethod("email")}>
                      <Mail size={18} /> {portalCopy.auth.email}
                    </button>
                    {SKYBRIDGE_PHONE_AUTH_ENABLED ? (
                      <button className={authMethod === "phone" ? "active" : ""} type="button" onClick={() => setAuthMethod("phone")}>
                        <Smartphone size={18} /> {portalCopy.auth.phone}
                      </button>
                    ) : null}
                    {authMode === "login" && (
                      <button className={authMethod === "nebula" ? "active" : ""} type="button" onClick={() => setAuthMethod("nebula")}>
                        <KeyRound size={18} /> {portalCopy.auth.nebula}
                      </button>
                    )}
                  </div>

                  <label>
                    {identifierLabel(authMode, authMethod)}
                    <input
                      autoComplete={identifierAutocomplete(authMethod)}
                      placeholder={identifierPlaceholder(authMode, authMethod)}
                      value={skybridgeIdentifier}
                      onChange={(event) => setSkybridgeIdentifier(event.target.value)}
                    />
                  </label>
                  {authMode === "register" && (
                    <label>
                      {portalCopy.auth.name}
                      <input
                        autoComplete="name"
                        placeholder={portalCopy.auth.namePlaceholder}
                        value={skybridgeName}
                        onChange={(event) => setSkybridgeName(event.target.value)}
                      />
                    </label>
                  )}
                  {authMode === "register" && authMethod === "phone" && (
                    <label>
                      {portalCopy.auth.backupEmail}
                      <input
                        autoComplete="email"
                        placeholder={portalCopy.auth.backupEmailPlaceholder}
                        value={skybridgePhoneEmail}
                        onChange={(event) => setSkybridgePhoneEmail(event.target.value)}
                      />
                    </label>
                  )}
                  {authMethod === "phone" ? (
                    <label>
                      {portalCopy.auth.smsCode}
                      <input
                        autoComplete="one-time-code"
                        placeholder={portalCopy.auth.smsCodePlaceholder}
                        value={skybridgePhoneCode}
                        onChange={(event) => setSkybridgePhoneCode(event.target.value)}
                      />
                    </label>
                  ) : (
                    <label>
                      {portalCopy.auth.password}
                      <input
                        autoComplete={authMode === "register" ? "new-password" : "current-password"}
                        placeholder={portalCopy.auth.passwordPlaceholder}
                        value={skybridgePassword}
                        onChange={(event) => setSkybridgePassword(event.target.value)}
                        type="password"
                      />
                    </label>
                  )}
                  {authMethod === "phone" && (
                    <button
                      className="secondary"
                      disabled={authBusy || skybridgeOtpBusy || directAuthNeedsTurnstile}
                      type="button"
                      onClick={sendSkybridgePhoneCode}
                    >
                      <Smartphone size={18} /> {portalCopy.auth.requestCode}
                    </button>
                  )}
                  <button disabled={authBusy || directAuthNeedsTurnstile} type="submit">
                    {authMode === "register" ? <UserPlus size={18} /> : <LogIn size={18} />}
                    {authSubmitText}
                  </button>
                </form>

                {!SKYBRIDGE_PHONE_AUTH_ENABLED && <p className="identity-note">{portalCopy.auth.phoneAuthUnavailable}</p>}
                {skybridgeOtpStatus && <p className="identity-note">{skybridgeOtpStatus}</p>}

                {SKYBRIDGE_TURNSTILE_CONFIGURED && (
                  <div className="turnstile-row">
                    <div ref={turnstileContainerRef} />
                    <span>{turnstileStatus}</span>
                  </div>
                )}

                {directAuthNeedsTurnstile && (
                  <p className="identity-note">{portalCopy.auth.needsVerification}</p>
                )}
              </section>
            )}

            {!DIRECT_SKYBRIDGE_AUTH_ENABLED && (
              <p className="identity-note">{directAuthUnavailableMessage}</p>
            )}

            {NEBULA_OAUTH_ENTRY_ENABLED && (
              <details className="compat-panel sso-panel">
                <summary>{portalCopy.auth.ssoSummary}</summary>
                <section className="nebula-oauth-panel">
                  <div className="section-title compact-title">
                    <ShieldCheck />
                    <div>
                      <h2>{portalCopy.auth.ssoTitle(authMode)}</h2>
                      <p>{portalCopy.auth.ssoText}</p>
                    </div>
                  </div>
                  <div className="oauth-actions">
                    <button disabled={authBusy || !NEBULA_OAUTH_CONFIGURED} onClick={() => startNebulaOAuth(authMode)}>
                      <ExternalLink size={18} /> {authMode === "register" ? portalCopy.auth.openSsoRegister : portalCopy.auth.openSsoLogin}
                    </button>
                  </div>
                </section>
              </details>
            )}

            {captchaBlocked && (
              <div className="captcha-callout">
                <AlertCircle />
                <div>
                  <strong>{portalCopy.auth.captchaTitle}</strong>
                  <p>{portalCopy.auth.captchaText}</p>
                </div>
              </div>
            )}

            {shouldShowAuthDialogStatus && (
              <div
                className={`status-line ${authState.phase === "failed" ? "danger-line" : authBusy ? "busy-line" : ""} ${statusUpdated ? "is-updated" : ""}`}
                role="status"
                aria-live="polite"
                aria-atomic="true"
                aria-busy={authBusy}
              >
                {authState.phase === "failed" ? <AlertCircle size={18} /> : authBusy ? <RefreshCcw size={18} /> : <CheckCircle2 size={18} />}
                {statusMessage}
              </div>
            )}

            <p className="auth-switch-copy">
              {authMode === "login" ? portalCopy.auth.switchQuestionLogin : portalCopy.auth.switchQuestionRegister}
              <button type="button" onClick={() => switchAuthMode(authMode === "login" ? "register" : "login")}>
                {authMode === "login" ? portalCopy.auth.switchToRegister : portalCopy.auth.switchToLogin}
              </button>
            </p>
          </section>
        </div>
      )}
    </main>
  );
}

function authReducer(state: AuthState, action: AuthAction): AuthState {
  if (action.type !== "begin" && action.type !== "signed_out" && action.requestId !== state.requestId) {
    return state;
  }
  if (action.type === "begin") {
    return { phase: action.phase, message: action.message, requestId: action.requestId };
  }
  if (action.type === "failure") {
    return { phase: "failed", message: action.message, requestId: action.requestId };
  }
  if (action.type === "signed_out") {
    return { phase: "signed_out", message: action.message, requestId: action.requestId };
  }
  return { phase: "authenticated", message: action.message, requestId: action.requestId };
}

function isAuthBusy(phase: AuthPhase) {
  return (
    phase === "authenticating_skybridge" ||
    phase === "creating_service_session" ||
    phase === "authenticating_local_dev" ||
    phase === "refreshing"
  );
}

function isValidStoredSession(value: unknown): value is Session {
  if (!value || typeof value !== "object") return false;
  const candidate = value as { token?: unknown; user?: unknown };
  if (typeof candidate.token !== "string" || !candidate.token) return false;
  const user = candidate.user as Partial<User> | undefined;
  if (!user || typeof user !== "object") return false;
  return (
    typeof user.id === "string" &&
    typeof user.tenant_id === "string" &&
    typeof user.nebula_id === "string" &&
    typeof user.role === "string"
  );
}

function loadSession(): Session | null {
  try {
    const raw = localStorage.getItem(SESSION_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as unknown;
    return isValidStoredSession(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

async function parseResponse<T>(response: Response): Promise<T> {
  const text = await response.text();
  let data: T | ApiError;
  try {
    data = text ? (JSON.parse(text) as T | ApiError) : ({ error: response.statusText } as ApiError);
  } catch {
    data = { error: text || response.statusText };
  }
  if (!response.ok || isApiError(data)) {
    throw new Error(isApiError(data) ? data.error : response.statusText);
  }
  return data;
}

async function fetchWithTimeout(input: RequestInfo | URL, init: RequestInit = {}, timeoutMs = REQUEST_TIMEOUT_MS) {
  const controller = new AbortController();
  const timeout = window.setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(input, {
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

async function copyText(value: string) {
  try {
    await navigator.clipboard.writeText(value);
    return;
  } catch {
    const element = document.createElement("textarea");
    element.value = value;
    element.setAttribute("readonly", "true");
    element.style.position = "fixed";
    element.style.opacity = "0";
    document.body.appendChild(element);
    element.select();
    document.execCommand("copy");
    document.body.removeChild(element);
  }
}

async function localNodeJson<T>(path: string): Promise<T> {
  const response = await fetchWithTimeout(`${LOCAL_NODE_API}${path}`, {}, 4_000);
  return parseResponse<T>(response);
}

async function localNodePost<T>(path: string, body: unknown): Promise<T> {
  const response = await fetchWithTimeout(
    `${LOCAL_NODE_API}${path}`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json"
      },
      body: JSON.stringify(body)
    },
    4_000
  );
  return parseResponse<T>(response);
}

function localNodeOnlineMessage(health: LocalNodeHealth, portalError: unknown | null) {
  const copy = getPortalCopy();
  const mode = health.real_ozon_enabled ? copy.messages.realOzonEnabled : copy.messages.devMode;
  const version = health.package_version ? ` v${health.package_version}` : "";
  if (portalError) {
    return copy.messages.localNodeOnlineWithIssue(version, mode, userFacingLocalNodeIssue(portalError));
  }
  return copy.messages.localNodeOnline(version, mode);
}

function localNodeFailedEndpoint(error: unknown) {
  const message = errorMessage(error);
  if (message.startsWith("health:")) return "health";
  if (message.startsWith("manifest:")) return "manifest";
  return null;
}

function stripLocalNodeEndpointPrefix(message: string) {
  return message.replace(/^(health|manifest):\s*/i, "");
}

function localNodeFailureMessage(error: unknown, endpoint: string | null = localNodeFailedEndpoint(error)) {
  const copy = getPortalCopy();
  if (endpoint === "health") {
    if (isLocalNodeBrowserBlock(error)) {
      return copy.messages.localNodeHealthBlocked;
    }
    return copy.messages.localNodeNoResponse;
  }
  if (endpoint === "manifest") {
    return copy.messages.localNodeManifestFailed;
  }
  if (isLocalNodeBrowserBlock(error)) {
    return copy.messages.localNodeBlocked;
  }
  return copy.messages.localNodeNotDetected;
}

function isLocalNodeBrowserBlock(error: unknown) {
  const message = errorMessage(error).toLowerCase();
  return message.includes("failed to fetch") || message.includes("networkerror") || message.includes("load failed");
}

function localNodeStatusLabel(phase: LocalNodePhase) {
  const labels = getPortalCopy().labels.localNodePhase;
  switch (phase) {
    case "checking":
      return labels.checking;
    case "online":
      return labels.online;
    case "degraded":
      return labels.degraded;
    case "blocked":
      return labels.blocked;
    case "offline":
      return labels.offline;
    default:
      return labels.idle;
  }
}

function confirmedOrderMessage(order: Order) {
  const messages = getPortalCopy().messages;
  if (order.payment_provider === "manual") {
    return messages.confirmedManualOrder;
  }
  return messages.confirmedOrder;
}

function wizardStepClass(done: boolean, current: boolean) {
  if (done) return "wizard-step done motion-complete-flash";
  if (current) return "wizard-step current motion-current-step";
  return "wizard-step";
}

function setupStepNumber(input: {
  activeEntitlement: Entitlement | null;
  computerHelperOnline: boolean;
  computerAuthorized: boolean;
  storeCredentialsReady: boolean;
}) {
  if (!input.activeEntitlement) return 1;
  if (!input.computerHelperOnline) return 2;
  if (!input.computerAuthorized) return 3;
  if (!input.storeCredentialsReady) return 4;
  return 0;
}

function orderStatusLabel(status: string) {
  const labels = getPortalCopy().labels.orderStatus;
  switch (status) {
    case "confirmed":
      return labels.confirmed;
    case "pending_manual_payment":
      return labels.pendingManualPayment;
    case "pending":
      return labels.pending;
    case "cancelled":
      return labels.cancelled;
    default:
      return status;
  }
}

function paymentProviderLabel(provider: string) {
  const labels = getPortalCopy().labels.paymentProvider;
  switch (provider) {
    case "manual":
      return labels.manual;
    case "wechat":
      return labels.wechat;
    case "stripe":
      return labels.stripe;
    default:
      return provider;
  }
}

function localNodePairingStatus(
  localNode: LocalNodeProbe,
  entitlement: Entitlement | null,
  device: Device | null,
  leaseStatus: LocalLeaseStatus | null,
  cloudLeaseIssued: boolean
) {
  const labels = getPortalCopy().labels.pairing;
  if (!entitlement) {
    return {
      kind: "warn",
      title: labels.serviceClosedTitle,
      message: labels.serviceClosedText
    };
  }
  if (!device) {
    if (localNode.phase !== "online") {
      return {
        kind: "warn",
        title: labels.connectHelperTitle,
        message: labels.connectHelperText
      };
    }
    return {
      kind: "warn",
      title: labels.notAuthorizedTitle,
      message: labels.notAuthorizedText
    };
  }
  if (!leaseStatus?.valid) {
    const issue = userFacingLocalLeaseIssue(leaseStatus?.issue);
    return {
      kind: localNode.phase === "online" ? "warn" : "offline",
      title: cloudLeaseIssued ? labels.helperLeaseMissingTitle : labels.almostDoneTitle,
      message: issue ?? labels.confirmAgainText
    };
  }
  return {
    kind: localNode.phase === "online" ? "online" : "warn",
    title: localNode.phase === "online" ? labels.authorizedTitle : labels.savedTitle,
    message: labels.expiresAt(leaseStatus.expires_at ?? labels.beforeExpiry)
  };
}

function setupStatusModel(input: {
  activeEntitlement: Entitlement | null;
  computerAuthorized: boolean;
  device: Device | null;
  localLeaseIssue: string | null;
  localNode: LocalNodeProbe;
  order: Order | null;
  storeCredentialsReady: boolean;
}) {
  const labels = getPortalCopy().labels.setupStatus;
  const nodeOnline = input.localNode.phase === "online";
  if (!input.activeEntitlement) {
    return {
      kind: "warn",
      title: input.order ? labels.waitServiceTitle : labels.openServiceTitle,
      message: input.order ? labels.orderPendingText : labels.openServiceText
    };
  }
  if (!nodeOnline) {
    return {
      kind: "warn",
      title: labels.installHelperTitle,
      message: labels.installHelperText
    };
  }
  if (input.computerAuthorized) {
    if (!input.storeCredentialsReady) {
      return {
        kind: "warn",
        title: labels.fillStoreTitle,
        message: labels.fillStoreText
      };
    }
    return {
      kind: "online",
      title: labels.readyTitle,
      message: labels.readyTextBrief
    };
  }
  if (!input.device) {
    return {
      kind: "warn",
      title: labels.authorizeTitle,
      message: labels.authorizeText
    };
  }
  if (!input.computerAuthorized) {
    const issue = userFacingLocalLeaseIssue(input.localLeaseIssue);
    return {
      kind: "warn",
      title: issue ? labels.incompleteTitle : labels.completeTitle,
      message: issue ?? labels.completeText
    };
  }
  return {
    kind: "online",
    title: labels.readyTitle,
    message: labels.readyTextImage
  };
}

function userFacingLocalLeaseIssue(issue?: string | null) {
  const messages = getPortalCopy().messages;
  if (!issue || issue === "cloud lease is not installed") return null;
  if (isLocalNodeBrowserBlock(new Error(issue))) {
    return messages.localLeaseBrowserBlock;
  }
  if (issue.includes("public key") || issue.includes("signature")) {
    return messages.localLeaseOldHelper;
  }
  if (issue.includes("expired")) {
    return messages.localLeaseExpired;
  }
  if (issue.includes("stored cloud lease is invalid")) {
    return messages.localLeaseInvalid;
  }
  return messages.localLeaseMissing;
}

function localLeaseWriteFailureMessage(error: unknown) {
  const issue = userFacingLocalLeaseIssue(errorMessage(error));
  return issue ?? getPortalCopy().messages.localLeaseMissing;
}

function userFacingLocalNodeIssue(error: unknown) {
  const issue = userFacingLocalLeaseIssue(stripLocalNodeEndpointPrefix(errorMessage(error)));
  return issue ?? getPortalCopy().messages.localNodeIssue;
}

function statusMessageTone(message: string, phase: AuthPhase, busy: boolean) {
  if (busy) return "ok";
  if (phase === "failed") return "danger";
  return portalMessageTone(message);
}

function absolutePortalUrl(pathOrUrl: string) {
  return new URL(pathOrUrl, window.location.origin).toString();
}

type LocalPlatform = "mac" | "windows" | "other";

type LocalNodeDownloadOption = {
  key: string;
  label: string;
  shortLabel: string;
  url: string;
};

function detectLocalPlatform(): LocalPlatform {
  const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
  const value = `${nav.userAgentData?.platform ?? ""} ${navigator.platform ?? ""} ${navigator.userAgent ?? ""}`.toLowerCase();
  if (value.includes("mac")) return "mac";
  if (value.includes("win")) return "windows";
  return "other";
}

function localNodeDownloads(
  platform: LocalPlatform,
  macDmgUrl: string,
  msiUrl: string,
  exeUrl: string
): LocalNodeDownloadOption[] {
  const labels = getPortalCopy().labels.downloads;
  const mac = macDmgUrl
    ? [{ key: "mac-dmg", label: labels.macLabel, shortLabel: labels.macShort, url: macDmgUrl }]
    : [];
  const windows = [
    ...(msiUrl ? [{ key: "windows-msi", label: labels.windowsMsiLabel, shortLabel: labels.windowsMsiShort, url: msiUrl }] : []),
    ...(exeUrl ? [{ key: "windows-exe", label: labels.windowsExeLabel, shortLabel: labels.windowsExeShort, url: exeUrl }] : [])
  ];

  if (platform === "mac") return [...mac, ...windows];
  if (platform === "windows") return [...windows, ...mac];
  return [...mac, ...windows];
}

function releaseChecksumLabel(manifest: LocalNodeReleaseManifest) {
  return [
    manifest.macos_aarch64_dmg ? `Mac ${shortSha256(manifest.macos_aarch64_dmg.sha256)}` : null,
    manifest.msi ? `MSI ${shortSha256(manifest.msi.sha256)}` : null,
    manifest.exe ? `EXE ${shortSha256(manifest.exe.sha256)}` : null
  ]
    .filter(Boolean)
    .join(" / ");
}

function shortCommit(commit: string) {
  return commit.length > 12 ? commit.slice(0, 12) : commit;
}

function shortSha256(hash: string) {
  return hash.length > 12 ? `${hash.slice(0, 12)}...` : hash;
}

function defaultCloudApiBase() {
  if (typeof window === "undefined") {
    return "http://127.0.0.1:8080";
  }
  const hostname = window.location.hostname.replace(/^www\./, "");
  if (hostname === "ozon66.com") {
    return "https://api.ozon66.com";
  }
  if (hostname === "localhost" || hostname === "127.0.0.1") {
    return "http://127.0.0.1:8080";
  }
  if (import.meta.env.DEV) {
    return "http://127.0.0.1:8080";
  }
  throw new Error(getPortalCopy().messages.cloudApiRequired);
}

function normalizeOptionalUrl(value: string | undefined) {
  const trimmed = value?.trim();
  if (!trimmed) return "";
  return normalizeBaseUrl(trimmed);
}

async function skybridgePasswordAuth(input: {
  mode: AuthMode;
  method: LoginMethod;
  identifier: string;
  password: string;
  phoneCode?: string;
  name: string;
  captchaToken?: string;
}): Promise<SkybridgeAuthSession> {
  const directAuth = await loadDirectSkybridgeAuth();
  return directAuth.skybridgePasswordAuth(input, directSkybridgeAuthConfig());
}

async function skybridgeSendPhoneOtp(input: {
  phone: string;
  mode: AuthMode;
  name: string;
  captchaToken?: string;
}) {
  const directAuth = await loadDirectSkybridgeAuth();
  await directAuth.skybridgeSendPhoneOtp(input, directSkybridgeAuthConfig());
}

async function skybridgeUpdatePhoneRegistrationProfile(input: {
  accessToken: string;
  phone: string;
  email: string;
  name: string;
}) {
  const directAuth = await loadDirectSkybridgeAuth();
  await directAuth.skybridgeUpdatePhoneRegistrationProfile(input, directSkybridgeAuthConfig());
}

async function loadDirectSkybridgeAuth() {
  if (!DIRECT_SKYBRIDGE_AUTH_ENABLED) {
    throw new Error(getPortalCopy().messages.directAuthNotConfigured);
  }
  if (
    import.meta.env.VITE_ENABLE_DIRECT_SKYBRIDGE_AUTH === "1" ||
    import.meta.env.VITE_ENABLE_DIRECT_SKYBRIDGE_AUTH === "true" ||
    import.meta.env.VITE_ENABLE_DIRECT_SKYBRIDGE_AUTH === "yes"
  ) {
    return import("./skybridgeAuth");
  }
  throw new Error(getPortalCopy().messages.directAuthNotConfigured);
}

function directSkybridgeAuthConfig() {
  return {
    authBase: SKYBRIDGE_AUTH_BASE,
    publicAuthKey: SKYBRIDGE_PUBLIC_AUTH_KEY.trim(),
    phoneAuthEnabled: SKYBRIDGE_PHONE_AUTH_ENABLED
  };
}

async function redirectToNebulaOAuth(flow: NebulaOAuthFlow) {
  if (!NEBULA_OAUTH_CONFIGURED) {
    throw new Error(getPortalCopy().messages.oauthConfigRequired);
  }
  if (!crypto.getRandomValues || !crypto.subtle) {
    throw new Error(getPortalCopy().messages.cryptoUnsupported);
  }

  const state = randomPkceString(32);
  const codeVerifier = randomPkceString(96);
  const codeChallenge = await sha256Base64Url(codeVerifier);
  const redirectUri = getNebulaRedirectUri();

  saveNebulaOAuthSession({
    baseUrl: NEBULA_OAUTH_BASE,
    clientId: NEBULA_CLIENT_ID,
    redirectUri,
    codeVerifier,
    state,
    flow
  });

  const authorizeUrl = new URL(`${NEBULA_OAUTH_BASE}/oauth/authorize`);
  authorizeUrl.searchParams.set("response_type", "code");
  authorizeUrl.searchParams.set("client_id", NEBULA_CLIENT_ID);
  authorizeUrl.searchParams.set("redirect_uri", redirectUri);
  authorizeUrl.searchParams.set("scope", NEBULA_SCOPE);
  authorizeUrl.searchParams.set("state", state);
  authorizeUrl.searchParams.set("code_challenge", codeChallenge);
  authorizeUrl.searchParams.set("code_challenge_method", "S256");
  if (flow === "register") {
    authorizeUrl.searchParams.set("flow", "register");
  }

  window.location.assign(authorizeUrl.toString());
}

async function exchangeNebulaOAuthCode(
  code: string,
  storedSession: NebulaOAuthSession
): Promise<SkybridgeAuthSession> {
  const response = await fetchWithTimeout(`${storedSession.baseUrl}/oauth/token`, {
    method: "POST",
    headers: {
      Accept: "application/json",
      "Content-Type": "application/x-www-form-urlencoded"
    },
    body: new URLSearchParams({
      grant_type: "authorization_code",
      client_id: storedSession.clientId,
      code,
      redirect_uri: storedSession.redirectUri,
      code_verifier: storedSession.codeVerifier
    }).toString()
  });
  const result = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(skybridgeErrorMessage(result, getPortalCopy().messages.webLoginFailed));
  }
  const session = extractSkybridgeSession(result);
  if (!session?.access_token) {
    throw new Error(getPortalCopy().messages.webLoginInvalid);
  }
  return session;
}

function getNebulaRedirectUri() {
  return `${window.location.origin}/auth/callback`;
}

function saveNebulaOAuthSession(session: NebulaOAuthSession) {
  sessionStorage.setItem(NEBULA_OAUTH_STORAGE_KEY, JSON.stringify(session));
}

function readNebulaOAuthSession(): NebulaOAuthSession | null {
  const rawValue = sessionStorage.getItem(NEBULA_OAUTH_STORAGE_KEY);
  if (!rawValue) return null;
  try {
    const parsed = JSON.parse(rawValue) as Partial<NebulaOAuthSession>;
    if (
      !parsed.baseUrl ||
      !parsed.clientId ||
      !parsed.redirectUri ||
      !parsed.codeVerifier ||
      !parsed.state ||
      !parsed.flow
    ) {
      return null;
    }
    return parsed as NebulaOAuthSession;
  } catch {
    return null;
  }
}

function clearNebulaOAuthSession() {
  sessionStorage.removeItem(NEBULA_OAUTH_STORAGE_KEY);
}

function replaceLocationPath(path: string) {
  window.history.replaceState(window.history.state, "", path);
}

function encodeBase64Url(bytes: Uint8Array) {
  let binary = "";
  bytes.forEach((byte) => {
    binary += String.fromCharCode(byte);
  });
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function randomPkceString(length: number) {
  const bytes = new Uint8Array(length);
  crypto.getRandomValues(bytes);
  return encodeBase64Url(bytes).slice(0, length);
}

async function sha256Base64Url(value: string) {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
  return encodeBase64Url(new Uint8Array(digest));
}

function normalizePhoneForSkybridge(value: string) {
  const sanitized = value
    .trim()
    .split("")
    .filter((character) => /[0-9+]/.test(character))
    .join("");
  if (sanitized.startsWith("+")) return sanitized;
  if (sanitized.startsWith("86") && sanitized.length === 13) return `+${sanitized}`;
  if (sanitized.startsWith("1") && sanitized.length === 11) return `+86${sanitized}`;
  return sanitized;
}

function isValidSkybridgePhone(value: string) {
  return /^\+[1-9]\d{1,14}$/.test(value);
}

function loadTurnstileScript() {
  if (!SKYBRIDGE_TURNSTILE_SCRIPT_URL) {
    return Promise.reject(new Error(getPortalCopy().messages.turnstileScriptNotConfigured));
  }
  if (window.turnstile) {
    return Promise.resolve();
  }

  const existing = document.querySelector<HTMLScriptElement>('script[data-ozon-turnstile="true"]');
  if (existing) {
    return new Promise<void>((resolve, reject) => {
      existing.addEventListener("load", () => resolve(), { once: true });
      existing.addEventListener("error", () => reject(new Error(getPortalCopy().messages.turnstileScriptLoadFailed)), {
        once: true
      });
    });
  }

  return new Promise<void>((resolve, reject) => {
    const script = document.createElement("script");
    script.dataset.ozonTurnstile = "true";
    script.async = true;
    script.defer = true;
    script.src = SKYBRIDGE_TURNSTILE_SCRIPT_URL;
    script.onload = () => resolve();
    script.onerror = () => reject(new Error(getPortalCopy().messages.turnstileScriptLoadFailed));
    document.head.appendChild(script);
  });
}

function turnstileFailureMessage(error: unknown) {
  const messages = getPortalCopy().messages;
  const code = String(error ?? "").trim();
  if (code.startsWith("200500")) {
    return messages.turnstileServiceBlocked;
  }
  if (code.startsWith("300") || code.startsWith("600")) {
    return messages.turnstileNotPassed;
  }
  return messages.turnstileFailed(code);
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
    const error = record.error as Record<string, unknown>;
    if (typeof error.message === "string") return error.message;
  }
  return fallback;
}

function isApiError(value: unknown): value is ApiError {
  return typeof value === "object" && value !== null && "error" in value;
}

function defaultFingerprint() {
  const existing = localStorage.getItem("ozon-rust-suite.portal.device-fingerprint");
  if (existing) return existing;
  const next = `portal-${crypto.randomUUID()}`;
  localStorage.setItem("ozon-rust-suite.portal.device-fingerprint", next);
  return next;
}

function buildLocalAuthBody(
  mode: AuthMode,
  loginMethod: LoginMethod,
  identifier: string,
  password: string,
  name: string
) {
  const trimmedIdentifier = identifier.trim();
  if (mode === "register") {
    const method = loginMethod === "nebula" ? "email" : loginMethod;
    return {
      login_method: method,
      identifier: trimmedIdentifier,
      password,
      name: name.trim() || undefined
    };
  }
  return {
    login_method: loginMethod,
    identifier: trimmedIdentifier,
    password
  };
}

function displayLoginAlias(user: User) {
  if (user.email) return user.email;
  if (user.phone) return user.phone;
  return getPortalCopy().labels.display.unbound;
}

function methodLabel(method: LoginMethod) {
  const labels = getPortalCopy().labels.methods;
  if (method === "email") return labels.email;
  if (method === "phone") return labels.phone;
  return labels.nebula;
}

function identifierLabel(mode: AuthMode, loginMethod: LoginMethod) {
  const labels = getPortalCopy().labels.methods;
  if (loginMethod === "email") return labels.email;
  if (loginMethod === "phone") return labels.phone;
  return mode === "login" ? labels.nebula : labels.email;
}

function identifierPlaceholder(mode: AuthMode, loginMethod: LoginMethod) {
  const placeholders = getPortalCopy().labels.placeholders;
  if (loginMethod === "email") return placeholders.email;
  if (loginMethod === "phone") return placeholders.phone;
  return mode === "login" ? placeholders.nebula : placeholders.email;
}

function identifierAutocomplete(loginMethod: LoginMethod) {
  if (loginMethod === "email") return "email";
  if (loginMethod === "phone") return "tel";
  return "username";
}

function formatMoney(amountMinor: number, currency: string) {
  const normalizedCurrency = currency.toUpperCase();
  try {
    return new Intl.NumberFormat(getCurrentLocale() === "zh" ? "zh-CN" : "en-US", {
      style: "currency",
      currency: normalizedCurrency
    }).format(amountMinor / 100);
  } catch {
    return `${normalizedCurrency} ${(amountMinor / 100).toFixed(2)}`;
  }
}

function normalizeBaseUrl(value: string) {
  return value.trim().replace(/\/+$/, "");
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : "unknown error";
}

function isInvalidSessionError(error: unknown) {
  const normalized = errorMessage(error).toLowerCase();
  return (
    normalized.includes("invalid bearer token") ||
    normalized.includes("missing bearer token") ||
    normalized.includes("expired bearer token") ||
    normalized.includes("jwt expired")
  );
}

function skybridgeDirectAuthFailureMessage(error: unknown) {
  const message = errorMessage(error);
  if (isCaptchaProtectionMessage(message)) {
    return getPortalCopy().messages.captchaProtection;
  }
  return message;
}

function isCaptchaProtectionMessage(message: string) {
  const normalized = message.toLowerCase();
  return normalized.includes("captcha") || normalized.includes("turnstile");
}

type PortalWindow = Window & {
  __OZON_PORTAL_ROOT__?: Root;
};

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error(getPortalCopy().messages.missingRoot);
}

const portalWindow = window as PortalWindow;
const root = portalWindow.__OZON_PORTAL_ROOT__ ?? createRoot(rootElement);
portalWindow.__OZON_PORTAL_ROOT__ = root;
applyDocumentLocale(getCurrentLocale());
const customerGuidePath = ["/customer-guide", "/customer-guide.html"].includes(
  window.location.pathname
);
root.render(customerGuidePath ? <CustomerGuide /> : <App />);
