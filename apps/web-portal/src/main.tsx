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
const SKYBRIDGE_ANON_KEY =
  import.meta.env.VITE_SKYBRIDGE_SUPABASE_ANON_KEY ??
  import.meta.env.VITE_SUPABASE_ANON_KEY ??
  "";
const SKYBRIDGE_AUTH_CONFIGURED = Boolean(SKYBRIDGE_AUTH_BASE && SKYBRIDGE_ANON_KEY);
const DIRECT_SKYBRIDGE_AUTH_ENABLED =
  SKYBRIDGE_AUTH_CONFIGURED &&
  !["0", "false", "no"].includes((import.meta.env.VITE_ENABLE_DIRECT_SKYBRIDGE_AUTH ?? "").toLowerCase());
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

type SkybridgeCurrentUserProfile = {
  id?: string;
  email?: string | null;
  phone?: string | null;
  user_metadata?: Record<string, unknown> | null;
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
    SKYBRIDGE_TURNSTILE_CONFIGURED ? "等待安全验证" : "无需验证"
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
    phase: loadSession() ? "authenticated" : "idle",
    message: loadSession() ? "已恢复本地会话，正在等待刷新" : "请选择邮箱或手机号登录",
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
  const [deviceName, setDeviceName] = useState("我的电脑");
  const [deviceFingerprint, setDeviceFingerprint] = useState(() => defaultFingerprint());
  const [device, setDevice] = useState<Device | null>(null);
  const [lease, setLease] = useState<Lease | null>(null);
  const [downloads, setDownloads] = useState<Downloads | null>(null);
  const [localNode, setLocalNode] = useState<LocalNodeProbe>({
    phase: "idle",
    message: "登录后会检测电脑助手是否打开"
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
  const releaseVersion = releaseManifest?.version ?? "待同步";
  const releaseCommit = releaseManifest?.commit ? shortCommit(releaseManifest.commit) : "待同步";
  const releaseChecksum = releaseManifest ? releaseChecksumLabel(releaseManifest) : "等待 release-manifest.json";
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
  const directAuthUnavailableMessage = "邮箱/手机号入口尚未开通，请联系运营支持。";
  const authSubmitText =
    authMethod === "phone"
      ? authMode === "register"
        ? "手机号注册"
        : "手机号登录"
      : authMethod === "nebula"
        ? "账号编号登录"
      : authMode === "register"
        ? "邮箱注册"
        : "邮箱登录";
  const authDialogTitle = authMode === "register" ? "创建账号" : "登录工作台";
  const authDialogDescription =
    authMode === "register"
      ? "账号创建后，按页面提示开通服务、安装电脑助手并连接店铺。"
      : "登录后继续完成开通、安装和电脑授权。";
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
    setSession(nextSession);
    localStorage.setItem(SESSION_KEY, JSON.stringify(nextSession));
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
    dispatchAuth({ type: "failure", message: "登录已过期，请重新登录", requestId });
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
    setTurnstileStatus(SKYBRIDGE_TURNSTILE_CONFIGURED ? "等待安全验证" : "无需验证");
  }

  async function startNebulaOAuth(flow: NebulaOAuthFlow) {
    const requestId = beginAuth(
      "authenticating_skybridge",
      `正在打开统一身份${flow === "register" ? "注册" : "登录"}页`
    );
    try {
      await redirectToNebulaOAuth(flow);
    } catch (error) {
      if (isCurrentAuth(requestId)) {
        dispatchAuth({
          type: "failure",
          message: `统一授权启动失败：${errorMessage(error)}`,
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

    const requestId = beginAuth("creating_service_session", "正在完成统一身份授权回调");
    const authError = currentUrl.searchParams.get("error_description") ?? currentUrl.searchParams.get("error");
    if (!storedSession) {
      replaceLocationPath("/");
      dispatchAuth({
        type: "failure",
        message: "授权上下文已失效，请从门户重新发起登录",
        requestId
      });
      return true;
    }

    if (authError) {
      clearNebulaOAuthSession();
      replaceLocationPath("/");
      dispatchAuth({
        type: "failure",
        message: `统一身份授权失败：${authError}`,
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
        message: "授权回调缺少 code/state，请重新登录",
        requestId
      });
      return true;
    }

    if (state !== storedSession.state) {
      clearNebulaOAuthSession();
      replaceLocationPath("/");
      dispatchAuth({
        type: "failure",
        message: "授权状态校验失败，请重新登录",
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
          message: `统一身份登录失败：${errorMessage(error)}`,
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
    if (!skybridgeIdentifier.trim() || (authMethod === "phone" ? !skybridgePhoneCode.trim() : !skybridgePassword)) {
      const requestId = beginAuth("failed", authMethod === "phone" ? "请填写手机号和短信验证码" : `请填写${methodLabel(authMethod)}和密码`);
      dispatchAuth({
        type: "failure",
        message: authMethod === "phone" ? "请填写手机号和短信验证码" : `请填写${methodLabel(authMethod)}和密码`,
        requestId
      });
      return;
    }

    const requestId = beginAuth(
      "authenticating_skybridge",
      `${authMode === "register" ? "正在注册" : "正在验证"}${methodLabel(authMethod)}`
    );
    try {
      const normalizedIdentifier =
        authMethod === "phone" ? normalizePhoneForSkybridge(skybridgeIdentifier) : skybridgeIdentifier.trim();
      if (authMethod === "phone" && !isValidSkybridgePhone(normalizedIdentifier)) {
        const message = "请填写有效手机号，例如 +8613800138000";
        dispatchAuth({ type: "failure", message, requestId });
        return;
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
        if (!skybridgeName.trim() || !skybridgePhoneEmail.trim()) {
          const message = "手机号注册需要填写昵称和联系邮箱";
          dispatchAuth({ type: "failure", message, requestId });
          return;
        }
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
          message: `账号登录失败：${skybridgeDirectAuthFailureMessage(error)}`,
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
    if (!SKYBRIDGE_AUTH_CONFIGURED) {
      setSkybridgeOtpStatus(directAuthUnavailableMessage);
      return;
    }
    if (!skybridgeIdentifier.trim()) {
      setSkybridgeOtpStatus("请先填写手机号");
      return;
    }
    const normalizedPhone = normalizePhoneForSkybridge(skybridgeIdentifier);
    if (!isValidSkybridgePhone(normalizedPhone)) {
      setSkybridgeOtpStatus("请填写有效手机号，例如 +8613800138000");
      return;
    }
    if (authMode === "register" && (!skybridgeName.trim() || !skybridgePhoneEmail.trim())) {
      setSkybridgeOtpStatus("手机号注册需要先填写昵称和联系邮箱");
      return;
    }
    if (directAuthNeedsTurnstile) {
      setSkybridgeOtpStatus("请先完成安全验证，再获取短信验证码");
      return;
    }

    setSkybridgeOtpBusy(true);
    setSkybridgeOtpStatus("正在请求短信验证码");
    try {
      await skybridgeSendPhoneOtp({
        phone: normalizedPhone,
        mode: authMode,
        name: skybridgeName,
        captchaToken: turnstileToken
      });
      setSkybridgeOtpStatus("短信验证码已发送，请查收后继续");
    } catch (error) {
      setSkybridgeOtpStatus(`短信验证码发送失败：${skybridgeDirectAuthFailureMessage(error)}`);
    } finally {
      setSkybridgeOtpBusy(false);
    }
  }

  async function createManualSkybridgeServiceSession() {
    if (!skybridgeAccessToken.trim()) {
      const requestId = beginAuth("failed", "请粘贴已登录会话 token");
      dispatchAuth({ type: "failure", message: "请粘贴已登录会话 token", requestId });
      return;
    }
    const requestId = beginAuth("creating_service_session", "正在用身份会话创建 Ozon 服务会话");
    try {
      await createSkybridgeServiceSession(skybridgeAccessToken.trim(), requestId);
      if (isCurrentAuth(requestId)) {
        setSkybridgeAccessToken("");
      }
    } catch (error) {
      if (isCurrentAuth(requestId)) {
        dispatchAuth({
          type: "failure",
          message: `身份会话交换失败：${errorMessage(error)}`,
          requestId
        });
      }
    }
  }

  async function createSkybridgeServiceSession(accessToken: string, requestId: number) {
    dispatchAuth({
      type: "begin",
      phase: "creating_service_session",
      message: "正在创建 Ozon 服务会话",
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
      message: `账号已登录：${displayLoginAlias(data.user)}`,
      requestId
    });
    await refreshAccount(nextSession.token, requestId);
  }

  async function authenticateLocalDev() {
    const requestId = beginAuth("authenticating_local_dev", "正在使用本地开发兜底入口");
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
        message: `local_dev 会话已建立：${data.user.nebula_id}`,
        requestId
      });
      await refreshAccount(nextSession.token, requestId);
    } catch (error) {
      if (isCurrentAuth(requestId)) {
        dispatchAuth({
          type: "failure",
          message: `本地开发入口失败：${errorMessage(error)}`,
          requestId
        });
      }
    }
  }

  async function refreshAccount(token = session?.token, requestId?: number) {
    if (!token) {
      const nextRequestId = beginAuth("failed", "请先登录账号");
      dispatchAuth({ type: "failure", message: "请先登录账号", requestId: nextRequestId });
      return;
    }
    const currentRequestId = requestId ?? beginAuth("refreshing", "正在刷新账户状态");
    if (requestId) {
      dispatchAuth({
        type: "begin",
        phase: "refreshing",
        message: "正在刷新账户状态",
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
      dispatchAuth({ type: "success", message: "账户状态已刷新", requestId: currentRequestId });
    } catch (error) {
      if (isCurrentAuth(currentRequestId)) {
        if (isInvalidSessionError(error)) {
          expireSession(currentRequestId);
          return;
        }
        dispatchAuth({
          type: "failure",
          message: `账户刷新失败：${errorMessage(error)}`,
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
    dispatchAuth({ type: "signed_out", message: "已退出登录", requestId });
  }

  async function createOrder() {
    if (!session) {
      setOperationStatus("请先登录账号");
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
        setOperationStatus("订单已创建，正在打开支付页");
        window.location.assign(data.payment.checkout_url);
        return;
      }
      setOperationStatus(data.payment?.message ?? "订单已创建，请按支付备注完成确认");
    } catch (error) {
      setOperationStatus(`创建订单失败：${errorMessage(error)}`);
    }
  }

  async function copyOrderInfo() {
    if (!order) {
      setOperationStatus("还没有可复制的订单");
      return;
    }
    await navigator.clipboard.writeText(
      `申请编号: ${order.id}\n付款方式: ${paymentProviderLabel(order.payment_provider)}\n付款备注: ${order.payment_reference}`
    );
    setOperationStatus("申请信息已复制");
  }

  async function refreshOrder() {
    if (!order) {
      setOperationStatus("还没有可刷新的订单");
      return;
    }
    try {
      const data = await api<OrderApiResponse>(`/orders/${order.id}`);
      setOrder(data.order);
      setPaymentSession(data.payment ?? null);
      setOperationStatus(
        data.order.status === "confirmed"
          ? confirmedOrderMessage(data.order)
          : `开通状态已刷新：${orderStatusLabel(data.order.status)}`
      );
    } catch (error) {
      setOperationStatus(`刷新订单失败：${errorMessage(error)}`);
    }
  }

  async function redeem() {
    if (!session || !cardKey.trim()) {
      setOperationStatus("需要登录并填写开通码");
      return;
    }
    try {
      await api<{ entitlement: Entitlement }>("/card-keys/redeem", {
        method: "POST",
        body: JSON.stringify({ card_key: cardKey.trim() })
      });
      setCardKey("");
      setOperationStatus("开通码已使用，服务已开通");
      await refreshAccount();
    } catch (error) {
      setOperationStatus(`开通失败：${errorMessage(error)}`);
    }
  }

  async function activateDevice() {
    if (!session) {
      setOperationStatus("请先登录账号");
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
      setOperationStatus("这台电脑已加入你的账号");
    } catch (error) {
      setOperationStatus(`授权这台电脑失败：${errorMessage(error)}`);
    }
  }

  async function issueLease() {
    if (!session || !device) {
      setOperationStatus("需要先登录并绑定设备");
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
        setOperationStatus("这台电脑已完成授权");
        await probeLocalNode();
      } catch (localError) {
        setLease(null);
        applyLocalLeaseIssue(errorMessage(localError));
        setOperationStatus(localLeaseWriteFailureMessage(localError));
      }
    } catch (error) {
      setOperationStatus(`电脑授权失败：${errorMessage(error)}`);
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
      message: "正在检测电脑助手是否已打开"
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
      setOperationStatus("电脑助手连接成功后，才能复制 OpenClaw 连接地址");
      return;
    }
    await copyText(localManifestUrl);
    setOperationStatus("OpenClaw 连接地址已复制");
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
        const requestId = beginAuth("refreshing", "正在恢复账户状态");
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
        message: "登录后会检测电脑助手是否打开"
      });
    }
  }, [session?.token]);

  useEffect(() => {
    if (checkoutNoticeHandled.current) return;
    const checkout = new URL(window.location.href).searchParams.get("checkout");
    if (!checkout) return;
    checkoutNoticeHandled.current = true;
    if (checkout === "success") {
      setOperationStatus("支付完成，正在刷新授权状态");
      if (session?.token) {
        refreshAccount();
      }
    } else if (checkout === "cancelled") {
      setOperationStatus("支付已取消，订单还没有扣款");
    }
  }, [session?.token]);

  useEffect(() => {
    if (authMode === "register" && authMethod === "nebula") {
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
    setTurnstileStatus("正在加载安全验证");
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
              setTurnstileStatus("安全验证已通过");
            },
            "expired-callback": () => {
              turnstileTokenRef.current = "";
              setTurnstileToken("");
              setTurnstileStatus("安全验证已过期，请重新验证");
            },
            "error-callback": (error) => {
              turnstileTokenRef.current = "";
              setTurnstileToken("");
              setTurnstileStatus(error ? `安全验证失败：${error}` : "安全验证失败，请重试");
            }
          });
        } catch (error) {
          setTurnstileStatus(`安全验证组件渲染失败：${errorMessage(error)}`);
          return;
        }
        setTurnstileStatus("安全验证组件已加载；完成验证后可提交");
        window.setTimeout(() => {
          if (!cancelled && !turnstileTokenRef.current) {
            setTurnstileStatus("如果没有看到安全验证，请刷新页面或联系运营支持");
          }
        }, 4_000);
      })
      .catch((error) => {
        if (!cancelled) {
          setTurnstileStatus(`安全验证组件加载失败：${errorMessage(error)}`);
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
        <a className="brand-mark" href="#top" aria-label="Ozon Rust Suite">
          <span className="brand-icon">
            <Orbit size={18} />
          </span>
          <span>Ozon Rust Suite</span>
        </a>
        <nav aria-label="primary navigation">
          {session ? (
            <>
              <a href="#console">接入向导</a>
              <a href="/customer-guide.html">操作说明</a>
              <a href="#advanced">排查</a>
            </>
          ) : (
            <>
              <a href="#capabilities">功能</a>
              <a href="#workflow">流程</a>
              <a href="#pricing">方案</a>
              <a href="/customer-guide.html">操作说明</a>
            </>
          )}
        </nav>
        <div className="nav-actions">
          {session ? (
            <>
              <button className="quiet" disabled={authBusy} onClick={() => refreshAccount()}>
                <RefreshCcw size={18} /> 刷新
              </button>
              <button className="quiet" onClick={logout}>
                <LogOut size={18} /> 退出
              </button>
            </>
          ) : (
            <>
              <button className="quiet" onClick={() => openAuthDialog("login")}>
                <LogIn size={18} /> 登录
              </button>
              <button onClick={() => openAuthDialog("register")}>
                <UserPlus size={18} /> 注册
              </button>
            </>
          )}
        </div>
      </header>

      {!session && (
        <>
      <section className="hero-section motion-hero motion-stage-entry" id="top">
        <div className="hero-copy">
          <p className="eyebrow">Ozon Rust Suite</p>
          <h1>
            <span>商品图进来，</span>
            <span>海报成稿出去。</span>
          </h1>
          <p>
            读取真实商品图、标题和卖点，本机助手负责店铺授权，龙虾/Codex 负责成图。少填表，多看成稿。
          </p>
          <div className="hero-actions">
            {session ? (
              <>
                <a className="download" href="#console">
                  <ArrowRight size={18} /> 继续接入流程
                </a>
                {canOpenLocalConsole && (
                  <a className="download secondary" href={LOCAL_CONSOLE_URL} target="_blank" rel="noreferrer">
                    <MonitorCheck size={18} /> 打开工作台
                  </a>
                )}
                <button className="secondary" disabled={authBusy} onClick={() => refreshAccount()}>
                  <RefreshCcw size={18} /> 刷新状态
                </button>
              </>
            ) : (
              <>
                <button onClick={() => openAuthDialog("login")}>
                  <LogIn size={18} /> 登录工作台
                </button>
                <button className="secondary" onClick={() => openAuthDialog("register")}>
                  <UserPlus size={18} /> 创建账号
                </button>
              </>
            )}
          </div>
          <div className="hero-meta">
            <span>邮箱/手机号登录</span>
            <span>真实商品读取</span>
            <span>龙虾/Codex 出图</span>
          </div>
        </div>
        <div className="hero-visual motion-card-flow" aria-label="商品海报生成预览">
          <div className="showcase-toolbar">
            <span>Live product brief</span>
            <strong>Ozon item #3169219</strong>
          </div>
          <div className="poster-stage">
            <article className="poster-card product-card">
              <span className="poster-label">商品原图</span>
              <div className="product-photo">
                <span className="lighter-shape" />
              </div>
              <strong>车系打火机</strong>
              <p>紫色车贴 · 金属喷嘴 · 随身款</p>
            </article>
            <article className="poster-card output-card">
              <span className="poster-label">海报成稿</span>
              <div className="poster-art">
                <span className="poster-product" />
                <span className="poster-road" />
              </div>
              <strong>时尚车系打火机</strong>
              <p>点亮风格，随身有行</p>
            </article>
            <article className="poster-card brief-card">
              <span className="poster-label">生成 brief</span>
              <ul>
                <li>保留商品颜色和车系元素</li>
                <li>突出便携、防风和礼品感</li>
                <li>禁止改品牌、改外观、乱写参数</li>
              </ul>
            </article>
          </div>
          <div className="template-marquee" aria-hidden="true">
            <div>
              <span>新品首图</span>
              <span>节日促销</span>
              <span>车品风格</span>
              <span>黑金质感</span>
              <span>俄语卖点</span>
              <span>竖版社媒</span>
              <span>商品对比</span>
              <span>新品首图</span>
              <span>节日促销</span>
              <span>车品风格</span>
              <span>黑金质感</span>
              <span>俄语卖点</span>
              <span>竖版社媒</span>
              <span>商品对比</span>
            </div>
          </div>
        </div>
      </section>

      <section className="capability-band motion-reveal" data-reveal id="capabilities">
        <div className="band-title">
          <p className="eyebrow">能做什么</p>
          <h2>先把商品拿准，再谈生成效果。</h2>
        </div>
        <div className="capability-grid">
          <article>
            <PackageCheck />
            <h3>店铺商品是真实来源</h3>
            <p>读取 Ozon 商品详情和图片，海报从真实资料开始，不让模型凭空编。</p>
          </article>
          <article>
            <Boxes />
            <h3>电脑助手保管授权</h3>
            <p>店铺密钥留在本机，网页只看连接状态，断在哪一步就显示哪一步。</p>
          </article>
          <article>
            <Bot />
            <h3>成稿要能复核</h3>
            <p>生成后检查商品外观、卖点和文字，明显跑偏就不当成成功。</p>
          </article>
        </div>
      </section>

      <section className="workflow-band motion-reveal" data-reveal id="workflow">
        <div className="workflow-copy">
          <p className="eyebrow">上手路径</p>
          <h2>用户只需要顺着下一步走。</h2>
          <p>
            默认路径保留给新手，排查信息收起来。客服需要定位问题时，再看版本、节点和授权状态。
          </p>
        </div>
        <div className="workflow-steps">
          <div>
            <span>01</span>
            <strong>登录账号</strong>
            <p>优先使用邮箱或手机号，不把普通用户送去额外安全页。</p>
          </div>
          <div>
            <span>02</span>
            <strong>安装电脑助手</strong>
            <p>电脑助手负责保存店铺授权信息，网页不会保存你的店铺密钥。</p>
          </div>
          <div>
            <span>03</span>
            <strong>连接这台电脑</strong>
            <p>打开电脑助手后，网页会确认它是否已经在运行。</p>
          </div>
          <div>
            <span>04</span>
            <strong>打开工作台</strong>
            <p>在工作台检查店铺授权，读取真实商品，并生成不乱写卖点的海报。</p>
          </div>
        </div>
      </section>
        </>
      )}

      {session && (
        <section className="account-console account-console-simple" id="console">
          <aside className="identity-panel">
            <p className="eyebrow">已登录</p>
            <h2>{displayLoginAlias(session.user)}</h2>
            <div className="rail-status">
              <span>服务状态</span>
              <strong>{activeEntitlement ? "已开通" : order ? "申请处理中" : "未开通"}</strong>
            </div>
            <div className="rail-status">
              <span>电脑助手</span>
              <strong>{computerHelperOnline ? "已连接" : "未连接"}</strong>
            </div>
            <div className="rail-status">
              <span>电脑授权</span>
              <strong>{computerAuthorized ? "已完成" : "未完成"}</strong>
            </div>
            <div className="rail-status">
              <span>店铺授权</span>
              <strong>{storeCredentialsReady ? "已保存" : readyForWorkspace ? "待填写" : "未开始"}</strong>
            </div>
          </aside>

          <section className="workspace">
            {LOCAL_DEV_AUTH_ENABLED && (
            <details className="local-dev-panel">
              <summary>开发调试</summary>
              <div className="auth-strip local-dev-strip">
                <div className="section-title">
                  <Orbit />
                  <div>
                    <h2>Nebula access_token</h2>
                    <p>仅用于开发诊断：用已登录的 Nebula 会话换取 Ozon 服务会话。</p>
                  </div>
                </div>
              </div>
              <div className="form-grid skybridge-grid">
                <label>
                  Nebula access_token
                  <input
                    autoComplete="off"
                    placeholder="从 Nebula 开发环境获取"
                    type="password"
                    value={skybridgeAccessToken}
                    onChange={(event) => setSkybridgeAccessToken(event.target.value)}
                  />
                </label>
                <button disabled={authBusy} onClick={createManualSkybridgeServiceSession}>
                  <Orbit size={18} /> 创建服务会话
                </button>
              </div>

              <div className="auth-strip local-dev-strip">
                <div className="section-title">
                  <ShieldCheck />
                  <div>
                    <h2>local_dev 账户</h2>
                    <p>仅用于离线调试；正式用户必须通过 Nebula，身份来源应显示 Nebula。</p>
                  </div>
                </div>
                <div className="mode-switch" aria-label="local auth mode">
                  <button className={localMode === "register" ? "active" : ""} onClick={() => setLocalMode("register")}>
                    <UserPlus size={18} /> 注册
                  </button>
                  <button className={localMode === "login" ? "active" : ""} onClick={() => setLocalMode("login")}>
                    <LogIn size={18} /> 登录
                  </button>
                </div>
              </div>

              <div className="method-switch" aria-label="local login method">
                <button className={localMethod === "email" ? "active" : ""} onClick={() => setLocalMethod("email")}>
                  <Mail size={18} /> 邮箱
                </button>
                <button className={localMethod === "phone" ? "active" : ""} onClick={() => setLocalMethod("phone")}>
                  <Smartphone size={18} /> 手机号
                </button>
                {localMode === "login" && (
                  <button className={localMethod === "nebula" ? "active" : ""} onClick={() => setLocalMethod("nebula")}>
                    <KeyRound size={18} /> 账号编号
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
                    昵称
                    <input
                      autoComplete="name"
                      placeholder="Ozon operator"
                      value={localName}
                      onChange={(event) => setLocalName(event.target.value)}
                    />
                  </label>
                )}
                <label>
                  本地密码
                  <input
                    autoComplete={localMode === "register" ? "new-password" : "current-password"}
                    placeholder="仅 local_dev 使用"
                    value={localPassword}
                    onChange={(event) => setLocalPassword(event.target.value)}
                    type="password"
                  />
                </label>
                <button disabled={authBusy} onClick={authenticateLocalDev}>
                  {localMode === "register" ? <UserPlus size={18} /> : <LogIn size={18} />}
                  {localMode === "register" ? "创建 local_dev" : "登录 local_dev"}
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
                <span>下一步</span>
                <h2>{setupStatus.title}</h2>
                <p>{setupStatus.message}</p>
              </div>
              <div className="setup-actions">
                {!activeEntitlement && order && (
                  <>
                    <button disabled={authBusy} onClick={refreshOrder}>
                      <RefreshCcw size={18} /> 刷新开通状态
                    </button>
                    <button className="secondary" onClick={copyOrderInfo}>
                      复制申请信息
                    </button>
                  </>
                )}
                {!activeEntitlement && !order && (
                  <button disabled={!canUseProtectedActions} onClick={createOrder}>
                    <ArrowRight size={18} /> 开通服务
                  </button>
                )}
                {activeEntitlement && !computerHelperOnline && primaryDownloadOption && (
                  <a className="download" href={primaryDownloadOption.url}>
                    <Download size={18} /> {primaryDownloadOption.shortLabel}
                  </a>
                )}
                {activeEntitlement && !computerHelperOnline && (
                  <button className="secondary" disabled={localNode.phase === "checking"} onClick={probeLocalNode}>
                    <RefreshCcw size={18} /> 我已打开，检测一下
                  </button>
                )}
                {activeEntitlement && computerHelperOnline && !device && !computerAuthorized && (
                  <button disabled={!canBindLocalDevice} onClick={activateDevice}>
                    <MonitorCheck size={18} /> 授权这台电脑
                  </button>
                )}
                {activeEntitlement && device && !computerAuthorized && (
                  <button onClick={issueLease} disabled={authBusy}>
                    <Radio size={18} /> 完成电脑授权
                  </button>
                )}
                {canStartWorkspace && (
                  <a className="download" href={LOCAL_CONSOLE_URL} target="_blank" rel="noreferrer">
                    <MonitorCheck size={18} /> 打开工作台
                  </a>
                )}
                {readyForWorkspace && !canOpenLocalConsole && (
                  <span className="inline-next-step">
                    <MonitorCheck size={18} /> 去电脑上的 Ozon Rust Local 继续
                  </span>
                )}
              </div>
            </div>

            <div className="wizard-steps" aria-label="接入步骤">
              <article
                className={wizardStepClass(Boolean(activeEntitlement), currentSetupStep === 1)}
                aria-current={currentSetupStep === 1 ? "step" : undefined}
              >
                <span className="step-number">1</span>
                <div>
                  <h3>开通服务</h3>
                  <p>
                    {activeEntitlement
                      ? "服务已开通，可以继续连接电脑。"
                      : order
                        ? "申请已经提交。付款或客服确认后，点刷新查看结果。"
                        : "先开通服务，后面才能连接电脑和读取商品。"}
                  </p>
                  {!activeEntitlement && (
                    <div className="step-actions">
                      {order ? (
                        <>
                          <button disabled={authBusy} onClick={refreshOrder}>
                            <RefreshCcw size={18} /> 刷新开通状态
                          </button>
                          <button className="secondary" onClick={copyOrderInfo}>
                            复制申请信息
                          </button>
                        </>
                      ) : (
                        <button disabled={!canUseProtectedActions} onClick={createOrder}>
                          <ArrowRight size={18} /> 开通服务
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
                  <h3>安装并打开电脑助手</h3>
                  <p>
                    {computerHelperOnline
                      ? "电脑助手已经打开。"
                      : activeEntitlement
                        ? "下载安装到这台电脑，打开后回到这里点“检测一下”。"
                        : "服务开通后，这里会给你下载入口。"}
                  </p>
                  {activeEntitlement && !computerHelperOnline && (
                    <div className="step-actions">
                      {primaryDownloadOption ? (
                        <a className="download" href={primaryDownloadOption.url}>
                          <Download size={18} /> {primaryDownloadOption.shortLabel}
                        </a>
                      ) : (
                        <button className="secondary" disabled>
                          <Download size={18} /> 安装包准备中
                        </button>
                      )}
                      <button disabled={localNode.phase === "checking"} onClick={probeLocalNode}>
                        <RefreshCcw size={18} /> 我已打开，检测一下
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
                  <h3>授权这台电脑</h3>
                  <p>
                    {computerAuthorized
                      ? "这台电脑已经可以使用你的服务。"
                      : "只允许已授权的电脑读取商品，并把任务交给龙虾/Codex。"}
                  </p>
                  {activeEntitlement && computerHelperOnline && !computerAuthorized && (
                    <div className="step-actions">
                      {!device ? (
                        <button disabled={!canBindLocalDevice} onClick={activateDevice}>
                          <MonitorCheck size={18} /> 授权这台电脑
                        </button>
                      ) : (
                        <button disabled={authBusy} onClick={issueLease}>
                          <Radio size={18} /> 完成授权
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
                  <h3>{storeCredentialsReady ? "开始读取商品" : "连接 Ozon 店铺"}</h3>
                  <p>
                    {!readyForWorkspace
                      ? "前面几步完成后，这里会告诉你去哪里填写店铺授权。"
                      : storeCredentialsReady
                        ? "店铺授权已经保存。打开工作台读取商品，再把海报任务复制给龙虾/Codex。"
                        : "切到电脑上的 Ozon Rust Local，在“店铺授权”里填写 Ozon Client ID 和 API Key，保存后回这里刷新。"}
                  </p>
                  {readyForWorkspace && (
                    <div className="step-actions">
                      {canStartWorkspace ? (
                        <a className="download" href={LOCAL_CONSOLE_URL} target="_blank" rel="noreferrer">
                          <MonitorCheck size={18} /> 打开工作台
                        </a>
                      ) : (
                        <span className="inline-next-step">
                          <MonitorCheck size={18} /> 请打开电脑上的 Ozon Rust Local
                        </span>
                      )}
                      <button className="secondary" disabled={localNode.phase === "checking"} onClick={probeLocalNode}>
                        <RefreshCcw size={18} /> 我已处理，刷新状态
                      </button>
                    </div>
                  )}
                </div>
              </article>
            </div>

            {readyForWorkspace && (
              <div className={`handoff-card ${storeCredentialsReady ? "online" : "warn"}`}>
                <div>
                  <span>{storeCredentialsReady ? "店铺已连好" : "现在去电脑助手"}</span>
                  <h3>{storeCredentialsReady ? "可以读取真实商品了" : "把 Ozon API 填到电脑助手里"}</h3>
                  <p>
                    {storeCredentialsReady
                      ? "下一步在 Ozon Rust Local 里读取商品，点“复制给龙虾/Codex”。图片 API 只是自动后台出图的可选项。"
                      : "网页已经确认这台电脑能用。接下来不是在网页里填密钥，而是在电脑助手里保存店铺授权，这样密钥只留在你的电脑上。"}
                  </p>
                </div>
                <div className="handoff-actions">
                  <button className="secondary" disabled={localNode.phase === "checking"} onClick={probeLocalNode}>
                    <RefreshCcw size={18} /> 刷新检查
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
                        <strong>微信扫码支付</strong>
                        <p>
                          支付备注：{paymentSession.payment_reference} ·{" "}
                          {formatMoney(paymentSession.amount_minor, paymentSession.currency)}
                        </p>
                      </div>
                    </div>
                  )}
                  {paymentSession.checkout_url && (
                    <a href={paymentSession.checkout_url}>
                      <ExternalLink size={16} /> 打开支付页
                    </a>
                  )}
                </div>
              </div>
            )}

            <details className="op-section support-section">
              <summary>
                <KeyRound size={20} />
                <div>
                  <strong>客服给了开通码？</strong>
                  <span>有开通码时再打开这里。</span>
                </div>
              </summary>
              <div className="form-grid">
                <label>
                  开通码
                  <input value={cardKey} onChange={(event) => setCardKey(event.target.value)} placeholder="ORS-..." />
                </label>
                <label>
                  服务状态
                  <input value={activeEntitlement ? "已开通" : "未开通"} readOnly />
                </label>
                <label>
                  有效期
                  <input value={activeEntitlement?.expires_at ?? "开通后显示"} readOnly />
                </label>
              </div>
              <div className="command-row">
                <button disabled={!canUseProtectedActions} onClick={redeem}>
                  <KeyRound size={18} /> 使用开通码
                </button>
              </div>
            </details>

            <details className="op-section support-section">
              <summary>
                <Clipboard size={20} />
                <div>
                  <strong>申请详情</strong>
                  <span>需要发给客服或核对付款时再打开。</span>
                </div>
              </summary>
              {order ? (
                <div className="form-grid">
                  <label>
                    申请编号
                    <input value={order.id} readOnly />
                  </label>
                  <label>
                    支付备注
                    <input value={order.payment_reference} readOnly />
                  </label>
                  <label>
                    状态
                    <input value={orderStatusLabel(order.status)} readOnly />
                  </label>
                  <label>
                    通道
                    <input value={paymentProviderLabel(order.payment_provider)} readOnly />
                  </label>
                  <label>
                    金额
                    <input value={formatMoney(order.amount_minor, order.currency)} readOnly />
                  </label>
                </div>
              ) : (
                <p className="section-hint">还没有申请记录。点“开通服务”后，这里会显示申请编号和付款备注。</p>
              )}
              <div className="command-row">
                <button className="secondary" onClick={copyOrderInfo} disabled={!order}>
                  复制申请信息
                </button>
                <button className="secondary" onClick={refreshOrder} disabled={!order || authBusy}>
                  <RefreshCcw size={18} /> 刷新状态
                </button>
              </div>
            </details>

            <details className="op-section support-section local-node-section" id="advanced">
              <summary>
                <MonitorCheck size={20} />
                <div>
                  <strong>排查用信息</strong>
                  <span>一般不用看；客服排查安装或插件问题时再打开。</span>
                </div>
              </summary>
              <div className="local-node-grid">
                <div className={`local-node-card ${localNode.phase}`}>
                  <span>电脑助手</span>
                  <strong>{localNodeStatusLabel(localNode.phase)}</strong>
                  <p>{localNode.message}</p>
                </div>
                <div className={`local-node-card ${localPairingStatus.kind}`}>
                  <span>电脑授权</span>
                  <strong>{localPairingStatus.title}</strong>
                  <p>{localPairingStatus.message}</p>
                </div>
                <div className={`local-node-card ${storeCredentialsReady ? "online" : "warn"}`}>
                  <span>店铺授权</span>
                  <strong>{storeCredentialsReady ? "已保存" : "待填写"}</strong>
                  <p>
                    {storeCredentialsReady
                      ? "电脑助手已保存 Ozon 店铺授权，可以读取商品。"
                      : "切到 Ozon Rust Local，在店铺授权里填写 Client ID 和 API Key。"}
                  </p>
                </div>
                <div className={`local-node-card ${posterConfigReady ? "online" : "warn"}`}>
                  <span>API 自动出图</span>
                  <strong>{posterConfigReady ? "已配置" : "可选"}</strong>
                  <p>
                    {posterConfigReady
                      ? `图片 API 已保存：${localNode.portal?.openai?.image_model ?? "当前模型"}。`
                      : "默认用龙虾/Codex 出图；只有需要后台自动生成时才配置图片 API。"}
                  </p>
                </div>
                <div className="local-node-card">
                  <span>安装包版本</span>
                  <strong>{releaseVersion}</strong>
                  <p>{downloads?.release_manifest_url ?? "等待下载信息同步"}</p>
                </div>
                <div className="local-node-card">
                  <span>发布校验</span>
                  <strong>{releaseCommit}</strong>
                  <p>{releaseChecksum}</p>
                </div>
              </div>
              <div className="command-row">
                <button disabled={localNode.phase === "checking"} onClick={probeLocalNode}>
                  <RefreshCcw size={18} /> 检测连接
                </button>
                {localNodeDownloadOptions.length === 0 && (
                  <button className="secondary" disabled>
                    <Download size={18} /> 安装包准备中
                  </button>
                )}
                {localNodeDownloadOptions.map((option, index) => (
                  <a className={`download ${index > 0 ? "secondary" : ""}`} href={option.url} key={option.key}>
                    <Download size={18} /> {option.label}
                  </a>
                ))}
                {canOpenLocalConsole && (
                  <a className="download secondary" href={LOCAL_CONSOLE_URL} target="_blank" rel="noreferrer">
                    <MonitorCheck size={18} /> 打开工作台
                  </a>
                )}
                {canDownloadOpenClawPlugin && (
                  <a className="download secondary" href={openclawPluginUrl} target="_blank" rel="noreferrer">
                    <Download size={18} /> 插件安装包
                  </a>
                )}
                <button className="secondary" disabled={!canCopyLocalManifest} onClick={copyLocalManifestUrl}>
                  <Clipboard size={18} /> 复制插件连接地址
                </button>
              </div>
              {localNode.manifest && (
                <div className="manifest-tools">
                  {localNode.manifest.tools.map((tool) => (
                    <div key={tool.name}>
                      <span>{tool.risk}</span>
                      <strong>{tool.name}</strong>
                      <em>{tool.approval_required ? "需要确认" : "只读"}</em>
                    </div>
                  ))}
                </div>
              )}
            </details>

            <details className="op-section support-section">
              <summary>
                <ShieldCheck size={20} />
                <div>
                  <strong>更换电脑或改名称</strong>
                  <span>需要改设备名、查看授权时间时再打开。</span>
                </div>
              </summary>
              <div className="form-grid">
                <label>
                  设备名
                  <input value={deviceName} onChange={(event) => setDeviceName(event.target.value)} />
                </label>
                <label>
                  设备码
                  <input
                    value={deviceFingerprint}
                    readOnly
                    placeholder="连接电脑助手后自动生成"
                    title="设备码由电脑助手生成，门户不允许手写伪造"
                  />
                </label>
                <label>
                  授权状态
                  <input value={computerAuthorized ? "已完成" : device ? "还差最后一步" : "未授权"} readOnly />
                </label>
              </div>
              <div className="command-row">
                <button disabled={!canBindLocalDevice} onClick={activateDevice}>
                  <MonitorCheck size={18} /> 授权这台电脑
                </button>
                <button className="secondary" onClick={issueLease} disabled={!device || computerAuthorized || authBusy}>
                  <Radio size={18} /> 完成授权
                </button>
              </div>
              {lease && (
                <div className="lease-line">
                  <span>授权</span>
                  <strong>{lease.lease_id}</strong>
                  <em>有效期至 {lease.expires_at}</em>
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
          <p className="eyebrow">开始接入</p>
          <h2>进入工作台，按步骤完成 Ozon 商品读取。</h2>
          <p>登录后页面会告诉你下一步该点什么：开通服务、安装电脑助手、连接这台电脑，然后开始读取商品。</p>
        </div>
        {session ? (
          <a className="download" href="#console">
            <ArrowRight size={18} /> 进入账户授权
          </a>
        ) : (
          <button onClick={() => openAuthDialog("register")}>
            <UserPlus size={18} /> 开始使用
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
            <button className="icon-button close-button" aria-label="关闭登录面板" onClick={() => setAuthDialogMode(null)}>
              <X size={18} />
            </button>
            <div className="dialog-heading">
              <p className="eyebrow">Ozon Rust Suite</p>
              <h2 id="auth-dialog-title">{authDialogTitle}</h2>
              <p>{authDialogDescription}</p>
            </div>

            <div className="auth-context-line">
              <ShieldCheck size={17} />
              <span>
                {DIRECT_SKYBRIDGE_AUTH_ENABLED
                  ? "默认使用邮箱或手机号登录；统一身份入口只作为企业账号备用。"
                  : NEBULA_OAUTH_CONFIGURED
                    ? "当前只配置了企业统一身份入口；如果安全验证失败，请联系运营开通邮箱/手机号入口。"
                    : "账号服务正在维护，请联系运营支持。"}
              </span>
            </div>

            {DIRECT_SKYBRIDGE_AUTH_ENABLED && (
              <section className="direct-auth-panel">
                <div className="section-title compact-title">
                  {authMethod === "phone" ? <Smartphone /> : <Mail />}
                  <div>
                    <h2>{authMode === "register" ? "创建账号" : "邮箱/手机号登录"}</h2>
                    <p>不跳出当前门户。登录后继续开通服务、安装电脑助手并读取商品。</p>
                  </div>
                </div>
                <form className="form-grid skybridge-auth-grid auth-card-form" onSubmit={handleSkybridgePasswordSubmit}>
                  <div className="method-switch auth-methods" aria-label="账号登录方式">
                    <button className={authMethod === "email" ? "active" : ""} type="button" onClick={() => setAuthMethod("email")}>
                      <Mail size={18} /> 邮箱
                    </button>
                    <button className={authMethod === "phone" ? "active" : ""} type="button" onClick={() => setAuthMethod("phone")}>
                      <Smartphone size={18} /> 手机号
                    </button>
                    {authMode === "login" && (
                      <button className={authMethod === "nebula" ? "active" : ""} type="button" onClick={() => setAuthMethod("nebula")}>
                        <KeyRound size={18} /> 账号编号
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
                      昵称
                      <input
                        autoComplete="name"
                        placeholder="姓名或团队昵称"
                        value={skybridgeName}
                        onChange={(event) => setSkybridgeName(event.target.value)}
                      />
                    </label>
                  )}
                  {authMode === "register" && authMethod === "phone" && (
                    <label>
                      备用邮箱
                      <input
                        autoComplete="email"
                        placeholder="用于接收账号通知"
                        value={skybridgePhoneEmail}
                        onChange={(event) => setSkybridgePhoneEmail(event.target.value)}
                      />
                    </label>
                  )}
                  {authMethod === "phone" ? (
                    <label>
                      短信验证码
                      <input
                        autoComplete="one-time-code"
                        placeholder="请输入验证码"
                        value={skybridgePhoneCode}
                        onChange={(event) => setSkybridgePhoneCode(event.target.value)}
                      />
                    </label>
                  ) : (
                    <label>
                      密码
                      <input
                        autoComplete={authMode === "register" ? "new-password" : "current-password"}
                        placeholder="请输入密码"
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
                      <Smartphone size={18} /> 获取验证码
                    </button>
                  )}
                  <button disabled={authBusy || directAuthNeedsTurnstile} type="submit">
                    {authMode === "register" ? <UserPlus size={18} /> : <LogIn size={18} />}
                    {authSubmitText}
                  </button>
                </form>

                {skybridgeOtpStatus && <p className="identity-note">{skybridgeOtpStatus}</p>}

                {SKYBRIDGE_TURNSTILE_CONFIGURED && (
                  <div className="turnstile-row">
                    <div ref={turnstileContainerRef} />
                    <span>{turnstileStatus}</span>
                  </div>
                )}

                {directAuthNeedsTurnstile && (
                  <p className="identity-note">当前登录需要先完成安全验证，验证通过后再提交。</p>
                )}
              </section>
            )}

            {!DIRECT_SKYBRIDGE_AUTH_ENABLED && (
              <p className="identity-note">{directAuthUnavailableMessage}</p>
            )}

            {NEBULA_OAUTH_ENTRY_ENABLED && (
              <details className="compat-panel sso-panel">
                <summary>企业统一身份入口</summary>
                <section className="nebula-oauth-panel">
                  <div className="section-title compact-title">
                    <ShieldCheck />
                    <div>
                      <h2>统一身份{authMode === "register" ? "注册" : "登录"}</h2>
                      <p>仅在企业账号或客服要求时使用；打开后会离开当前页面完成验证。</p>
                    </div>
                  </div>
                  <div className="oauth-actions">
                    <button disabled={authBusy || !NEBULA_OAUTH_CONFIGURED} onClick={() => startNebulaOAuth(authMode)}>
                      <ExternalLink size={18} /> {authMode === "register" ? "打开统一注册页" : "打开统一登录页"}
                    </button>
                  </div>
                </section>
              </details>
            )}

            {captchaBlocked && (
              <div className="captcha-callout">
                <AlertCircle />
                <div>
                  <strong>这次登录还差安全验证</strong>
                  <p>
                    账号服务要求先完成验证码或二次确认。请刷新后重试；如果仍失败，请联系运营处理账号风控。
                  </p>
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
              {authMode === "login" ? "还没有账号？" : "已有账号？"}
              <button type="button" onClick={() => switchAuthMode(authMode === "login" ? "register" : "login")}>
                {authMode === "login" ? "创建账号" : "登录"}
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

function loadSession(): Session | null {
  try {
    const raw = localStorage.getItem(SESSION_KEY);
    return raw ? (JSON.parse(raw) as Session) : null;
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
      throw new Error("请求超时，请检查账号服务或 Ozon 服务是否可访问");
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
  const mode = health.real_ozon_enabled ? "真实 Ozon 商品读取已开启" : "开发模式";
  const version = health.package_version ? ` v${health.package_version}` : "";
  if (portalError) {
    return `电脑助手${version}已打开，${mode}；${userFacingLocalNodeIssue(portalError)}`;
  }
  return `电脑助手${version}已连接，${mode}`;
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
  if (endpoint === "health") {
    if (isLocalNodeBrowserBlock(error)) {
      return "网页没有找到电脑助手。请先打开电脑助手；如果已经打开，安装最新版后再试。";
    }
    return "电脑助手没有回应。请确认已打开，或重启电脑助手后再点检测。";
  }
  if (endpoint === "manifest") {
    return "电脑助手已打开，但连接信息读取失败。请重启电脑助手，仍不行就安装最新版。";
  }
  if (isLocalNodeBrowserBlock(error)) {
    return "网页没有找到电脑助手。请打开电脑助手；如果还是不行，安装最新版后再点检测。";
  }
  return "未检测到电脑助手。请确认已打开，或安装最新版后再点检测。";
}

function isLocalNodeBrowserBlock(error: unknown) {
  const message = errorMessage(error).toLowerCase();
  return message.includes("failed to fetch") || message.includes("networkerror") || message.includes("load failed");
}

function localNodeStatusLabel(phase: LocalNodePhase) {
  switch (phase) {
    case "checking":
      return "检测中";
    case "online":
      return "已在线";
    case "degraded":
      return "部分可用";
    case "blocked":
      return "未连上";
    case "offline":
      return "未启动";
    default:
      return "待检测";
  }
}

function confirmedOrderMessage(order: Order) {
  if (order.payment_provider === "manual") {
    return "申请已人工确认。请使用运营发送的开通码完成开通。";
  }
  return "开通已确认，刷新账户后即可继续下一步";
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
  switch (status) {
    case "confirmed":
      return "已开通";
    case "pending_manual_payment":
      return "等待付款确认";
    case "pending":
      return "处理中";
    case "cancelled":
      return "已取消";
    default:
      return status;
  }
}

function paymentProviderLabel(provider: string) {
  switch (provider) {
    case "manual":
      return "人工确认";
    case "wechat":
      return "微信支付";
    case "stripe":
      return "Stripe";
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
  if (!entitlement) {
    return {
      kind: "warn",
      title: "服务未开通",
      message: "先开通服务或输入开通码，才能连接店铺并读取商品。"
    };
  }
  if (!device) {
    if (localNode.phase !== "online") {
      return {
        kind: "warn",
        title: "先连接电脑助手",
        message: "服务已开通。打开电脑助手并检测成功后，就能授权这台电脑。"
      };
    }
    return {
      kind: "warn",
      title: "还没授权这台电脑",
      message: "点“授权这台电脑”，把当前电脑加入你的账号。"
    };
  }
  if (!leaseStatus?.valid) {
    const issue = userFacingLocalLeaseIssue(leaseStatus?.issue);
    return {
      kind: localNode.phase === "online" ? "warn" : "offline",
      title: cloudLeaseIssued ? "电脑助手还没保存授权" : "还差最后一步",
      message: issue ?? "再点一次“完成授权”，电脑助手就能开始工作。"
    };
  }
  return {
    kind: localNode.phase === "online" ? "online" : "warn",
    title: localNode.phase === "online" ? "这台电脑已授权" : "电脑授权已保存",
    message: `有效期至 ${leaseStatus.expires_at ?? "授权到期前"}。`
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
  const nodeOnline = input.localNode.phase === "online";
  if (!input.activeEntitlement) {
    return {
      kind: "warn",
      title: input.order ? "等待服务开通" : "先开通服务",
      message: input.order
        ? "申请已经提交。付款或客服确认后，点“刷新开通状态”。"
        : "开通后按提示安装电脑助手，再授权这台电脑。"
    };
  }
  if (!nodeOnline) {
    return {
      kind: "warn",
      title: "安装并打开电脑助手",
      message: "下载电脑助手，打开后回到这里点“检测一下”。"
    };
  }
  if (input.computerAuthorized) {
    if (!input.storeCredentialsReady) {
      return {
        kind: "warn",
        title: "去电脑助手填写店铺 API",
        message: "这台电脑已经授权。现在切到 Ozon Rust Local，在店铺授权里保存 Ozon Client ID 和 API Key。"
      };
    }
    return {
      kind: "online",
      title: "可以开始了",
      message: "服务、电脑和店铺授权都准备好了。打开工作台读取商品，再把海报任务交给龙虾/Codex。"
    };
  }
  if (!input.device) {
    return {
      kind: "warn",
      title: "授权这台电脑",
      message: "服务已开通，电脑助手也在线。现在把这台电脑加入账号。"
    };
  }
  if (!input.computerAuthorized) {
    const issue = userFacingLocalLeaseIssue(input.localLeaseIssue);
    return {
      kind: "warn",
      title: issue ? "电脑授权没有完成" : "完成电脑授权",
      message: issue ?? "再确认一次授权，电脑助手就可以读取商品并交给龙虾/Codex。"
    };
  }
  return {
    kind: "online",
    title: "可以开始了",
    message: "服务、电脑和店铺授权都准备好了。打开工作台读取商品，再交给龙虾/Codex 出图。"
  };
}

function userFacingLocalLeaseIssue(issue?: string | null) {
  if (!issue || issue === "cloud lease is not installed") return null;
  if (isLocalNodeBrowserBlock(new Error(issue))) {
    return "网页没有连上电脑助手。请先打开电脑助手，再点“我已打开，检测一下”。";
  }
  if (issue.includes("public key") || issue.includes("signature")) {
    return "你现在打开的是旧版电脑助手。请下载安装最新版，打开后再点“完成授权”。";
  }
  if (issue.includes("expired")) {
    return "电脑授权已过期，请重新完成授权。";
  }
  if (issue.includes("stored cloud lease is invalid")) {
    return "电脑里的授权记录已失效，请重新点“完成授权”。";
  }
  return "电脑助手没有保存授权。请重启电脑助手，仍不行就安装最新版后再试。";
}

function localLeaseWriteFailureMessage(error: unknown) {
  const issue = userFacingLocalLeaseIssue(errorMessage(error));
  return issue ?? "电脑助手没有保存授权。请重启电脑助手，仍不行就安装最新版后再试。";
}

function userFacingLocalNodeIssue(error: unknown) {
  const issue = userFacingLocalLeaseIssue(stripLocalNodeEndpointPrefix(errorMessage(error)));
  return issue ?? "账户授权状态读取失败。请重启电脑助手，仍不行就安装最新版。";
}

function statusMessageTone(message: string, phase: AuthPhase, busy: boolean) {
  if (busy) return "ok";
  if (phase === "failed") return "danger";
  if (/(失败|不能|没有找到|没有保存|读取失败|未检测|太旧|不完整|失效|过期)/.test(message)) {
    return "danger";
  }
  if (/(请|需要|等待|未开通|未启动|处理中)/.test(message)) {
    return "warn";
  }
  return "ok";
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
  const mac = macDmgUrl
    ? [{ key: "mac-dmg", label: "Mac 安装包", shortLabel: "下载 Mac 版", url: macDmgUrl }]
    : [];
  const windows = [
    ...(msiUrl ? [{ key: "windows-msi", label: "Windows 安装包", shortLabel: "下载 Windows 版", url: msiUrl }] : []),
    ...(exeUrl ? [{ key: "windows-exe", label: "Windows 备用安装包", shortLabel: "备用下载", url: exeUrl }] : [])
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
  throw new Error("VITE_CLOUD_API must be configured for this production portal host");
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
  if (!SKYBRIDGE_AUTH_CONFIGURED) {
    throw new Error("邮箱/手机号登录未配置");
  }
  if (input.mode === "register" && input.method === "nebula") {
    throw new Error("账号编号由系统分配，注册请使用邮箱或手机号");
  }
  if (input.method === "nebula") {
    return skybridgeNebulaLogin(input.identifier, input.password);
  }
  if (input.method === "phone") {
    return skybridgePhoneOtpLogin(input.identifier, input.phoneCode ?? "");
  }

  const endpoint =
    input.mode === "register"
      ? `${SKYBRIDGE_AUTH_BASE}/auth/v1/signup`
      : `${SKYBRIDGE_AUTH_BASE}/auth/v1/token?grant_type=password`;
  const body = {
    email: input.identifier.trim(),
    password: input.password,
    data: skybridgeMetadata(input.name, "email"),
    gotrue_meta_security: skybridgeCaptchaMetadata(input.captchaToken)
  };
  const response = await fetchWithTimeout(endpoint, {
    method: "POST",
    headers: skybridgeAuthHeaders(),
    body: JSON.stringify(body)
  });
  const result = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(skybridgeErrorMessage(result, "账号认证失败"));
  }
  const session = extractSkybridgeSession(result);
  if (!session?.access_token) {
    throw new Error("身份服务已接受请求，请完成邮箱验证后再登录");
  }
  return session;
}

async function skybridgeSendPhoneOtp(input: {
  phone: string;
  mode: AuthMode;
  name: string;
  captchaToken?: string;
}) {
  if (!SKYBRIDGE_AUTH_CONFIGURED) {
    throw new Error("邮箱/手机号登录未配置");
  }

  const body: Record<string, unknown> = {
    phone: input.phone.trim(),
    gotrue_meta_security: skybridgeCaptchaMetadata(input.captchaToken)
  };
  if (input.mode === "register") {
    body.data = skybridgeMetadata(input.name, "phone");
  }

  const response = await fetchWithTimeout(`${SKYBRIDGE_AUTH_BASE}/auth/v1/otp`, {
    method: "POST",
    headers: skybridgeAuthHeaders(),
    body: JSON.stringify(body)
  });
  const result = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(skybridgeErrorMessage(result, "短信验证码发送失败"));
  }
}

async function skybridgePhoneOtpLogin(phone: string, token: string): Promise<SkybridgeAuthSession> {
  const response = await fetchWithTimeout(`${SKYBRIDGE_AUTH_BASE}/auth/v1/token`, {
    method: "POST",
    headers: skybridgeAuthHeaders(),
    body: JSON.stringify({
      phone: phone.trim(),
      token: token.trim(),
      type: "sms",
      grant_type: "otp"
    })
  });
  const result = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(skybridgeErrorMessage(result, "手机号验证码登录失败"));
  }
  const session = extractSkybridgeSession(result);
  if (!session?.access_token) {
    throw new Error("手机号验证码响应无效");
  }
  return session;
}

async function skybridgeUpdatePhoneRegistrationProfile(input: {
  accessToken: string;
  phone: string;
  email: string;
  name: string;
}) {
  const displayName = input.name.trim();
  const email = input.email.trim();
  if (!displayName || !email) {
    throw new Error("手机号注册需要填写昵称和联系邮箱");
  }

  const currentProfile = await skybridgeFetchCurrentUserProfile(input.accessToken);
  if (!shouldBootstrapSkybridgePhoneProfile(currentProfile)) {
    return;
  }

  const response = await fetchWithTimeout(`${SKYBRIDGE_AUTH_BASE}/auth/v1/user`, {
    method: "PUT",
    headers: {
      ...skybridgeAuthHeaders(),
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
    throw new Error(skybridgeErrorMessage(result, "手机号注册资料回写失败"));
  }
}

async function skybridgeFetchCurrentUserProfile(accessToken: string): Promise<SkybridgeCurrentUserProfile | null> {
  const response = await fetchWithTimeout(`${SKYBRIDGE_AUTH_BASE}/auth/v1/user`, {
    headers: {
      apikey: SKYBRIDGE_ANON_KEY,
      Authorization: `Bearer ${accessToken}`
    }
  });
  const result = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(skybridgeErrorMessage(result, "用户资料读取失败"));
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

async function skybridgeNebulaLogin(nebulaId: string, password: string): Promise<SkybridgeAuthSession> {
  const response = await fetchWithTimeout(`${SKYBRIDGE_AUTH_BASE}/functions/v1/nebula-login`, {
    method: "POST",
    headers: skybridgeAuthHeaders(),
    body: JSON.stringify({
      nebula_id: nebulaId.trim().toUpperCase(),
      password
    })
  });
  const result = await response.json().catch(() => null);
  if (!response.ok) {
    throw new Error(skybridgeErrorMessage(result, "账号编号登录失败"));
  }
  const session = extractSkybridgeSession(result);
  if (!session?.access_token) {
    throw new Error("账号编号登录响应无效");
  }
  return session;
}

async function redirectToNebulaOAuth(flow: NebulaOAuthFlow) {
  if (!NEBULA_OAUTH_CONFIGURED) {
    throw new Error("请先配置 VITE_NEBULA_BASE_URL 和 VITE_NEBULA_CLIENT_ID");
  }
  if (!crypto.getRandomValues || !crypto.subtle) {
    throw new Error("当前浏览器不支持 Web Crypto，无法启动 PKCE 授权");
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
    throw new Error(skybridgeErrorMessage(result, "网页登录失败"));
  }
  const session = extractSkybridgeSession(result);
  if (!session?.access_token) {
    throw new Error("网页登录返回信息无效");
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

function skybridgeAuthHeaders() {
  return {
    "Content-Type": "application/json",
    apikey: SKYBRIDGE_ANON_KEY,
    Authorization: `Bearer ${SKYBRIDGE_ANON_KEY}`
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

function stringFromMetadata(metadata: Record<string, unknown>, key: string) {
  const value = metadata[key];
  return typeof value === "string" ? value.trim() : undefined;
}

function isPlaceholderDisplayName(value: string | undefined) {
  const trimmed = value?.trim() ?? "";
  if (!trimmed) return true;
  return ["用户", "新用户", "访客", "Apple 用户", "Nebula 用户", "User", "New User", "Guest"].includes(trimmed);
}

function loadTurnstileScript() {
  if (!SKYBRIDGE_TURNSTILE_SCRIPT_URL) {
    return Promise.reject(new Error("安全验证脚本未配置"));
  }
  if (window.turnstile) {
    return Promise.resolve();
  }

  const existing = document.querySelector<HTMLScriptElement>('script[data-ozon-turnstile="true"]');
  if (existing) {
    return new Promise<void>((resolve, reject) => {
      existing.addEventListener("load", () => resolve(), { once: true });
      existing.addEventListener("error", () => reject(new Error("Cloudflare Turnstile 脚本加载失败")), {
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
    script.onerror = () => reject(new Error("Cloudflare Turnstile 脚本加载失败"));
    document.head.appendChild(script);
  });
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
  return "未绑定";
}

function displayNebulaId(user: User) {
  return user.nebula_id ?? "刷新中";
}

function identitySourceLabel(source: User["nebula_source"]) {
  return source === "skybridge" ? "Nebula" : "local_dev";
}

function methodLabel(method: LoginMethod) {
  if (method === "email") return "邮箱";
  if (method === "phone") return "手机号";
  return "账号编号";
}

function identifierLabel(mode: AuthMode, loginMethod: LoginMethod) {
  if (loginMethod === "email") return "邮箱";
  if (loginMethod === "phone") return "手机号";
  return mode === "login" ? "账号编号" : "邮箱";
}

function identifierPlaceholder(mode: AuthMode, loginMethod: LoginMethod) {
  if (loginMethod === "email") return "name@example.com";
  if (loginMethod === "phone") return "请输入手机号";
  return mode === "login" ? "请输入账号编号" : "name@example.com";
}

function identifierAutocomplete(loginMethod: LoginMethod) {
  if (loginMethod === "email") return "email";
  if (loginMethod === "phone") return "tel";
  return "username";
}

function formatMoney(amountMinor: number, currency: string) {
  const normalizedCurrency = currency.toUpperCase();
  try {
    return new Intl.NumberFormat("zh-CN", {
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
    return "这次登录需要先完成安全验证。请刷新页面重试；如果仍失败，请联系运营处理账号风控。";
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
  throw new Error("missing root element");
}

const portalWindow = window as PortalWindow;
const root = portalWindow.__OZON_PORTAL_ROOT__ ?? createRoot(rootElement);
portalWindow.__OZON_PORTAL_ROOT__ = root;
const customerGuidePath = ["/customer-guide", "/customer-guide.html"].includes(
  window.location.pathname
);
root.render(customerGuidePath ? <CustomerGuide /> : <App />);
