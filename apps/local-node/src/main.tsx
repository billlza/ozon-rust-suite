import React, { useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  Activity,
  CheckCircle2,
  Clipboard,
  DatabaseZap,
  Image as ImageIcon,
  KeyRound,
  ListChecks,
  PauseCircle,
  Play,
  Radio,
  RefreshCcw,
  Repeat2,
  Sparkles,
  ShieldCheck,
  SlidersHorizontal,
  TerminalSquare,
  Workflow,
  XCircle
} from "lucide-react";
import { copyFor, initialLocale, localeOptions, type Locale } from "./i18n";
import "./styles.css";

const DEFAULT_LOCAL_API = import.meta.env.VITE_LOCAL_SKILL_API ?? "http://127.0.0.1:8790";
const DEFAULT_AGENT_API = import.meta.env.VITE_LOCAL_AGENT_API ?? "http://127.0.0.1:17870";
const DEFAULT_LOCAL_TOKEN = import.meta.env.VITE_LOCAL_TOKEN ?? "dev-local-token";

type RuntimeConfig = {
  skill_api: string;
  agent_api: string;
  local_token: string;
  connector_mode: "mock" | "real" | string;
  sidecar_pid: number | null;
  sidecar_status: "starting" | "running" | "restarting" | "stopped" | "failed" | string;
  sidecar_restart_count: number;
  sidecar_last_started_at_ms: number | null;
  sidecar_last_exit: string | null;
  sidecar_last_error: string | null;
  sidecar_log_path: string;
};

type Health = {
  service: string;
  status: string;
  protocol_version?: string;
  build_commit?: string;
  package_version?: string;
  supervisor?: string;
  features: string[];
  real_ozon_enabled: boolean;
};

type Product = {
  product_id: string;
  offer_id: string;
  name: string | null;
  visibility: string | null;
  archived: boolean | null;
  has_fbo_stocks: boolean | null;
  has_fbs_stocks: boolean | null;
};

type ProductListResult = {
  connector_mode: "mock" | "real";
  products: Product[];
  total: number;
  last_id: string | null;
  visibility: string;
  archived_fallback: boolean;
};

type ProductCountResult = {
  count: number;
  visibility: string;
  archived_fallback: boolean;
};

type ProductImage = {
  url: string;
  role: "primary" | "gallery" | "color" | "spin360";
  position: number;
};

type ProductAttribute = {
  id: number | null;
  name: string | null;
  values: string[];
};

type ProductDetail = {
  lookup: {
    product_id: string | null;
    offer_id: string | null;
    sku: string | null;
  };
  product_id: string;
  offer_id: string;
  sku: string | null;
  name: string | null;
  description_category_id: number | null;
  type_id: number | null;
  barcodes: string[];
  primary_image: string | null;
  images: ProductImage[];
  gallery_images: string[];
  images360: string[];
  color_image: string | null;
  attributes: ProductAttribute[];
  visibility: string | null;
  archived: boolean | null;
  autoarchived: boolean | null;
  created_at: string | null;
  updated_at: string | null;
  warnings: string[];
};

type ProductDetailResult = {
  connector_mode: "mock" | "real";
  product: ProductDetail;
};

type PosterBrief = {
  theme: string;
  headline: string;
  subheadline: string;
  selling_points: string[];
  cta_line: string;
  compliance_note: string;
  background_prompt: string;
};

type PosterBriefResult = {
  connector_mode: "mock" | "real";
  product: ProductDetail;
  brief: PosterBrief;
};

type PosterGenerateResult = {
  connector_mode: "mock" | "real";
  product: ProductDetail;
  brief: PosterBrief;
  image_model: string;
  prompt: string;
  revised_prompt?: string | null;
  background_data_url: string;
};

type PosterHandoffResult = {
  connector_mode: "mock" | "real";
  generated_at: string;
  mode: "openclaw_codex";
  product: ProductDetail;
  brief: PosterBrief;
  source_images: Array<{
    role: string;
    url: string;
    note: string;
  }>;
  openclaw: {
    manifest_url: string;
    auth_header: string;
    token_policy: string;
    recommended_tools: string[];
  };
  instructions: string[];
  prompt: string;
};

type PosterCopyMismatch = {
  field: string;
  expected: string;
  actual: string;
};

type PosterVerifyResult = {
  ok: boolean;
  checked_at: string;
  approved_copy: {
    headline: string;
    subheadline: string;
    selling_points: string[];
    cta_line: string;
    compliance_note: string;
  };
  mismatches: PosterCopyMismatch[];
  warnings: string[];
};

type Task = {
  id: string;
  operation: string;
  state: string;
  risk: string;
  source: string;
  shop_id: string;
  dry_run: {
    summary: string;
    target_count: number;
    warnings: string[];
    changes: Array<{
      object_id: string;
      field: string;
      before?: string;
      after?: string;
    }>;
  };
  receipt?: {
    result_summary: string;
    executed_at: string;
  };
};

type Manifest = {
  name: string;
  version: string;
  base_url: string;
  tools: Array<{
    name: string;
    path: string;
    risk: string;
    approval_required: boolean;
  }>;
  safety_rules: string[];
};

type OpenClawPairingStart = {
  status: string;
  bind_url: string;
  pairing_code: string;
  claim_url: string;
  manifest_url: string;
  auth_header: string;
  expires_at: string;
  instructions: string[];
};

