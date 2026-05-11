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
const NEBULA_OAUTH_ENTRY_ENABLED =
  import.meta.env.DEV || ["1", "true", "yes"].includes((import.meta.env.VITE_ENABLE_NEBULA_OAUTH_ENTRY ?? "").toLowerCase());
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
const DEV_NEBULA_OAUTH_BASE = import.meta.env.DEV ? "http://127.0.0.1:8788" : "";
const DEV_NEBULA_CLIENT_ID = import.meta.env.DEV ? "ozon_rust_suite_portal" : "";
const NEBULA_OAUTH_BASE = normalizeBaseUrl(
  import.meta.env.VITE_NEBULA_BASE_URL ?? import.meta.env.VITE_SKYBRIDGE_AUTH_BASEURL ?? DEV_NEBULA_OAUTH_BASE
);
const NEBULA_CLIENT_ID = (import.meta.env.VITE_NEBULA_CLIENT_ID ?? DEV_NEBULA_CLIENT_ID).trim();
const NEBULA_SCOPE = (import.meta.env.VITE_NEBULA_SCOPE ?? DEFAULT_NEBULA_SCOPE).trim();
const NEBULA_OAUTH_CONFIGURED = Boolean(NEBULA_OAUTH_BASE && NEBULA_CLIENT_ID);
const SKYBRIDGE_TURNSTILE_SITE_KEY = (import.meta.env.VITE_TURNSTILE_SITE_KEY ?? "").trim();
const SKYBRIDGE_TURNSTILE_CONFIGURED = Boolean(SKYBRIDGE_TURNSTILE_SITE_KEY);
const REQUEST_TIMEOUT_MS = 15_000;
const DEFAULT_LOCAL_NODE_MSI_URL =
  "https://github.com/billlza/ozon-rust-suite-downloads/releases/latest/download/OzonRustLocal-x64.msi";
const DEFAULT_LOCAL_NODE_EXE_URL =
  "https://github.com/billlza/ozon-rust-suite-downloads/releases/latest/download/OzonRustLocalSetup-x64.exe";

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

type Downloads = {
  local_node: string;
  local_node_msi?: string;
  local_node_exe?: string;
  version?: string;
  checksum: string;
  checksum_sha256?: string;
  openclaw_plugin?: string;
  openclaw_manifest?: string;
  local_manifest_url?: string;
};

type Session = {
  token: string;
  user: User;
};

type LocalNodePhase = "idle" | "checking" | "online" | "offline" | "blocked";

type LocalNodeHealth = {
  service: string;
  status: string;
  skill_port: number;
  agent_port: number;
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
  real_ozon_enabled: boolean;
  device_fingerprint: string;
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
    message: loadSession() ? "已恢复本地会话，正在等待刷新" : "请选择邮箱、手机号或 Nebula ID 登录",
    requestId: 0
  });
  const [operationStatus, setOperationStatus] = useState<string | null>(null);
  const authRequestId = useRef(0);
  const checkoutNoticeHandled = useRef(false);

  const [order, setOrder] = useState<Order | null>(null);
  const [paymentSession, setPaymentSession] = useState<PaymentSession | null>(null);
  const [cardKey, setCardKey] = useState("");
  const [entitlements, setEntitlements] = useState<Entitlement[]>([]);
  const [deviceName, setDeviceName] = useState("MacBook Local Node");
  const [deviceFingerprint, setDeviceFingerprint] = useState(() => defaultFingerprint());
  const [device, setDevice] = useState<Device | null>(null);
  const [lease, setLease] = useState<Lease | null>(null);
  const [downloads, setDownloads] = useState<Downloads | null>(null);
  const [localNode, setLocalNode] = useState<LocalNodeProbe>({
    phase: "idle",
    message: "登录后会检测 127.0.0.1:8790 本机节点"
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
  const localPairingStatus = localNodePairingStatus(localNode, activeEntitlement, device, lease);
  const localNodeMsiUrl = downloads?.local_node_msi ?? downloads?.local_node ?? DEFAULT_LOCAL_NODE_MSI_URL;
  const localNodeExeUrl = downloads?.local_node_exe ?? DEFAULT_LOCAL_NODE_EXE_URL;
  const openclawPluginUrl = downloads?.openclaw_plugin ? absolutePortalUrl(downloads.openclaw_plugin) : "";
  const localManifestUrl = localNode.portal?.manifest_url ?? `${LOCAL_NODE_API}/openclaw/manifest`;
  const canOpenLocalConsole = Boolean(LOCAL_CONSOLE_URL);
  const canCopyLocalManifest = localNode.phase === "online";
  const canDownloadOpenClawPlugin = Boolean(openclawPluginUrl);
  const canBindLocalDevice = canUseProtectedActions && localNode.phase === "online" && Boolean(localNode.portal?.device_fingerprint);
  const directAuthUnavailableMessage = "账号服务正在维护，请稍后再试或联系运营支持。";
  const authSubmitText =
    authMethod === "phone"
      ? authMode === "register"
        ? "手机号注册"
        : "手机号登录"
      : authMethod === "nebula"
        ? "Nebula ID 登录"
      : authMode === "register"
        ? "邮箱注册"
        : "邮箱登录";
  const authDialogTitle = authMode === "register" ? "创建账号" : "登录工作台";
  const authDialogDescription =
    authMode === "register"
      ? "账号创建后会进入同一套安装包、设备绑定和本机节点流程。"
      : "登录后继续查看授权、安装包和本机节点状态。";
  const shouldShowAuthDialogStatus =
    authBusy || authState.phase === "failed" || authState.phase === "authenticated" || Boolean(operationStatus);
  const setupStatus = setupStatusModel({
    activeEntitlement,
    device,
    lease,
    localNode,
    order
  });

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
      message: `账号已登录，Nebula ID：${data.user.nebula_id}`,
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
      const nextRequestId = beginAuth("failed", "请先使用 Nebula 登录");
      dispatchAuth({ type: "failure", message: "请先使用 Nebula 登录", requestId: nextRequestId });
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
    localStorage.removeItem(SESSION_KEY);
    setSession(null);
    setEntitlements([]);
    setOrder(null);
    setPaymentSession(null);
    setDevice(null);
    setLease(null);
    setOperationStatus(null);
    dispatchAuth({ type: "signed_out", message: "已退出登录", requestId });
  }

  async function createOrder() {
    if (!session) {
      setOperationStatus("请先使用 Nebula 登录");
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
      `订单 UUID: ${order.id}\n支付方式: ${order.payment_provider}\n支付备注: ${order.payment_reference}`
    );
    setOperationStatus("订单信息已复制");
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
          : `订单状态已刷新：${data.order.status}`
      );
    } catch (error) {
      setOperationStatus(`刷新订单失败：${errorMessage(error)}`);
    }
  }

  async function redeem() {
    if (!session || !cardKey.trim()) {
      setOperationStatus("需要登录并填写卡密");
      return;
    }
    try {
      await api<{ entitlement: Entitlement }>("/card-keys/redeem", {
        method: "POST",
        body: JSON.stringify({ card_key: cardKey.trim() })
      });
      setCardKey("");
      setOperationStatus("卡密已兑换，授权已生效");
      await refreshAccount();
    } catch (error) {
      setOperationStatus(`兑换失败：${errorMessage(error)}`);
    }
  }

  async function activateDevice() {
    if (!session) {
      setOperationStatus("请先使用 Nebula 登录");
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
      setOperationStatus("设备已绑定到当前账户");
    } catch (error) {
      setOperationStatus(`设备绑定失败：${errorMessage(error)}`);
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
      setLease(data.lease);
      try {
        await localNodePost("/portal/lease", { lease: data.lease });
        setOperationStatus("授权租约已签发，并已写入本机节点");
        await probeLocalNode();
      } catch (localError) {
        setOperationStatus(`云端租约已签发，但写入本机节点失败：${errorMessage(localError)}`);
      }
    } catch (error) {
      setOperationStatus(`签发租约失败：${errorMessage(error)}`);
    }
  }

  async function probeLocalNode() {
    setLocalNode({
      phase: "checking",
      message: "正在检测本机 127.0.0.1:8790 节点"
    });
    try {
      const [health, manifest, portal] = await Promise.all([
        localNodeJson<LocalNodeHealth>("/health"),
        localNodeJson<LocalNodeManifest>("/openclaw/manifest"),
        localNodeJson<LocalPortalStatus>("/portal/status").catch(() => null)
      ]);
      if (portal?.device_fingerprint) {
        setDeviceFingerprint(portal.device_fingerprint);
      }
      setLocalNode({
        phase: "online",
        message: health.real_ozon_enabled ? "本机节点在线，当前是真实 Ozon API 模式" : "本机节点在线，当前使用开发连接器",
        checkedAt: new Date().toISOString(),
        health,
        manifest,
        portal: portal ?? undefined
      });
    } catch (error) {
      setLocalNode({
        phase: isLocalNodeBrowserBlock(error) ? "blocked" : "offline",
        message: localNodeFailureMessage(error),
        checkedAt: new Date().toISOString()
      });
    }
  }

  async function copyLocalManifestUrl() {
    if (!canCopyLocalManifest) {
      setOperationStatus("检测到本机节点 online 后即可复制本机 manifest");
      return;
    }
    await copyText(localManifestUrl);
    setOperationStatus("本机 OpenClaw manifest URL 已复制");
  }

  useEffect(() => {
    let cancelled = false;
    async function bootPortal() {
      fetchDownloads()
        .then((downloadData) => {
          if (!cancelled) setDownloads(downloadData);
        })
        .catch(() => {
          // Download links have stable release fallbacks; keep the account flow usable.
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
        message: "登录后会检测 127.0.0.1:8790 本机节点"
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
    if (!SKYBRIDGE_TURNSTILE_CONFIGURED || !authDialogOpen) {
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
            setTurnstileStatus("若验证控件不可见，请确认 Turnstile site key 允许当前域名，或使用 Nebula 授权入口");
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
    <main>
      <header className="site-nav">
        <a className="brand-mark" href="#top" aria-label="Ozon Rust Suite">
          <span className="brand-icon">
            <Orbit size={18} />
          </span>
          <span>Ozon Rust Suite</span>
        </a>
        <nav aria-label="primary navigation">
          <a href="#capabilities">功能</a>
          <a href="#workflow">流程</a>
          <a href="#pricing">方案</a>
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

      <section className="hero-section" id="top">
        <div className="hero-copy">
          <p className="eyebrow">Ozon Rust Suite</p>
          <h1>从登录到真实商品读取，卖家工作台一条线接好。</h1>
          <p>
            门户管理账号、授权、安装包和本机节点；本地控制台保存 Ozon 凭据、读取商品详情、生成海报，并把写操作留在本机审批。
          </p>
          <div className="hero-actions">
            {session ? (
              <>
                <a className="download" href="#console">
                  <ArrowRight size={18} /> 继续接入流程
                </a>
                {canOpenLocalConsole && (
                  <a className="download secondary" href={LOCAL_CONSOLE_URL} target="_blank" rel="noreferrer">
                    <MonitorCheck size={18} /> 打开本地控制台
                  </a>
                )}
                <button className="secondary" disabled={authBusy} onClick={() => refreshAccount()}>
                  <RefreshCcw size={18} /> 刷新状态
                </button>
              </>
            ) : (
              <>
                <button onClick={() => openAuthDialog("register")}>
                  <UserPlus size={18} /> 立即注册
                </button>
                <a className="download secondary" href="#workflow">
                  <ArrowRight size={18} /> 看看接入步骤
                </a>
              </>
            )}
          </div>
          <div className="hero-meta">
            <span>Nebula 统一身份</span>
            <span>真实 Seller API 读取</span>
            <span>本机密钥与审批</span>
          </div>
        </div>
        <div className="hero-visual" aria-label="Ozon Rust Suite workflow preview">
          <div className="visual-topbar">
            <span />
            <span />
            <span />
            <strong>operator path</strong>
          </div>
          <div className="visual-grid">
            <div>
              <span>Step 1</span>
              <strong>登录账号</strong>
            </div>
            <div>
              <span>Step 2</span>
              <strong>下载并启动本地节点</strong>
            </div>
            <div>
              <span>Step 3</span>
              <strong>检测 127.0.0.1:8790</strong>
            </div>
          </div>
          <div className="diff-preview">
            <span>Step 4</span>
            <strong>验证 Ozon 商品，再生成可校验海报</strong>
            <em>商品事实、图片顺序和文案校验都留在本机控制台。</em>
          </div>
        </div>
      </section>

      <section className="capability-band" id="capabilities">
        <div className="band-title">
          <p className="eyebrow">已接通的核心链路</p>
          <h2>账户、授权、本机节点、商品读取和海报生成已经串起来。</h2>
        </div>
        <div className="capability-grid">
          <article>
            <PackageCheck />
            <h3>账号、下载和授权走在一条线上</h3>
            <p>登录、安装包、设备绑定和租约状态都在同一个工作台里看，不用来回找页面。</p>
          </article>
          <article>
            <Boxes />
            <h3>真实 Ozon 商品已经能读</h3>
            <p>本地节点直接读官方 Seller API。凭据没配好就报错，不会悄悄回退成假数据。</p>
          </article>
          <article>
            <Bot />
            <h3>商品海报带事实校验</h3>
            <p>读取商品图和属性后生成海报背景，文案叠加按 fact pack 校验，避免错货错词。</p>
          </article>
        </div>
      </section>

      <section className="workflow-band" id="workflow">
        <div className="workflow-copy">
          <p className="eyebrow">上手路径</p>
          <h2>登录、下载、检测、验证、读取商品。</h2>
          <p>
            门户负责账号和授权，本地控制台负责凭据、商品读取和海报生成。云端不接触 Ozon API Key，本机节点只接受经过授权的操作。
          </p>
        </div>
        <div className="workflow-steps">
          <div>
            <span>01</span>
            <strong>登录账号</strong>
            <p>邮箱、手机号和 Nebula ID 都归同一个身份，登录后自动恢复下载和授权状态。</p>
          </div>
          <div>
            <span>02</span>
            <strong>下载安装包</strong>
            <p>安装并启动本机节点，门户会检测 `127.0.0.1:8790` 的在线状态。</p>
          </div>
          <div>
            <span>03</span>
            <strong>检测本机节点</strong>
            <p>门户只探测 `127.0.0.1:8790` 有没有响应，不去越权读取你的本地密钥。</p>
          </div>
          <div>
            <span>04</span>
            <strong>打开本地控制台</strong>
            <p>在本地控制台验证 Ozon 凭据、读取真实商品，并生成带事实校验的海报。</p>
          </div>
        </div>
      </section>

      {session && (
        <section className="account-console" id="console">
          <aside className="identity-panel">
            <p className="eyebrow">Nebula session</p>
            <h2>账户与授权</h2>
            <div className="rail-status">
              <span>Nebula ID</span>
              <strong>{displayNebulaId(session.user)}</strong>
            </div>
            <div className="rail-status">
              <span>登录别名</span>
              <strong>{displayLoginAlias(session.user)}</strong>
            </div>
            <div className="rail-status">
              <span>身份来源</span>
              <strong>{identitySourceLabel(session.user.nebula_source)}</strong>
            </div>
            <div className="rail-status">
              <span>授权状态</span>
              <strong>{activeEntitlement ? "active" : "none"}</strong>
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
                    <KeyRound size={18} /> Nebula ID
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

          <div className={`status-line ${authState.phase === "failed" ? "danger-line" : ""}`} aria-busy={authBusy}>
            <CheckCircle2 size={18} /> {statusMessage}
          </div>

          <section className="operations">
            <div className={`setup-panel ${setupStatus.kind}`}>
              <div>
                <span>当前下一步</span>
                <h2>{setupStatus.title}</h2>
                <p>{setupStatus.message}</p>
              </div>
              <div className="setup-actions">
                {!activeEntitlement && (
                  <button disabled={!canUseProtectedActions} onClick={createOrder}>
                    <ArrowRight size={18} /> 提交开通申请
                  </button>
                )}
                {localNode.phase !== "online" && (
                  <>
                    <a className="download" href={localNodeMsiUrl}>
                      <Download size={18} /> 下载 MSI
                    </a>
                    <a className="download secondary" href={localNodeExeUrl}>
                      <Download size={18} /> 下载 EXE
                    </a>
                    <button className="secondary" disabled={localNode.phase === "checking"} onClick={probeLocalNode}>
                      <RefreshCcw size={18} /> 检测本机节点
                    </button>
                  </>
                )}
                {activeEntitlement && localNode.phase === "online" && !device && (
                  <button disabled={!canBindLocalDevice} onClick={activateDevice}>
                    <MonitorCheck size={18} /> 绑定设备
                  </button>
                )}
                {activeEntitlement && device && !lease && (
                  <button className="secondary" onClick={issueLease} disabled={authBusy}>
                    <Radio size={18} /> 签发租约
                  </button>
                )}
                {setupStatus.kind === "online" && canOpenLocalConsole && (
                  <a className="download" href={LOCAL_CONSOLE_URL} target="_blank" rel="noreferrer">
                    <MonitorCheck size={18} /> 打开本地控制台
                  </a>
                )}
              </div>
            </div>

            <div className="op-section">
              <div className="section-title">
                  <Clipboard />
                  <div>
                    <h2>开通申请</h2>
                  <p>收款通道打开前，这里用于提交授权申请；运营确认后会发送卡密，兑换后授权同步到账户。</p>
                  </div>
                </div>
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
                    开通状态
                    <input value={order.status} readOnly />
                  </label>
                  <label>
                    通道
                    <input value={order.payment_provider} readOnly />
                  </label>
                  <label>
                    金额
                    <input value={formatMoney(order.amount_minor, order.currency)} readOnly />
                  </label>
                </div>
              ) : (
                <p className="section-hint">
                  当前账号还没有开通记录。提交申请后会生成编号；如果你已有卡密，也可以直接在下方兑换。
                </p>
              )}
              {paymentSession && (
                <div className="payment-note">
                  <CheckCircle2 size={18} />
                  <span>{paymentSession.message}</span>
                  {paymentSession.checkout_url && (
                    <a href={paymentSession.checkout_url}>
                      <ExternalLink size={16} /> 打开支付页
                    </a>
                  )}
                </div>
              )}
              <div className="command-row">
                <button disabled={!canUseProtectedActions} onClick={createOrder}>
                  <ArrowRight size={18} /> 提交开通申请
                </button>
                <button className="secondary" onClick={copyOrderInfo} disabled={!order}>
                  复制申请信息
                </button>
                <button className="secondary" onClick={refreshOrder} disabled={!order || authBusy}>
                  <RefreshCcw size={18} /> 刷新状态
                </button>
              </div>
            </div>

            <details className="op-section support-section">
              <summary>
                <KeyRound size={20} />
                <div>
                  <strong>已有卡密？兑换授权</strong>
                  <span>客服确认、企业内测或线下开通时使用。</span>
                </div>
              </summary>
              <div className="form-grid">
                <label>
                  卡密
                  <input value={cardKey} onChange={(event) => setCardKey(event.target.value)} placeholder="ORS-..." />
                </label>
                <label>
                  当前计划
                  <input value={activeEntitlement?.plan_code ?? "未授权"} readOnly />
                </label>
                <label>
                  到期时间
                  <input value={activeEntitlement?.expires_at ?? "无"} readOnly />
                </label>
              </div>
              <div className="command-row">
                <button disabled={!canUseProtectedActions} onClick={redeem}>
                  <KeyRound size={18} /> 兑换授权
                </button>
              </div>
            </details>

            <div className="op-section local-node-section">
              <div className="section-title">
                  <MonitorCheck />
                  <div>
                    <h2>本机节点与 OpenClaw</h2>
                  <p>门户检测本机服务并下发授权，本地控制台负责凭据校验、商品读取和海报生成。</p>
                  </div>
                </div>
              <div className="local-node-grid">
                <div className={`local-node-card ${localNode.phase}`}>
                  <span>本机服务</span>
                  <strong>{localNodeStatusLabel(localNode.phase)}</strong>
                  <p>{localNode.message}</p>
                </div>
                <div className={`local-node-card ${localPairingStatus.kind}`}>
                  <span>配对/授权</span>
                  <strong>{localPairingStatus.title}</strong>
                  <p>{localPairingStatus.message}</p>
                </div>
                <div className="local-node-card">
                  <span>OpenClaw manifest</span>
                  <strong>{localNode.manifest?.version ?? downloads?.version ?? "待检测"}</strong>
                  <p>{localManifestUrl}</p>
                </div>
              </div>
              <div className="command-row">
                <button disabled={localNode.phase === "checking"} onClick={probeLocalNode}>
                  <RefreshCcw size={18} /> 检测本机节点
                </button>
                {localNodeMsiUrl && (
                  <a className="download" href={localNodeMsiUrl}>
                    <Download size={18} /> MSI 安装包
                  </a>
                )}
                {localNodeExeUrl && (
                  <a className="download" href={localNodeExeUrl}>
                    <Download size={18} /> EXE 安装包
                  </a>
                )}
                {canOpenLocalConsole && (
                  <a className="download secondary" href={LOCAL_CONSOLE_URL} target="_blank" rel="noreferrer">
                    <MonitorCheck size={18} /> 打开本地控制台
                  </a>
                )}
                {canDownloadOpenClawPlugin && (
                  <a className="download" href={openclawPluginUrl} target="_blank" rel="noreferrer">
                    <Download size={18} /> OpenClaw 插件
                  </a>
                )}
                <button className="secondary" disabled={!canCopyLocalManifest} onClick={copyLocalManifestUrl}>
                  <Clipboard size={18} /> 复制本机 manifest
                </button>
              </div>
              {localNode.phase !== "online" && (
                <p className="action-reason">安装并启动本机节点后，回到这里点击“检测本机节点”。检测到 online 后才能复制 manifest、绑定设备和签发租约。</p>
              )}
              <p className="section-hint">
                安装并启动本机节点后，复制本机 manifest 给 OpenClaw；授权信息由本机节点提供。
              </p>
              {localNode.manifest && (
                <div className="manifest-tools">
                  {localNode.manifest.tools.map((tool) => (
                    <div key={tool.name}>
                      <span>{tool.risk}</span>
                      <strong>{tool.name}</strong>
                      <em>{tool.approval_required ? "本地审批" : "只读"}</em>
                    </div>
                  ))}
                </div>
              )}
            </div>

            <div className="op-section">
              <div className="section-title">
                  <MonitorCheck />
                  <div>
                    <h2>设备绑定</h2>
                  <p>使用本机节点生成的指纹绑定设备；超出 `max_devices` 会被云端拒绝。</p>
                  </div>
                </div>
              <div className="form-grid">
                <label>
                  设备名
                  <input value={deviceName} onChange={(event) => setDeviceName(event.target.value)} />
                </label>
                <label>
                  设备指纹
                  <input
                    value={deviceFingerprint}
                    readOnly
                    placeholder="先检测本机节点"
                    title="设备指纹由本机节点生成，门户不再允许手写伪造"
                  />
                </label>
                <label>
                  绑定状态
                  <input value={device ? `${device.name} · ${device.status}` : "未绑定"} readOnly />
                </label>
              </div>
              <div className="command-row">
                <button disabled={!canBindLocalDevice} onClick={activateDevice}>
                  <MonitorCheck size={18} /> 绑定设备
                </button>
                <button className="secondary" onClick={issueLease} disabled={!device || authBusy}>
                  <Radio size={18} /> 签发租约
                </button>
              </div>
              {!activeEntitlement && <p className="action-reason">设备绑定需要有效授权。请先提交开通申请，或兑换已有卡密。</p>}
              {activeEntitlement && localNode.phase !== "online" && (
                <p className="action-reason">设备绑定需要本机节点 online。请先启动本机节点并重新检测。</p>
              )}
              {activeEntitlement && localNode.phase === "online" && !device && (
                <p className="action-reason">本机节点已在线，可以绑定这台设备。</p>
              )}
              {lease && (
                <div className="lease-line">
                  <span>Lease</span>
                  <strong>{lease.lease_id}</strong>
                  <em>expires {lease.expires_at}</em>
                </div>
              )}
            </div>
          </section>
        </section>
        </section>
      )}

      <section className="pricing-band" id="pricing">
        <div>
          <p className="eyebrow">开始接入</p>
          <h2>进入工作台，完成授权、本机节点和 Ozon 商品读取。</h2>
          <p>账号、安装包、设备、租约和本机控制台已经连在同一条路径里。登录后按状态提示完成接入即可。</p>
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

      {authDialogOpen && (
        <div className="auth-backdrop" role="presentation" onMouseDown={() => setAuthDialogMode(null)}>
          <section
            aria-labelledby="auth-dialog-title"
            aria-modal="true"
            className="auth-dialog"
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
              <span>{SKYBRIDGE_AUTH_CONFIGURED ? "安全登录后可进入授权工作台。" : "账号服务正在维护，请联系运营支持。"}</span>
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
                    <KeyRound size={18} /> Nebula ID
                  </button>
                )}
              </div>

              {!SKYBRIDGE_AUTH_CONFIGURED && (
                <p className="identity-note">{directAuthUnavailableMessage}</p>
              )}

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
                  disabled={authBusy || skybridgeOtpBusy || directAuthNeedsTurnstile || !SKYBRIDGE_AUTH_CONFIGURED}
                  type="button"
                  onClick={sendSkybridgePhoneCode}
                >
                  <Smartphone size={18} /> 获取验证码
                </button>
              )}
              <button disabled={authBusy || directAuthNeedsTurnstile || !SKYBRIDGE_AUTH_CONFIGURED} type="submit">
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
              <p className="identity-note">Nebula 已启用人机验证；完成安全验证后再提交兼容入口。</p>
            )}

            {NEBULA_OAUTH_ENTRY_ENABLED && (
              <details className="compat-panel">
                <summary>统一身份页</summary>
                <section className="nebula-oauth-panel">
                  <div className="section-title compact-title">
                    <ShieldCheck />
                    <div>
                      <h2>统一身份页</h2>
                      <p>通过 Nebula 完成账号授权并返回当前工作台。</p>
                    </div>
                  </div>
                  <div className="oauth-actions">
                    <button disabled={authBusy || !NEBULA_OAUTH_CONFIGURED} onClick={() => startNebulaOAuth(authMode)}>
                      <ExternalLink size={18} /> {authMode === "register" ? "打开注册" : "打开登录"}
                    </button>
                  </div>
                  {!NEBULA_OAUTH_CONFIGURED && (
                    <p className="identity-note">统一身份页暂不可用，请使用上方账号入口。</p>
                  )}
                </section>
              </details>
            )}

            {shouldShowAuthDialogStatus && (
              <div className={`status-line ${authState.phase === "failed" ? "danger-line" : authBusy ? "busy-line" : ""}`} aria-busy={authBusy}>
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

            {captchaBlocked && (
              <div className="captcha-callout">
                <AlertCircle />
                <div>
                  <strong>Nebula 安全策略已拦截这次密码交换</strong>
                  <p>
                    请求已经到达身份服务，但安全策略要求 captcha_token。这个门户不会绕过验证码；推荐改用统一授权页，
                    由身份服务完成验证码、MFA 和风控后再回调 Ozon。
                  </p>
                </div>
              </div>
            )}
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
      throw new Error("请求超时，请检查 Nebula/Ozon 服务是否可访问");
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

function localNodeFailureMessage(error: unknown) {
  const message = errorMessage(error);
  if (isLocalNodeBrowserBlock(error)) {
    return "浏览器没拿到本机节点响应。请安装/启动新版本地节点；旧版本可能缺少 ozon66.com 的本地网络预检允许。";
  }
  return `未检测到本机节点：${message}`;
}

function isLocalNodeBrowserBlock(error: unknown) {
  const message = errorMessage(error).toLowerCase();
  return message.includes("failed to fetch") || message.includes("networkerror") || message.includes("load failed");
}

function localNodeStatusLabel(phase: LocalNodePhase) {
  switch (phase) {
    case "checking":
      return "checking";
    case "online":
      return "online";
    case "blocked":
      return "blocked";
    case "offline":
      return "offline";
    default:
      return "waiting";
  }
}

function confirmedOrderMessage(order: Order) {
  if (order.payment_provider === "manual") {
    return "申请已人工确认。请使用运营发送的卡密兑换授权。";
  }
  return "订单已确认，授权状态会在账户刷新后生效";
}

function localNodePairingStatus(
  localNode: LocalNodeProbe,
  entitlement: Entitlement | null,
  device: Device | null,
  lease: Lease | null
) {
  if (!entitlement) {
    return {
      kind: "warn",
      title: "未授权",
      message: "先兑换卡密或完成付款确认，才可签发本地节点租约。"
    };
  }
  if (!device) {
    return {
      kind: "warn",
      title: "待绑定设备",
      message: "授权已存在；下一步绑定本机设备，再签发租约。"
    };
  }
  if (!lease) {
    return {
      kind: localNode.phase === "online" ? "warn" : "offline",
      title: "待签发租约",
      message: "设备已绑定；点击签发租约后，本机节点可展示授权状态。"
    };
  }
  return {
    kind: localNode.phase === "online" ? "online" : "warn",
    title: localNode.phase === "online" ? "已配对" : "云端已授权",
    message: `Lease ${lease.lease_id} 已签发，过期时间 ${lease.expires_at}。`
  };
}

function setupStatusModel(input: {
  activeEntitlement: Entitlement | null;
  device: Device | null;
  lease: Lease | null;
  localNode: LocalNodeProbe;
  order: Order | null;
}) {
  const nodeOnline = input.localNode.phase === "online";
  if (!input.activeEntitlement && !nodeOnline) {
    return {
      kind: "warn",
      title: "本机节点和授权都还没接上",
      message:
        "下载安装包并启动本机节点，同时提交开通申请或兑换已有卡密。两件事完成后，设备绑定和租约会自动变成可操作。"
    };
  }
  if (!nodeOnline) {
    return {
      kind: "warn",
      title: "启动本机节点",
      message: "授权已经存在。现在下载安装包并启动本机节点，检测到 online 后即可绑定设备。"
    };
  }
  if (!input.activeEntitlement) {
    return {
      kind: input.order ? "warn" : "offline",
      title: input.order ? "等待授权开通" : "开通账户授权",
      message: input.order
        ? "开通申请已生成。收到卡密并兑换后，设备绑定和租约会变成可操作。"
        : "本机节点已经在线。提交开通申请或兑换已有卡密后，就能绑定设备并签发本机租约。"
    };
  }
  if (!input.device) {
    return {
      kind: "warn",
      title: "绑定这台设备",
      message: "本机节点在线且授权有效。使用本机节点生成的设备指纹完成绑定。"
    };
  }
  if (!input.lease) {
    return {
      kind: "warn",
      title: "签发本机租约",
      message: "设备已绑定。签发租约后，本机控制台会显示云端授权状态。"
    };
  }
  return {
    kind: "online",
    title: "工作台已接通",
    message: "账号授权、设备绑定和本机租约都已就绪，可以打开本地控制台读取 Ozon 商品并生成海报。"
  };
}

function absolutePortalUrl(pathOrUrl: string) {
  return new URL(pathOrUrl, window.location.origin).toString();
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
    throw new Error("Nebula ID 由 Nebula 分配，注册请使用邮箱或手机号");
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
    throw new Error(skybridgeErrorMessage(result, "Nebula ID 登录失败"));
  }
  const session = extractSkybridgeSession(result);
  if (!session?.access_token) {
    throw new Error("Nebula ID 登录响应无效");
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
    throw new Error(skybridgeErrorMessage(result, "Nebula 授权换取会话失败"));
  }
  const session = extractSkybridgeSession(result);
  if (!session?.access_token) {
    throw new Error("Nebula 授权返回的 access_token 无效");
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
    script.src = "https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit";
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
  return "Nebula ID";
}

function identifierLabel(mode: AuthMode, loginMethod: LoginMethod) {
  if (loginMethod === "email") return "邮箱";
  if (loginMethod === "phone") return "手机号";
  return mode === "login" ? "Nebula ID" : "邮箱";
}

function identifierPlaceholder(mode: AuthMode, loginMethod: LoginMethod) {
  if (loginMethod === "email") return "name@example.com";
  if (loginMethod === "phone") return "请输入手机号";
  return mode === "login" ? "请输入 Nebula ID" : "name@example.com";
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

function skybridgeDirectAuthFailureMessage(error: unknown) {
  const message = errorMessage(error);
  if (isCaptchaProtectionMessage(message)) {
    return "请求已到达身份服务，但当前入口缺少 captcha_token，已被安全策略拒绝。请使用统一授权页，或在后续接入 Turnstile 验证组件。";
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
root.render(<App />);