type ConfigStatus = {
  service: string;
  checked_at: string;
  real_ozon_enabled: boolean;
  connector_mode: "mock" | "real";
  secret_store: {
    backend: string;
    available: boolean;
  };
  ozon: {
    configured: boolean;
    source: string;
    client_id: string | null;
    api_key_fingerprint: string | null;
    issue: string | null;
  };
  poster_generation: {
    preferred: "openclaw_codex" | string;
    openclaw_bridge_ready: boolean;
    handoff_path: string;
    manifest_url: string;
    api_fallback_configured: boolean;
    api_fallback_model: string | null;
    api_fallback_issue: string | null;
    message: string;
  };
  openai: {
    configured: boolean;
    source: string;
    base_url: string;
    image_model: string;
    api_key_fingerprint: string | null;
    issue: string | null;
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
  endpoints: {
    skill_api: string;
    agent_api: string;
    manifest_url: string;
  };
};

type ValidationResult = {
  ok: boolean;
  checked_at: string;
  connector_mode: "mock" | "real";
  message: string;
};

type ScheduleStatus = {
  enabled: boolean;
  interval_secs: number;
  limit: number;
  connector_mode: "mock" | "real";
  last_run: null | {
    started_at: string;
    completed_at: string;
    duration_ms: number;
    connector_mode: "mock" | "real";
    product_count: number;
    sample_size: number;
    next_last_id: string | null;
    products: Product[];
  };
  last_error: string | null;
  audit: Array<{
    at: string;
    actor: string;
    action: string;
    summary: string;
  }>;
  safety: string[];
};

function App() {
  const [locale, setLocale] = useState<Locale>(() => initialLocale());
  const c = copyFor(locale);
  const [runtime, setRuntime] = useState<RuntimeConfig>(() => defaultRuntimeConfig());
  const [runtimeReady, setRuntimeReady] = useState(false);
  const [health, setHealth] = useState<Health | null>(null);
  const [manifest, setManifest] = useState<Manifest | null>(null);
  const [configStatus, setConfigStatus] = useState<ConfigStatus | null>(null);
  const [clientId, setClientId] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [openAiBaseUrl, setOpenAiBaseUrl] = useState("https://api.openai.com");
  const [openAiImageModel, setOpenAiImageModel] = useState("gpt-image-1");
  const [openAiApiKey, setOpenAiApiKey] = useState("");
  const [products, setProducts] = useState<Product[]>([]);
  const [productCount, setProductCount] = useState<number | null>(null);
  const [productListMeta, setProductListMeta] = useState<ProductListResult | null>(null);
  const [productLookup, setProductLookup] = useState("");
  const [productDetail, setProductDetail] = useState<ProductDetailResult | null>(null);
  const [posterTheme, setPosterTheme] = useState("studio");
  const [posterBrief, setPosterBrief] = useState<PosterBriefResult | null>(null);
  const [posterHandoff, setPosterHandoff] = useState<PosterHandoffResult | null>(null);
  const [posterBackground, setPosterBackground] = useState<PosterGenerateResult | null>(null);
  const [posterVerification, setPosterVerification] = useState<PosterVerifyResult | null>(null);
  const [imageGenerationIssue, setImageGenerationIssue] = useState<string | null>(null);
  const [posterHeadline, setPosterHeadline] = useState("");
  const [posterSubheadline, setPosterSubheadline] = useState("");
  const [posterSellingPoints, setPosterSellingPoints] = useState(["", "", ""]);
  const [posterCtaLine, setPosterCtaLine] = useState("");
  const [posterComplianceNote, setPosterComplianceNote] = useState("");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [selectedOperation, setSelectedOperation] = useState("ozon_update_price_mock");
  const [eventState, setEventState] = useState("connecting");
  const [message, setMessage] = useState(() => copyFor(initialLocale()).messages.initial);
  const [validation, setValidation] = useState<ValidationResult | null>(null);
  const [schedule, setSchedule] = useState<ScheduleStatus | null>(null);
  const [scheduleInterval, setScheduleInterval] = useState(900);
  const [scheduleLimit, setScheduleLimit] = useState(20);
  const realModeRequiresLease = configStatus?.real_ozon_enabled ?? true;
  const ozonConfigReady = configStatus?.ozon.configured === true;
  const openAiConfigReady = configStatus?.openai.configured === true;
  const leaseReady = !realModeRequiresLease || configStatus?.lease.valid === true;
  const canUseOzonReadTools = Boolean(configStatus && ozonConfigReady && leaseReady);
  const canGeneratePosterBackground = canUseOzonReadTools && openAiConfigReady && !imageGenerationIssue;
  const localServiceReady = Boolean(health && isConnectedRuntime(runtime));
  const openClawBridgeReady = localServiceReady && Boolean(manifest?.tools?.length);
  const setupSteps = [
    {
      number: "1",
      title: c.setup.steps.saveOzon.title,
      status: canUseOzonReadTools ? c.setup.steps.saveOzon.done : ozonConfigReady ? c.setup.steps.saveOzon.needLease : c.setup.steps.saveOzon.pending,
      detail: c.setup.steps.saveOzon.detail,
      state: canUseOzonReadTools ? "done" : "active"
    },
    {
      number: "2",
      title: c.setup.steps.connectOpenClaw.title,
      status: openClawBridgeReady ? c.setup.steps.connectOpenClaw.ready : c.setup.steps.connectOpenClaw.waiting,
      detail: c.setup.steps.connectOpenClaw.detail,
      state: canUseOzonReadTools && openClawBridgeReady ? "active" : openClawBridgeReady ? "ready" : "wait"
    },
    {
      number: "3",
      title: c.setup.steps.readProducts.title,
      status: productDetail ? c.setup.steps.readProducts.selected : canUseOzonReadTools ? c.setup.steps.readProducts.ready : c.setup.steps.readProducts.waiting,
      detail: c.setup.steps.readProducts.detail,
      state: productDetail ? "done" : canUseOzonReadTools ? "ready" : "wait"
    }
  ];
  const ozonReadGateMessage = !configStatus
    ? c.gates.connectLocalFirst
    : !configStatus.secret_store.available
      ? c.gates.secretStoreUnavailable
      : !ozonConfigReady
        ? userFacingError(configStatus.ozon.issue ?? "Ozon credentials are not configured")
        : realModeRequiresLease && !configStatus.lease.valid
          ? c.gates.needsLease
          : "";
  const posterGenerationGateMessage = !canUseOzonReadTools
    ? ozonReadGateMessage
    : !openAiConfigReady
      ? c.gates.useOpenClawWithoutImageApi
      : imageGenerationIssue
        ? imageGenerationIssue
        : "";

  const queueStats = useMemo(() => {
    const pending = tasks.filter((task) => task.state === "pending_approval").length;
    const queued = tasks.filter((task) => task.state === "queued").length;
    const done = tasks.filter((task) => task.state === "succeeded").length;
    return { pending, queued, done };
  }, [tasks]);

  useEffect(() => {
    window.localStorage.setItem("ozon-local-locale", locale);
    document.documentElement.lang = locale;
  }, [locale]);

  async function api(path: string, init: RequestInit = {}) {
    return fetch(`${runtime.skill_api}${path}`, {
      ...init,
      headers: {
        "Content-Type": "application/json",
        "x-local-token": runtime.local_token,
        ...(init.headers ?? {})
      }
    });
  }

  function ensureOzonReadAccess(action: string) {
    if (canUseOzonReadTools) {
      return true;
    }
    setMessage(c.gates.blocked(action, ozonReadGateMessage));
    return false;
  }

  function userFacingError(error: string | undefined) {
    const raw = error?.trim() || c.messages.unknownError;
    if (raw.includes("Ozon credentials are not configured")) {
      return c.errors.credentialsMissing;
    }
    if (raw.includes("OZON_CLIENT_ID/OZON_API_KEY environment credentials are incomplete")) {
      return c.errors.envCredentialsIncomplete;
    }
    if (raw.includes("ozon connector failed")) {
      return raw.replace(/^ozon connector failed:\s*/i, "");
    }
    if (raw.includes("cloud lease is not installed")) {
      return c.errors.cloudLeaseMissing;
    }
    if (raw.includes("lease public key") || raw.includes("stored cloud lease is invalid")) {
      return c.errors.cloudLeaseInvalid;
    }
    if (
      raw.includes("No available channel for model") ||
      raw.includes("image model is not available")
    ) {
      return c.errors.imageModelUnavailable;
    }
    if (raw.includes("OpenAI API key is required")) {
      return c.errors.openAiKeyRequired;
    }
    return raw;
  }

  async function refresh() {
    const [tasksResponse, scheduleResponse] = await Promise.all([
      api("/tasks"),
      api("/schedules/ecommerce-read")
    ]);
    if (tasksResponse.ok) {
      setTasks(await tasksResponse.json());
    }
    if (scheduleResponse.ok) {
      const nextSchedule: ScheduleStatus = await scheduleResponse.json();
      setSchedule(nextSchedule);
      setScheduleInterval(nextSchedule.interval_secs);
      setScheduleLimit(nextSchedule.limit);
    }
  }

  async function checkHealth(): Promise<ConfigStatus | null> {
    try {
      const [healthResponse, manifestResponse, configResponse] = await Promise.all([
        fetch(`${runtime.skill_api}/health`),
        fetch(`${runtime.skill_api}/openclaw/manifest`),
        api("/config/status")
      ]);
      setHealth(healthResponse.ok ? await healthResponse.json() : null);
      setManifest(manifestResponse.ok ? await manifestResponse.json() : null);
      let nextConfig: ConfigStatus | null = null;
      if (configResponse.ok) {
        nextConfig = await configResponse.json();
        setConfigStatus(nextConfig);
        setOpenAiBaseUrl(nextConfig!.openai.base_url);
        setOpenAiImageModel(nextConfig!.openai.image_model);
      } else {
        setConfigStatus(null);
      }
      setMessage(healthResponse.ok ? c.messages.localServiceConnected : c.messages.localServiceNotReady);
      return nextConfig;
    } catch {
      setHealth(null);
      setConfigStatus(null);
      setMessage(c.messages.localServiceUnreachable);
      return null;
    }
  }

  async function saveConfig() {
    if (!clientId.trim() || !apiKey.trim()) {
      setMessage(c.messages.fillOzonCredentials);
      return;
    }
    try {
      const response = await api("/config/ozon", {
        method: "POST",
        body: JSON.stringify({ client_id: clientId.trim(), api_key: apiKey.trim() })
      });
      const data = await response.json();
      if (!response.ok) {
        setMessage(c.messages.saveFailed(userFacingError(data.error)));
        return;
      }
      setApiKey("");
      const refreshedConfig = await checkHealth();

      const validationResponse = await api("/config/ozon/validate", { method: "POST" });
      const validationData = await validationResponse.json();
      if (!validationResponse.ok) {
        setValidation(null);
        setMessage(c.messages.ozonSavedValidationFailed(userFacingError(validationData.error)));
        return;
      }
      setValidation(validationData);
      setMessage(c.messages.ozonSavedValidated(data.client_id, refreshedConfig?.ozon.api_key_fingerprint ?? c.advanced.notSaved));
    } catch {
      setMessage(c.messages.saveFailed(c.messages.localServiceUnreachable));
    }
  }

  async function saveOpenAiConfig() {
    const keyIsBlank = !openAiApiKey.trim();
    const canReuseStoredOpenAiKey =
      configStatus?.openai.configured && configStatus.openai.source !== "env";
    if (keyIsBlank && !canReuseStoredOpenAiKey) {
      setMessage(
        configStatus?.openai.source === "env"
          ? c.messages.envOpenAiKeyMustBeReentered
          : c.messages.firstOpenAiKeyRequired
      );
      return;
    }
    try {
      const response = await api("/config/openai", {
        method: "POST",
        body: JSON.stringify({
          api_key: openAiApiKey.trim(),
          base_url: openAiBaseUrl.trim(),
          image_model: openAiImageModel.trim()
        })
      });
      const data = await response.json();
      setMessage(
        response.ok
          ? c.messages.imageApiSaved(data.base_url, data.image_model, data.api_key_fingerprint)
          : c.messages.imageApiSaveFailed(userFacingError(data.error))
      );
      if (response.ok) {
        setOpenAiApiKey("");
        setImageGenerationIssue(null);
        await checkHealth();
      }
    } catch {
      setMessage(c.messages.imageApiSaveFailed(c.messages.localServiceUnreachable));
    }
  }

  async function validateConfig() {
    try {
      const response = await api("/config/ozon/validate", { method: "POST" });
      const data = await response.json();
      if (!response.ok) {
        setValidation(null);
        setMessage(c.messages.ozonValidationFailed(userFacingError(data.error)));
        return;
      }
      setValidation(data);
      setMessage(validationMessage(data, c));
      await checkHealth();
    } catch {
      setValidation(null);
      setMessage(c.messages.ozonValidationFailed(c.messages.localServiceUnreachable));
    }
  }

  async function loadProducts() {
    if (!ensureOzonReadAccess(c.actions.readProducts)) return;
    try {
      const [countResponse, listResponse] = await Promise.all([
        api("/tools/ozon.products.count", { method: "POST" }),
        api("/tools/ozon.products.list", {
          method: "POST",
          body: JSON.stringify({ limit: 3 })
        })
      ]);
      const countData = await countResponse.json();
      const listData = await listResponse.json();
      if (!countResponse.ok || !listResponse.ok) {
        setMessage(c.messages.ozonReadFailed(userFacingError(countData.error ?? listData.error)));
        return;
      }
      const count = countData as ProductCountResult;
      const list = listData as ProductListResult;
      setProductCount(count.count);
      setProducts(list.products);
      setProductListMeta(list);
      if (list.archived_fallback || count.archived_fallback) {
        setMessage(c.messages.archivedLoaded(count.count));
        return;
      }
      setMessage(
        list.connector_mode === "real"
          ? c.messages.productReadReal(count.count, list.products.length)
          : c.messages.productReadMock
      );
    } catch {
      setMessage(c.messages.ozonReadFailed(c.messages.localServiceUnreachable));
    }
  }

  async function loadProductDetail(lookup: { offer_id?: string; product_id?: string; sku?: string }) {
    if (!ensureOzonReadAccess(c.actions.readDetails)) return;
    const cleanLookup = Object.fromEntries(
      Object.entries(lookup).filter(([, value]) => value && value.trim())
    );
    if (Object.keys(cleanLookup).length !== 1) {
      setMessage(c.messages.lookupRequired);
      return;
    }
    try {
      const response = await api("/tools/ozon.products.get", {
        method: "POST",
        body: JSON.stringify(cleanLookup)
      });
      const data = await response.json();
      if (!response.ok) {
        setProductDetail(null);
        setMessage(c.messages.productDetailFailed(userFacingError(data.error)));
        return;
      }
      setProductDetail(data);
      resetPosterWorkbench();
      setMessage(
        data.connector_mode === "real"
          ? c.messages.productDetailReal(data.product.offer_id, data.product.images.length)
          : c.messages.productDetailMock(data.product.offer_id)
      );
    } catch {
      setProductDetail(null);
      setMessage(c.messages.productDetailFailed(c.messages.localServiceUnreachable));
    }
  }

  function resetPosterWorkbench() {
    setPosterBrief(null);
    setPosterHandoff(null);
    setPosterBackground(null);
    setPosterVerification(null);
    setPosterHeadline("");
    setPosterSubheadline("");
    setPosterSellingPoints(["", "", ""]);
    setPosterCtaLine("");
    setPosterComplianceNote("");
  }

  function applyPosterBrief(nextBrief: PosterBrief) {
    const paddedPoints = [...nextBrief.selling_points];
    while (paddedPoints.length < 3) {
      paddedPoints.push("");
    }
    setPosterHeadline(nextBrief.headline);
    setPosterSubheadline(nextBrief.subheadline);
    setPosterSellingPoints(paddedPoints.slice(0, 3));
    setPosterCtaLine(nextBrief.cta_line);
    setPosterComplianceNote(nextBrief.compliance_note);
  }

  function currentLookupPayload() {
    if (productDetail) {
      if (productDetail.product.lookup.offer_id) {
        return { offer_id: productDetail.product.lookup.offer_id };
      }
      if (productDetail.product.lookup.product_id) {
        return { product_id: productDetail.product.lookup.product_id };
      }
      if (productDetail.product.lookup.sku) {
        return { sku: productDetail.product.lookup.sku };
      }
      return { offer_id: productDetail.product.offer_id };
    }
    const value = productLookup.trim();
    if (!value) {
      return null;
    }
    return parseProductLookupInput(value);
  }

  async function buildPosterBrief() {
    if (!ensureOzonReadAccess(c.actions.buildPosterBrief)) return;
    const lookup = currentLookupPayload();
    if (!lookup) {
      setMessage(c.messages.needProductForBrief);
      return;
    }
    try {
      const response = await api("/poster/brief", {
        method: "POST",
        body: JSON.stringify({ ...lookup, theme: posterTheme, locale })
      });
      const data = await response.json();
      if (!response.ok) {
        setPosterBrief(null);
        setMessage(c.messages.posterBriefFailed(userFacingError(data.error)));
        return;
      }
      setProductDetail({ connector_mode: data.connector_mode, product: data.product });
      setPosterBrief(data);
      setPosterBackground(null);
      setPosterHandoff(null);
      setPosterVerification(null);
      applyPosterBrief(data.brief);
      setMessage(c.messages.posterBriefReady);
    } catch {
      setPosterBrief(null);
      setMessage(c.messages.posterBriefFailed(c.messages.localServiceUnreachable));
    }
  }

  async function copyPosterHandoff() {
    if (!ensureOzonReadAccess(c.actions.preparePosterHandoff)) return;
    const lookup = currentLookupPayload();
    if (!lookup) {
      setMessage(c.messages.needProductForHandoff);
      return;
    }
    try {
      const response = await api("/poster/handoff", {
        method: "POST",
        body: JSON.stringify({ ...lookup, theme: posterTheme, locale })
      });
      const data = await response.json();
      if (!response.ok) {
        setPosterHandoff(null);
        setMessage(c.messages.posterHandoffFailed(userFacingError(data.error)));
        return;
      }
      setProductDetail({ connector_mode: data.connector_mode, product: data.product });
      setPosterBrief({ connector_mode: data.connector_mode, product: data.product, brief: data.brief });
      setPosterHandoff(data);
      setPosterBackground(null);
      setPosterVerification(null);
      applyPosterBrief(data.brief);
      await navigator.clipboard.writeText(data.prompt);
      setMessage(c.messages.posterHandoffCopied(data.product.offer_id, data.source_images.length));
    } catch {
      setPosterHandoff(null);
      setMessage(c.messages.posterHandoffFailed(c.messages.localServiceUnreachable));
    }
  }

  async function generatePosterBackground() {
    if (!canGeneratePosterBackground) {
      setMessage(c.messages.posterBackgroundBlocked(posterGenerationGateMessage));
      return;
    }
    const lookup = currentLookupPayload();
    if (!lookup) {
      setMessage(c.messages.needProductForBackground);
      return;
    }
    try {
      const response = await api("/poster/generate", {
        method: "POST",
        body: JSON.stringify({ ...lookup, theme: posterTheme, locale })
      });
      const data = await response.json();
      if (!response.ok) {
        setPosterBackground(null);
        const issue = userFacingError(data.error);
        if (issue === c.errors.imageModelUnavailable) {
          setImageGenerationIssue(issue);
        }
        setMessage(c.messages.posterBackgroundFailed(issue));
        return;
      }
      setImageGenerationIssue(null);
      setProductDetail({ connector_mode: data.connector_mode, product: data.product });
      setPosterBrief({ connector_mode: data.connector_mode, product: data.product, brief: data.brief });
      setPosterHandoff(null);
      setPosterBackground(data);
      setPosterVerification(null);
      applyPosterBrief(data.brief);
      setMessage(c.messages.posterBackgroundReady(data.image_model));
    } catch {
      setPosterBackground(null);
      setMessage(c.messages.posterBackgroundFailed(c.messages.localServiceUnreachable));
    }
  }

  async function verifyPosterCopy() {
    if (!ensureOzonReadAccess(c.actions.verifyPosterCopy)) return;
    const lookup = currentLookupPayload();
    if (!lookup) {
      setMessage(c.messages.needProductForVerify);
      return;
    }
    try {
      const response = await api("/poster/verify", {
        method: "POST",
        body: JSON.stringify({
          ...lookup,
          theme: posterTheme,
          locale,
          headline: posterHeadline,
          subheadline: posterSubheadline,
          selling_points: posterSellingPoints.filter((value) => value.trim()),
          cta_line: posterCtaLine,
          compliance_note: posterComplianceNote
        })
      });
      const data = await response.json();
      if (!response.ok) {
        setPosterVerification(null);
        setMessage(c.messages.posterVerifyFailed(userFacingError(data.error)));
        return;
      }
      setPosterVerification(data);
      setMessage(data.ok ? c.messages.posterVerifyPassed : c.messages.posterVerifyFailedCopy);
    } catch {
      setPosterVerification(null);
      setMessage(c.messages.posterVerifyFailed(c.messages.localServiceUnreachable));
    }
  }

  function updatePosterSellingPoint(index: number, value: string) {
    setPosterSellingPoints((current) => current.map((item, itemIndex) => (itemIndex === index ? value : item)));
  }

  const posterProductImage =
    productDetail?.product.primary_image ??
    posterBackground?.product.primary_image ??
    posterBrief?.product.primary_image ??
    null;

  async function loadProductDetailFromInput() {
    const value = productLookup.trim();
    if (!value) {
      setMessage(c.messages.inputLookup);
      return;
    }
    const lookup = parseProductLookupInput(value);
    if (!lookup) {
      setMessage(c.messages.invalidLookup);
      return;
    }
    await loadProductDetail(lookup);
  }

  async function createDryRun() {
    try {
      const response = await api("/tasks/dry-run", {
        method: "POST",
        body: JSON.stringify({
          operation: selectedOperation,
          source: "open_claw",
          shop_id: "default-shop",
          risk: selectedOperation.includes("mock") ? "high" : "medium",
          idempotency_key: `ui-${selectedOperation}-${Date.now()}`
        })
      });
      const data = await response.json();
      setMessage(response.ok ? c.messages.dryRunCreated : c.messages.dryRunCreateFailed(userFacingError(data.error)));
      await refresh();
    } catch {
      setMessage(c.messages.dryRunCreateFailed(c.messages.localServiceUnreachable));
    }
  }

  async function approve(taskId: string) {
    try {
      const response = await api(`/tasks/${taskId}/approve`, {
        method: "POST",
        body: JSON.stringify({ approved_by: "local-ui", note: "approved in local operator console" })
      });
      const data = await response.json();
      setMessage(response.ok ? c.messages.approved : c.messages.approveFailed(userFacingError(data.error)));
      await refresh();
    } catch {
      setMessage(c.messages.approveFailed(c.messages.localServiceUnreachable));
    }
  }

  async function execute(taskId: string) {
    try {
      const response = await api(`/tasks/${taskId}/execute-mock`, { method: "POST" });
      const data = await response.json();
      setMessage(response.ok ? c.messages.executed : c.messages.executeFailed(userFacingError(data.error)));
      await refresh();
    } catch {
      setMessage(c.messages.executeFailed(c.messages.localServiceUnreachable));
    }
  }

  async function configureSchedule(enabled: boolean) {
    if (enabled && !ensureOzonReadAccess(c.actions.enableSchedule)) return;
    try {
      const response = await api("/schedules/ecommerce-read", {
        method: "POST",
        body: JSON.stringify({
          enabled,
          interval_secs: scheduleInterval,
          limit: scheduleLimit
        })
      });
      const data = await response.json();
      if (!response.ok) {
        setMessage(c.messages.scheduleConfigFailed(userFacingError(data.error)));
        return;
      }
      setSchedule(data);
      setMessage(enabled ? c.messages.scheduleEnabled : c.messages.scheduleStopped);
    } catch {
      setMessage(c.messages.scheduleConfigFailed(c.messages.localServiceUnreachable));
    }
  }

  async function runScheduleNow() {
    if (!ensureOzonReadAccess(c.actions.runNow)) return;
    try {
      const response = await api("/schedules/ecommerce-read/run-now", { method: "POST" });
      const data = await response.json();
      if (!response.ok) {
        setMessage(c.messages.manualRunFailed(userFacingError(data.error)));
        return;
      }
      setProducts(data.run.products);
      setProductCount(data.run.product_count);
      setMessage(c.messages.manualRunDone(data.run.sample_size, data.run.duration_ms));
      await refresh();
    } catch {
      setMessage(c.messages.manualRunFailed(c.messages.localServiceUnreachable));
    }
  }

  async function copyManifest() {
    await navigator.clipboard.writeText(`${runtime.skill_api}/openclaw/manifest`);
    setMessage(c.messages.manifestCopied);
  }

  async function copyOpenClawPairingLink() {
    if (!openClawBridgeReady) {
      setMessage(c.messages.nodeNotReadyRefresh);
      return;
    }
    try {
      const response = await api("/openclaw/pairing/start", { method: "POST" });
      const data = (await response.json()) as OpenClawPairingStart & { error?: string };
      if (!response.ok) {
        setMessage(c.messages.pairingLinkFailed(userFacingError(data.error ?? "unknown error")));
        return;
      }
      await navigator.clipboard.writeText(data.bind_url);
      setMessage(c.messages.pairingLinkCopied);
    } catch {
      setMessage(c.messages.pairingLinkFailed(c.messages.localServiceUnreachable));
    }
  }

  async function startOpenClawAutoBinding() {
    if (!openClawBridgeReady) {
      setMessage(c.messages.nodeNotReadyRecheck);
      return;
    }
    let data: OpenClawPairingStart & { error?: string };
    try {
      const response = await api("/openclaw/pairing/start", { method: "POST" });
      data = (await response.json()) as OpenClawPairingStart & { error?: string };
      if (!response.ok) {
        setMessage(c.messages.autoBindingFailed(userFacingError(data.error ?? "unknown error")));
        return;
      }
    } catch {
      setMessage(c.messages.autoBindingFailed(c.messages.localServiceUnreachable));
      return;
    }
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("open_openclaw_binding_url", { bindUrl: data.bind_url });
    } catch (error) {
      const errorText = error instanceof Error ? error.message : String(error);
      if (isBindingUrlSafetyError(errorText)) {
        setMessage(c.messages.bindingRejected(userFacingError(errorText)));
        return;
      }
      await navigator.clipboard.writeText(data.bind_url);
      setMessage(c.messages.bindingLinkCopiedFallback);
      return;
    }
    setMessage(c.messages.bindingOpened);
  }

  async function restartSidecar() {
    setMessage(c.messages.restartingNode);
    try {
      const nextRuntime = await restartRuntimeConfig();
      setRuntime(nextRuntime);
      setRuntimeReady(true);
      setEventState("connecting");
      setMessage(c.messages.restartRequested);
      window.setTimeout(() => {
        checkHealth();
      }, 1200);
    } catch (error) {
      setMessage(error instanceof Error ? error.message : c.messages.restartUnsupported);
    }
  }

  useEffect(() => {
    let cancelled = false;
    async function syncRuntime(markReady = false) {
      const nextRuntime = await loadRuntimeConfig();
      if (cancelled) {
        return;
      }
      setRuntime(nextRuntime);
      if (markReady) {
        setRuntimeReady(true);
      }
    }

    syncRuntime(true);
    const interval = window.setInterval(() => {
      syncRuntime();
    }, 3000);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, []);

  useEffect(() => {
    if (!runtimeReady) {
      return;
    }

    checkHealth();
    refresh();
    let cancelled = false;
    let attempt = 0;
    let reconnectTimer: number | null = null;
    let controller: AbortController | null = null;
    let reader: ReadableStreamDefaultReader<Uint8Array> | null = null;

    function scheduleReconnect() {
      if (cancelled || reconnectTimer !== null) {
        return;
      }
      const delay = Math.min(1000 * 2 ** attempt, 30000);
      attempt += 1;
      setEventState("connecting");
      reconnectTimer = window.setTimeout(() => {
        reconnectTimer = null;
        connectEvents();
      }, delay);
    }

    async function connectEvents() {
      if (cancelled) {
        return;
      }
      controller = new AbortController();
      try {
        const response = await fetch(`${runtime.agent_api}/events`, {
          headers: { "x-local-token": runtime.local_token },
          signal: controller.signal
        });
        if (!response.ok || !response.body) {
          setEventState("offline");
          scheduleReconnect();
          return;
        }
        attempt = 0;
        setEventState("connected");
        reader = response.body.getReader();
        const decoder = new TextDecoder();
        let buffer = "";
        while (!cancelled) {
          const { value, done } = await reader.read();
          if (done) {
            break;
          }
          buffer += decoder.decode(value, { stream: true });
          let boundary = buffer.indexOf("\n\n");
          while (boundary !== -1) {
            const event = buffer.slice(0, boundary);
            buffer = buffer.slice(boundary + 2);
            if (event.includes("event: task.changed")) {
              refresh();
            }
            boundary = buffer.indexOf("\n\n");
          }
        }
        if (!cancelled) {
          setEventState("offline");
          scheduleReconnect();
        }
      } catch {
        if (!cancelled) {
          setEventState("offline");
          scheduleReconnect();
        }
      }
    }

    connectEvents();
    return () => {
      cancelled = true;
      if (reconnectTimer !== null) {
        window.clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
      reader?.cancel().catch(() => {});
      controller?.abort();
    };
  }, [runtimeReady, runtime.skill_api, runtime.agent_api, runtime.local_token]);

  return (
    <main>
      <nav>
        <div>
          <strong>{c.app.title}</strong>
          <span>{c.app.subtitle}</span>
        </div>
        <label className="language-switch">
          <span>{c.language.label}</span>
          <select value={locale} onChange={(event) => setLocale(event.target.value as Locale)}>
            {localeOptions.map((option) => (
              <option key={option.locale} value={option.locale}>
                {option.locale === "zh-CN" ? c.language.zh : c.language.en}
              </option>
            ))}
          </select>
        </label>
        <span className={health ? "ok" : "warn"}>{health ? c.app.ready : c.app.offline}</span>
      </nav>

      <section className="overview">
        <div>
          <Activity />
          <span>{c.overview.localService}</span>
          <strong>{runtime.skill_api.replace("http://", "")}</strong>
        </div>
        <div>
          <Radio />
          <span>{c.overview.openClawConnection}</span>
          <strong>{eventState === "connected" ? c.overview.connected : c.overview.checking}</strong>
        </div>
        <div>
          <ListChecks />
          <span>{c.overview.pendingApproval}</span>
          <strong>{queueStats.pending}</strong>
        </div>
        <div>
          <ShieldCheck />
          <span>{c.overview.writePolicy}</span>
          <strong>{c.overview.writePolicyValue}</strong>
        </div>
        <div>
          <SlidersHorizontal />
          <span>{c.overview.productMode}</span>
          <strong>{configStatus?.connector_mode === "real" ? c.overview.realRead : (configStatus?.connector_mode ?? runtime.connector_mode)}</strong>
        </div>
        <div className="overview-sidecar">
          <TerminalSquare />
          <span>{c.overview.desktopNode}</span>
          <strong>{sidecarSummary(runtime, c)}</strong>
          <button className="mini-action" onClick={restartSidecar} title={c.overview.restartNode}>
            <RefreshCcw size={15} />
          </button>
        </div>
        <div>
          <Repeat2 />
          <span>{c.overview.readSchedule}</span>
          <strong>{schedule?.enabled ? c.overview.scheduleOn : c.overview.schedulePaused}</strong>
        </div>
      </section>

      <section className={`runtime-strip ${isConnectedRuntime(runtime) ? "runtime-ok" : "runtime-warn"}`}>
        <div>
          <strong>{sidecarStatusLabel(runtime, c)}</strong>
          <span>{sidecarDiagnostic(runtime, c)}</span>
        </div>
        <button className="secondary-button" onClick={restartSidecar}>
          <RefreshCcw size={16} />
          {c.overview.restartNode}
        </button>
      </section>

      <section className="runtime-strip runtime-ok">
        <div>
          <strong>{c.protocol.title(health?.protocol_version ?? c.protocol.waiting)}</strong>
          <span>
            {health
              ? c.protocol.detail(
                  health.package_version ?? c.advanced.values.unknown,
                  shortCommit(health.build_commit, c.advanced.values.unknown),
                  health.supervisor ?? c.protocol.unknownSupervisor
                )
              : c.protocol.pending}
          </span>
        </div>
      </section>

      <section className="setup-guide">
        <div className="section-title">
          <Workflow />
          <div>
            <h1>{c.setup.title}</h1>
            <p>{c.setup.description}</p>
          </div>
        </div>
        <div className="setup-steps">
          {setupSteps.map((step) => (
            <div className={`setup-step ${step.state}`} key={step.number}>
              <span className="step-number">{step.number}</span>
              <div>
                <strong>{step.title}</strong>
                <em>{step.status}</em>
                <p>{step.detail}</p>
              </div>
            </div>
          ))}
        </div>
        <div className="setup-action-row">
          <button disabled={!openClawBridgeReady} onClick={startOpenClawAutoBinding}>
            <Workflow size={18} /> {c.setup.actions.autoBind}
          </button>
          <button className="secondary-button" disabled={!openClawBridgeReady} onClick={copyOpenClawPairingLink}>
            <Clipboard size={16} /> {c.setup.actions.copyPairing}
          </button>
          <button className="secondary-button" onClick={refresh}>
            <RefreshCcw size={16} /> {c.setup.actions.recheck}
          </button>
          <p>{c.setup.note}</p>
        </div>
      </section>

      <section className="grid">
        <div className="panel bridge-panel">
          <div className="section-title">
            <Workflow />
            <div>
              <h2>{c.bridge.title}</h2>
              <p>{c.bridge.description}</p>
            </div>
          </div>
          <details className="advanced-connect">
            <summary>{c.bridge.advancedSummary}</summary>
            <div className="bridge-endpoint">
              <code>{runtime.skill_api}/openclaw/manifest</code>
              <button className="icon-button" onClick={copyManifest} title={c.bridge.copyManifestTitle}>
                <Clipboard size={18} />
              </button>
            </div>
            <div className="tool-list compact-tools">
              {(manifest?.tools ?? []).map((tool) => (
                <div key={tool.name}>
                  <span>{tool.risk}</span>
                  <strong>{tool.name}</strong>
                  <em>{tool.approval_required ? c.bridge.approvalRequired : c.bridge.readOnly}</em>
                </div>
              ))}
            </div>
          </details>
        </div>

        <div className="panel config-panel">
          <div className="section-title">
            <KeyRound />
            <div>
              <h2>{c.config.title}</h2>
              <p>{c.config.description}</p>
            </div>
          </div>
          <p className="notice mode-notice">
            {configStatus?.real_ozon_enabled
              ? c.config.realModeNotice
              : c.config.mockModeNotice}
          </p>
          <div className="form-grid compact">
            <label>
              Ozon Client ID
              <input
                autoComplete="off"
                placeholder={c.config.clientIdPlaceholder}
                value={clientId}
                onChange={(event) => setClientId(event.target.value)}
              />
            </label>
            <label>
              Ozon API Key
              <input
                autoComplete="off"
                placeholder={c.config.apiKeyPlaceholder}
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
                type="password"
              />
            </label>
          </div>
          <button onClick={saveConfig}>
            <CheckCircle2 size={18} /> {c.config.saveAndValidate}
          </button>
        </div>

        <details className="advanced-local-settings">
          <summary>{c.advanced.summary}</summary>
          <div className="advanced-local-grid">
            <div className="panel config-panel">
              <div className="section-title">
                  <Sparkles />
                  <div>
                  <h2>{c.advanced.imageApiTitle}</h2>
                  <p>{c.advanced.imageApiDescription}</p>
                </div>
              </div>
              <div className="form-grid compact">
                <label>
                  Base URL
                  <input
                    autoComplete="off"
                    placeholder={c.advanced.baseUrlPlaceholder}
                    value={openAiBaseUrl}
                    onChange={(event) => setOpenAiBaseUrl(event.target.value)}
                  />
                </label>
                <label>
                  Image model
                  <input
                    autoComplete="off"
                    placeholder="gpt-image-1"
                    value={openAiImageModel}
                    onChange={(event) => setOpenAiImageModel(event.target.value)}
                  />
                </label>
                <label>
                  API Key
                  <input
                    autoComplete="off"
                    placeholder={configStatus?.openai.configured ? c.advanced.apiKeyReusePlaceholder : c.advanced.apiKeyFirstPlaceholder}
                    value={openAiApiKey}
                    onChange={(event) => setOpenAiApiKey(event.target.value)}
                    type="password"
                  />
                </label>
              </div>
              <button className="secondary-button" onClick={saveOpenAiConfig}>
                <CheckCircle2 size={18} /> {c.advanced.saveImageConfig}
              </button>
              {imageGenerationIssue && <p className="notice warn-text">{imageGenerationIssue}</p>}
            </div>

            <div className="panel diagnostics-panel">
              <div className="section-title">
                <SlidersHorizontal />
                <div>
                  <h2>{c.advanced.diagnosticsTitle}</h2>
                  <p>{c.advanced.diagnosticsDescription}</p>
                </div>
              </div>
              <div className="inline-actions">
                <button onClick={checkHealth}>
                  <RefreshCcw size={18} /> {c.advanced.refreshDiagnostics}
                </button>
                <button onClick={validateConfig}>
                  <ShieldCheck size={18} /> {c.advanced.validateOzonCredentials}
                </button>
              </div>
              <div className="status-list">
                <div className="status-item">
                  <span>{c.advanced.labels.connector}</span>
                  <strong>{configStatus?.connector_mode ?? c.advanced.values.unknown}</strong>
                  <em className={configStatus?.real_ozon_enabled ? "badge warn-badge" : "badge ok-badge"}>
                    {configStatus?.real_ozon_enabled ? c.advanced.values.realApi : c.advanced.values.mock}
                  </em>
                </div>
                <div className="status-item">
                  <span>{c.advanced.labels.secretStore}</span>
                  <strong>{configStatus?.secret_store.backend ?? "system_keyring"}</strong>
                  <em className={configStatus?.secret_store.available ? "badge ok-badge" : "badge warn-badge"}>
                    {configStatus?.secret_store.available ? c.advanced.values.available : c.advanced.values.unavailable}
                  </em>
                </div>
                <div className="status-item">
                  <span>{c.advanced.labels.ozonConfig}</span>
                  <strong>{configStatus?.ozon.configured ? c.advanced.values.configured : c.advanced.values.notConfigured}</strong>
                  <em className="badge neutral-badge">{configStatus?.ozon.source ?? c.advanced.values.checking}</em>
                </div>
                <div className="status-item">
                  <span>{c.advanced.labels.clientId}</span>
                  <strong>{configStatus?.ozon.client_id ?? c.advanced.notSaved}</strong>
                </div>
                <div className="status-item">
                  <span>{c.advanced.labels.apiKeyFingerprint}</span>
                  <strong>{configStatus?.ozon.api_key_fingerprint ?? c.advanced.notSaved}</strong>
                </div>
                <div className="status-item">
                  <span>{c.advanced.labels.imageApi}</span>
                  <strong>{configStatus?.openai.configured ? configStatus.openai.base_url : c.advanced.values.notConfigured}</strong>
                  <em className={configStatus?.openai.configured ? "badge ok-badge" : "badge warn-badge"}>
                    {configStatus?.openai.source ?? c.advanced.values.checking}
                  </em>
                </div>
                <div className="status-item">
                  <span>{c.advanced.labels.imageModel}</span>
                  <strong>{configStatus?.openai.image_model ?? c.advanced.notSaved}</strong>
                </div>
                <div className="status-item">
                  <span>{c.advanced.labels.lease}</span>
                  <strong>{configStatus?.lease.lease_id ?? c.advanced.notImported}</strong>
                  <em className={configStatus?.lease.valid ? "badge ok-badge" : "badge warn-badge"}>
                    {configStatus?.lease.valid ? c.advanced.values.valid : c.advanced.values.missing}
                  </em>
                </div>
              </div>
              {configStatus?.ozon.issue && <p className="notice warn-text">{userFacingError(configStatus.ozon.issue)}</p>}
              {configStatus?.openai.issue && <p className="notice warn-text">{userFacingError(configStatus.openai.issue)}</p>}
              {configStatus?.lease.issue && <p className="notice warn-text">{userFacingError(configStatus.lease.issue)}</p>}
              {validation && (
                <p className="notice">
                  {validation.checked_at} · {validationMessage(validation, c)}
                </p>
              )}
            </div>
          </div>
        </details>
      </section>

      <section className="workspace-grid">
        <div className="panel read-panel">
          <div className="section-title">
            <DatabaseZap />
            <div>
              <h2>{c.read.title}</h2>
              <p>{c.read.description}</p>
            </div>
          </div>
          <button disabled={!canUseOzonReadTools} onClick={loadProducts}>
            {c.read.loadProducts}
          </button>
          {!canUseOzonReadTools && <p className="notice warn-text">{ozonReadGateMessage}</p>}
          <div className="task-command product-lookup-command">
            <input
              placeholder={c.read.lookupPlaceholder}
              value={productLookup}
              onChange={(event) => setProductLookup(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  void loadProductDetailFromInput();
                }
              }}
            />
            <button disabled={!canUseOzonReadTools} onClick={loadProductDetailFromInput}>
              <ImageIcon size={18} /> {c.read.readDetails}
            </button>
          </div>
          <div className="metric-row">
            <span>{c.read.productCount}</span>
            <strong>{productCount ?? c.read.notRead}</strong>
          </div>
          <div className="read-meta">
            <span className={productListMeta?.connector_mode === "real" ? "badge ok-badge" : "badge neutral-badge"}>
              {productListMeta?.connector_mode === "real" ? c.read.realSellerData : c.read.notLoaded}
            </span>
            {productListMeta?.archived_fallback && <span className="badge warn-badge">{c.read.archivedBadge}</span>}
            {productListMeta?.visibility && <code>{productListMeta.visibility}</code>}
            {productListMeta?.last_id && <code>{c.read.nextCursor(String(productListMeta.last_id))}</code>}
          </div>
          {productListMeta?.archived_fallback && (
            <p className="notice warn-text">{c.read.archivedNotice}</p>
          )}
          <div className="product-list">
            {products.map((product) => (
              <div key={product.product_id}>
                <strong>{product.offer_id}</strong>
                <span>{product.name ?? c.read.productFallback(product.product_id)}</span>
                <em>
                  {(product.visibility ?? c.read.visibilityUnavailable)} · FBO {product.has_fbo_stocks ? c.read.yes : c.read.no} · FBS{" "}
                  {product.has_fbs_stocks ? c.read.yes : c.read.no}
                  {product.archived ? ` · ${c.read.archivedShort}` : ""}
                </em>
                <button
                  className="secondary-button"
                  disabled={!canUseOzonReadTools}
                  onClick={() => loadProductDetail({ offer_id: product.offer_id })}
                >
                  <ImageIcon size={16} /> {c.read.readDetails}
                </button>
              </div>
            ))}
          </div>
          {productDetail && (
            <div className="product-detail">
              <div className="section-title compact-title">
                <ImageIcon />
                <div>
                  <h3>{productDetail.product.name ?? productDetail.product.offer_id}</h3>
                  <p>{productDetail.product.offer_id} · {c.read.productMeta(productDetail.product.product_id, productDetail.connector_mode)}</p>
                </div>
              </div>
              <div className="image-strip">
                {productDetail.product.images.length === 0 && <p className="empty">{c.read.imageEmpty}</p>}
                {productDetail.product.images.slice(0, 6).map((image) => (
                  <a key={`${image.role}-${image.position}-${image.url}`} href={image.url} target="_blank" rel="noreferrer">
                    <img src={image.url} alt={`${image.role} ${image.position}`} />
                    <span>{image.role}</span>
                  </a>
                ))}
              </div>
              <div className="fact-grid">
                <div>
                  <span>{c.read.primaryImage}</span>
                  <strong>{productDetail.product.primary_image ? c.read.available : c.read.missing}</strong>
                </div>
                <div>
                  <span>{c.read.attributes}</span>
                  <strong>{productDetail.product.attributes.length}</strong>
                </div>
                <div>
                  <span>{c.read.barcodes}</span>
                  <strong>{productDetail.product.barcodes.length}</strong>
                </div>
              </div>
              {productDetail.product.attributes.length > 0 && (
                <div className="attribute-list">
                  {productDetail.product.attributes.slice(0, 8).map((attribute, index) => (
                    <p key={`${attribute.id ?? index}-${attribute.name ?? c.read.attributeFallback}`}>
                      <strong>{attribute.name ?? attribute.id ?? c.read.attributeFallback}</strong>
                      <span>{attribute.values.join(", ") || c.read.emptyValue}</span>
                    </p>
                  ))}
                </div>
              )}
              {productDetail.product.warnings.map((warning) => (
                <p className="notice warn-text" key={warning}>
                  {warning}
                </p>
              ))}
            </div>
          )}
          {(productDetail || posterBrief || posterBackground) && (
            <div className="poster-workbench">
              <div className="section-title compact-title">
                <Sparkles />
                <div>
                  <h3>{c.poster.title}</h3>
                  <p>{c.poster.description}</p>
                </div>
              </div>
              <div className="task-command poster-toolbar">
                <select value={posterTheme} onChange={(event) => setPosterTheme(event.target.value)}>
                  <option value="studio">{c.poster.themes.studio}</option>
                  <option value="spotlight">{c.poster.themes.spotlight}</option>
                  <option value="launch">{c.poster.themes.launch}</option>
                  <option value="lifestyle">{c.poster.themes.lifestyle}</option>
                </select>
                <button disabled={!canUseOzonReadTools} onClick={buildPosterBrief}>
                  <Sparkles size={18} /> {c.poster.buildBrief}
                </button>
                <button disabled={!canUseOzonReadTools} onClick={copyPosterHandoff}>
                  <Clipboard size={18} /> {c.poster.copyHandoff}
                </button>
                <button disabled={!canGeneratePosterBackground} onClick={generatePosterBackground}>
                  <ImageIcon size={18} /> {c.poster.apiGenerate}
                </button>
                <button className="secondary-button" disabled={!canUseOzonReadTools} onClick={verifyPosterCopy}>
                  <ShieldCheck size={16} /> {c.poster.verifyCopy}
                </button>
              </div>
              {!canGeneratePosterBackground && (
                <p className={`notice ${imageGenerationIssue ? "warn-text" : ""}`}>
                  {posterGenerationGateMessage || c.poster.apiUnavailable}
                </p>
              )}
              {posterBrief && (
                <div className="poster-editor">
                  <label>
                    {c.poster.headline}
                    <input value={posterHeadline} onChange={(event) => setPosterHeadline(event.target.value)} />
                  </label>
                  <label>
                    {c.poster.subheadline}
                    <input value={posterSubheadline} onChange={(event) => setPosterSubheadline(event.target.value)} />
                  </label>
                  {posterSellingPoints.map((point, index) => (
                    <label key={`poster-point-${index}`}>
                      {c.poster.sellingPoint(index + 1)}
                      <input value={point} onChange={(event) => updatePosterSellingPoint(index, event.target.value)} />
                    </label>
                  ))}
                  <label>
                    {c.poster.cta}
                    <input value={posterCtaLine} onChange={(event) => setPosterCtaLine(event.target.value)} />
                  </label>
                  <label className="full-span">
                    {c.poster.note}
                    <input value={posterComplianceNote} onChange={(event) => setPosterComplianceNote(event.target.value)} />
                  </label>
                </div>
              )}
              {(posterBrief || posterBackground) && (
                <div className="poster-preview-shell">
                  <div
                    className="poster-preview"
                    style={
                      posterBackground?.background_data_url
                        ? { backgroundImage: `linear-gradient(180deg, rgba(9, 17, 14, 0.18), rgba(9, 17, 14, 0.48)), url(${posterBackground.background_data_url})` }
                        : undefined
                    }
                  >
                    {!posterBackground?.background_data_url && <div className="poster-preview-fallback" />}
                    <div className="poster-copy">
                      <span className="poster-kicker">{posterBrief?.brief.theme ?? posterTheme}</span>
                      <h3>{posterHeadline || posterBrief?.brief.headline}</h3>
                      <p>{posterSubheadline || posterBrief?.brief.subheadline}</p>
                      <ul>
                        {posterSellingPoints.filter((value) => value.trim()).map((point) => (
                          <li key={point}>{point}</li>
                        ))}
                      </ul>
                      <strong>{posterCtaLine || posterBrief?.brief.cta_line}</strong>
                      <em>{posterComplianceNote || posterBrief?.brief.compliance_note}</em>
                    </div>
                    {posterProductImage && (
                      <img
                        className="poster-product"
                        src={posterProductImage}
                        alt={
                          productDetail?.product.name ??
                          posterBackground?.product.name ??
                          posterBrief?.product.name ??
                          productDetail?.product.offer_id ??
                          posterBackground?.product.offer_id ??
                          posterBrief?.product.offer_id ??
                          c.poster.productAlt
                        }
                      />
                    )}
                  </div>
                  <div className="poster-meta">
                    <div className="fact-grid">
                      <div>
                        <span>{c.poster.route}</span>
                        <strong>
                          {posterBackground?.image_model ?? (posterHandoff ? c.poster.viaOpenClaw : c.poster.notGenerated)}
                        </strong>
                      </div>
                      <div>
                        <span>{c.poster.theme}</span>
                        <strong>{posterBrief?.brief.theme ?? posterTheme}</strong>
                      </div>
                      <div>
                        <span>{c.poster.verification}</span>
                        <strong>{posterVerification ? (posterVerification.ok ? c.poster.passed : c.poster.needsFix) : c.poster.notChecked}</strong>
                      </div>
                    </div>
                    {posterHandoff && (
                      <div className="poster-handoff">
                      <div>
                          <span>{c.poster.copiedTitle}</span>
                          <p>{c.poster.copiedDescription(posterHandoff.source_images.length)}</p>
                        </div>
                        <button className="secondary-button" onClick={() => navigator.clipboard.writeText(posterHandoff.prompt)}>
                          <Clipboard size={16} /> {c.poster.copyAgain}
                        </button>
                      </div>
                    )}
                    {posterBackground && (
                      <div className="poster-prompt">
                        <span>{c.poster.backgroundPrompt}</span>
                        <p>{posterBackground.revised_prompt ?? posterBackground.prompt}</p>
                      </div>
                    )}
                    {posterVerification && (
                      <div className={`poster-verify ${posterVerification.ok ? "poster-verify-ok" : "poster-verify-warn"}`}>
                        {posterVerification.warnings.map((warning) => (
                          <p key={warning}>{warning}</p>
                        ))}
                        {posterVerification.mismatches.map((mismatch) => (
                          <p key={`${mismatch.field}-${mismatch.expected}`}>
                            {c.poster.mismatch(mismatch.field, mismatch.expected, mismatch.actual)}
                          </p>
                        ))}
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
          )}
        </div>

        <div className="panel schedule-panel">
          <div className="section-title">
            <Repeat2 />
            <div>
              <h2>{c.schedule.title}</h2>
              <p>{c.schedule.description}</p>
            </div>
          </div>
          <div className="task-command">
            <input
              min={60}
              max={86400}
              step={60}
              type="number"
              value={scheduleInterval}
              onChange={(event) => setScheduleInterval(Number(event.target.value))}
              title={c.schedule.intervalTitle}
            />
            <input
              min={1}
              max={100}
              type="number"
              value={scheduleLimit}
              onChange={(event) => setScheduleLimit(Number(event.target.value))}
              title={c.schedule.limitTitle}
            />
          </div>
          <div className="inline-actions">
            <button disabled={!canUseOzonReadTools} onClick={() => configureSchedule(true)}>
              <Play size={18} /> {c.schedule.enable}
            </button>
            <button onClick={() => configureSchedule(false)}>
              <PauseCircle size={18} /> {c.schedule.stop}
            </button>
            <button disabled={!canUseOzonReadTools} onClick={runScheduleNow}>
              <RefreshCcw size={18} /> {c.schedule.runNow}
            </button>
          </div>
          {!canUseOzonReadTools && <p className="notice warn-text">{ozonReadGateMessage}</p>}
          <div className="status-list schedule-status">
            <div className="status-item">
              <span>{c.schedule.status}</span>
              <strong>{schedule?.enabled ? c.schedule.enabled : c.schedule.paused}</strong>
              <em className={schedule?.enabled ? "badge ok-badge" : "badge neutral-badge"}>
                {schedule?.connector_mode ?? c.advanced.values.mock}
              </em>
            </div>
            <div className="status-item">
              <span>{c.schedule.lastCount}</span>
              <strong>{schedule?.last_run?.product_count ?? c.schedule.notRun}</strong>
            </div>
            <div className="status-item">
              <span>{c.schedule.lastSample}</span>
              <strong>{schedule?.last_run?.sample_size ?? c.schedule.notRun}</strong>
            </div>
          </div>
          {schedule?.last_error && <p className="notice warn-text">{schedule.last_error}</p>}
          <div className="audit compact-audit">
            {(schedule?.audit ?? []).slice(-4).map((item) => (
              <p key={`${item.at}-${item.action}`}>{item.at} · {item.action} · {item.summary}</p>
            ))}
          </div>
        </div>

        <div className="panel task-panel">
          <div className="section-title">
            <Play />
            <div>
              <h2>{c.tasks.title}</h2>
              <p>{c.tasks.description}</p>
            </div>
          </div>
          <div className="task-command">
            <select value={selectedOperation} onChange={(event) => setSelectedOperation(event.target.value)}>
              <option value="ozon_update_price_mock">{c.tasks.operations.ozon_update_price_mock}</option>
              <option value="ozon_update_inventory_mock">{c.tasks.operations.ozon_update_inventory_mock}</option>
              <option value="ozon_join_promotion_mock">{c.tasks.operations.ozon_join_promotion_mock}</option>
              <option value="draft_upload_mock">{c.tasks.operations.draft_upload_mock}</option>
              <option value="import1688_mock">{c.tasks.operations.import1688_mock}</option>
            </select>
            <button onClick={createDryRun}>{c.tasks.create}</button>
          </div>
          <div className="task-list">
            {tasks.length === 0 && <p className="empty">{c.tasks.empty}</p>}
            {tasks.map((task) => (
              <article key={task.id}>
                <div className="task-copy">
                  <span>{task.operation}</span>
                  <strong>{task.dry_run.summary}</strong>
                  <p>{task.dry_run.warnings.join(" · ") || c.tasks.noWarnings}</p>
                  {task.receipt && <code>{task.receipt.result_summary}</code>}
                </div>
                <div className="task-actions">
                  <em className={`state ${task.state}`}>{taskStateLabel(task.state, c)}</em>
                  <em>{taskRiskLabel(task.risk, c)}</em>
                  {task.state === "pending_approval" && <button onClick={() => approve(task.id)}>{c.tasks.approve}</button>}
                  {task.state === "queued" && <button onClick={() => execute(task.id)}>{c.tasks.execute}</button>}
                </div>
              </article>
            ))}
          </div>
        </div>
      </section>

      <footer>
        <TerminalSquare size={18} />
        <span>{message}</span>
        {health ? <CheckCircle2 size={18} /> : <XCircle size={18} />}
      </footer>
    </main>
  );
}

function defaultRuntimeConfig(): RuntimeConfig {
  return {
    skill_api: DEFAULT_LOCAL_API,
    agent_api: DEFAULT_AGENT_API,
    local_token: DEFAULT_LOCAL_TOKEN,
    connector_mode: import.meta.env.DEV ? "mock" : "real",
    sidecar_pid: null,
    sidecar_status: "external",
    sidecar_restart_count: 0,
    sidecar_last_started_at_ms: null,
    sidecar_last_exit: null,
    sidecar_last_error: null,
    sidecar_log_path: ""
  };
}

async function loadRuntimeConfig(): Promise<RuntimeConfig> {
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<RuntimeConfig>("local_node_runtime");
  } catch {
    return defaultRuntimeConfig();
  }
}

async function restartRuntimeConfig(): Promise<RuntimeConfig> {
  const { invoke } = await import("@tauri-apps/api/core");
  return await invoke<RuntimeConfig>("restart_local_node");
}

type LocalNodeCopy = ReturnType<typeof copyFor>;

function sidecarSummary(runtime: RuntimeConfig, c: LocalNodeCopy) {
  if (runtime.sidecar_status === "running" && runtime.sidecar_pid) {
    return `pid ${runtime.sidecar_pid}`;
  }
  if (runtime.sidecar_status === "external") {
    return c.sidecar.external;
  }
  if (runtime.sidecar_status === "blocked") {
    return c.sidecar.blocked;
  }
  return runtime.sidecar_status || "external";
}

function isConnectedRuntime(runtime: RuntimeConfig) {
  return runtime.sidecar_status === "running" || runtime.sidecar_status === "external";
}

function sidecarStatusLabel(runtime: RuntimeConfig, c: LocalNodeCopy) {
  if (runtime.sidecar_status === "running") {
    return runtime.sidecar_restart_count > 0
      ? c.sidecar.recovered(runtime.sidecar_restart_count)
      : c.sidecar.running;
  }
  if (runtime.sidecar_status === "failed") {
    return c.sidecar.failed;
  }
  if (runtime.sidecar_status === "restarting") {
    return c.sidecar.restarting;
  }
  if (runtime.sidecar_status === "external") {
    return c.sidecar.connected;
  }
  if (runtime.sidecar_status === "blocked") {
    return c.sidecar.portBlocked;
  }
  return c.sidecar.unknown;
}

function sidecarDiagnostic(runtime: RuntimeConfig, c: LocalNodeCopy) {
  const logPath = runtime.sidecar_log_path || c.sidecar.logUnavailable;
  if (runtime.sidecar_last_error) {
    if (runtime.sidecar_last_error === "existing_node_agent_port_not_ready") {
      return c.sidecar.existingAgentPortNotReady(logPath);
    }
    if (runtime.sidecar_last_error === "existing_node_token_rejected") {
      return c.sidecar.existingTokenRejected(logPath);
    }
    return c.sidecar.lastError(runtime.sidecar_last_error, logPath);
  }
  if (runtime.sidecar_last_exit) {
    return c.sidecar.lastExit(runtime.sidecar_last_exit, logPath);
  }
  if (runtime.sidecar_status === "running" && runtime.sidecar_pid) {
    const started = runtime.sidecar_last_started_at_ms
      ? new Date(runtime.sidecar_last_started_at_ms).toLocaleString()
      : c.sidecar.justNow;
    return c.sidecar.runningDiagnostic(started, logPath);
  }
  if (runtime.sidecar_status === "external") {
    return c.sidecar.externalDiagnostic(logPath);
  }
  return c.sidecar.defaultDiagnostic;
}

function validationMessage(validation: ValidationResult, c: LocalNodeCopy) {
  return validation.connector_mode === "real" ? c.advanced.validationReal : c.advanced.validationMock;
}

function taskStateLabel(state: string, c: LocalNodeCopy) {
  return c.tasks.states[state as keyof typeof c.tasks.states] ?? state;
}

function taskRiskLabel(risk: string, c: LocalNodeCopy) {
  return c.tasks.risks[risk as keyof typeof c.tasks.risks] ?? risk;
}

function shortCommit(value: string | undefined, fallback: string) {
  if (!value || value === "local-build") {
    return value ?? fallback;
  }
  return value.slice(0, 8);
}

function isBindingUrlSafetyError(errorText: string) {
  const normalized = errorText.toLowerCase();
  return (
    normalized.includes("binding url") ||
    normalized.includes("not allowed") ||
    normalized.includes("pairing fragment") ||
    normalized.includes("pairing code")
  );
}

function parseProductLookupInput(value: string): { offer_id?: string; product_id?: string; sku?: string } | null {
  const trimmed = value.trim();
  if (!trimmed) {
    return null;
  }
  const prefixed = trimmed.match(/^(offer|offer_id|product|product_id|sku)\s*:\s*(.+)$/i);
  if (prefixed) {
    const key = prefixed[1].toLowerCase();
    const content = prefixed[2].trim();
    if (!content) {
      return null;
    }
    if (key === "sku") {
      return { sku: content };
    }
    if (key === "product" || key === "product_id") {
      return { product_id: content };
    }
    return { offer_id: content };
  }
  return /^\d+$/.test(trimmed) ? { product_id: trimmed } : { offer_id: trimmed };
}

createRoot(document.getElementById("root")!).render(<App />);
