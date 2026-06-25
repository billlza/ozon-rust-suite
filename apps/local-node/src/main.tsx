import React, { useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  Activity,
  ArrowRight,
  Boxes,
  Clapperboard,
  CheckCircle2,
  Clipboard,
  Cpu,
  DatabaseZap,
  Download,
  Eye,
  EyeOff,
  FileSpreadsheet,
  FileText,
  Image as ImageIcon,
  KeyRound,
  Languages,
  Layers,
  ListChecks,
  PackagePlus,
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

// Module 2 acceptance state persists locally only (the user's own review
// convenience — never synced to the cloud) so a refresh/restart does not lose
// the generated before/after results and which ones were adopted.
const ACCEPTANCE_STORAGE_KEY = "ozon-local-acceptance";

type StoredAcceptance = {
  results: RelistItem[] | null;
  adopt: Record<string, boolean>;
  pushResults: RelistPushResult[] | null;
  // Selected candidate index per relist item key (default 0).
  choice: Record<string, number>;
};

// Migrate an older persisted RelistItem (which had `new_url` but no
// `candidates`) so prior saved reviews still render under the candidate model.
function migrateRelistItem(item: RelistItem): RelistItem {
  if (Array.isArray(item.candidates)) {
    return item;
  }
  const url = item.new_url ?? null;
  return { ...item, candidates: url ? [url] : [] };
}

function loadStoredAcceptance(): StoredAcceptance {
  try {
    const raw = window.localStorage.getItem(ACCEPTANCE_STORAGE_KEY);
    if (!raw) {
      return { results: null, adopt: {}, pushResults: null, choice: {} };
    }
    const parsed = JSON.parse(raw) as Partial<StoredAcceptance>;
    const results = parsed.results ? parsed.results.map(migrateRelistItem) : null;
    return {
      results,
      adopt: parsed.adopt ?? {},
      pushResults: parsed.pushResults ?? null,
      choice: parsed.choice ?? {}
    };
  } catch {
    return { results: null, adopt: {}, pushResults: null, choice: {} };
  }
}

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

type RelistItem = {
  product_id: string;
  offer_id: string;
  name: string | null;
  original_url: string | null;
  // Up to RELIST_MAX_CANDIDATES hosted restyle URLs; the operator picks one.
  candidates: string[];
  // Retained as optional for migration of older persisted reviews (pre-candidates).
  new_url?: string | null;
  error: string | null;
  // Set on rows imported from a supplier .xlsx (Module 1): these can be
  // relisted/exported but have no Ozon product_id, so push-by-product_id is N/A.
  imported?: boolean;
};

// Resolve the candidate the operator has selected for an item (default index 0),
// falling back to a migrated single new_url. Used at every render/push/export
// read site so selection stays consistent.
function selectedCandidateUrl(
  item: RelistItem,
  choice: number | undefined
): string | null {
  const idx = choice ?? 0;
  return item.candidates?.[idx] ?? item.candidates?.[0] ?? item.new_url ?? null;
}

type Module3Attribute = {
  name: string;
  values: string[];
};

type Module3Fields = {
  title: string;
  description: string;
  attributes: Module3Attribute[];
  type_category: string;
};

type Module3Item = {
  product_id: string;
  offer_id: string;
  source: Module3Fields;
  proposal: Module3Fields | null;
  error: string | null;
};

type RelistPushResult = {
  product_id: string;
  primary_url: string;
  image_count: number;
  ok: boolean;
  error: string | null;
};

// Module 6 (cloud image-to-video). A job is async: create -> poll -> hosted URL.
type VideoStatus = "queued" | "running" | "succeeded" | "failed";

type VideoJob = {
  id: string;
  status: VideoStatus;
  provider_job_id: string | null;
  video_url: string | null;
  error: string | null;
  created_at: string;
  updated_at: string;
  first_frame_url: string;
  last_frame_url: string | null;
  prompt: string;
  duration_seconds: number;
};

// Module 4 (export / delivery). One assembled row joins the module-2 adopted
// image (primary_image_url) with the module-3 redistributed title/listing,
// keyed by product_id, falling back to the product's source values.
type ExportRow = {
  product_id: string;
  offer_id: string;
  title: string;
  listing: string;
  primary_image_url: string | null;
  additional_image_urls: string[];
};

type ExportVerifySummary = {
  ok: boolean;
  expected_changes: number;
  unexpected_changes: number;
  frozen_cells_compared: number;
  sheets_compared: number;
};

type RelistExportResponse = {
  ok: boolean;
  out_path: string;
  file_url: string;
  verify: ExportVerifySummary | null;
  warnings: string[];
};

type Module3FieldChange = {
  before: string;
  after: string;
};

type Module3MatchedAttribute = {
  attribute_id: number;
  name: string;
  values: string[];
};

type Module3DroppedItem = {
  name: string;
  value: string | null;
  reason: string;
};

type Module3PushPreview = {
  title: Module3FieldChange;
  description: Module3FieldChange;
  attributes_to_write: Module3MatchedAttribute[];
  dropped: Module3DroppedItem[];
};

type Module3PushItem = {
  product_id: string;
  offer_id: string;
  preview: Module3PushPreview | null;
  written: boolean;
  task_id: string | null;
  error: string | null;
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
  capabilities?: {
    capability: string;
    ready: boolean;
    provider_kind?: string | null;
    base_url?: string | null;
    model?: string | null;
    secret_present?: boolean;
    issue?: string | null;
  }[];
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
  const [showApiKey, setShowApiKey] = useState(false);
  // Inline result shown right at the Ozon save form, so feedback is visible
  // without scrolling to the bottom status bar.
  const [saveNotice, setSaveNotice] = useState<{ tone: "ok" | "warn" | "info"; text: string } | null>(null);
  const [openAiBaseUrl, setOpenAiBaseUrl] = useState("https://api.openai.com");
  const [openAiImageModel, setOpenAiImageModel] = useState("gpt-image-1");
  const [openAiApiKey, setOpenAiApiKey] = useState("");
  // Provider-config UI (modules 3/6): secret save + add/replace provider entry.
  const [secretName, setSecretName] = useState("");
  const [secretValue, setSecretValue] = useState("");
  const [secretFingerprint, setSecretFingerprint] = useState<string | null>(null);
  const [providerCapability, setProviderCapability] = useState<"image_gen" | "text_gen" | "video_gen">("text_gen");
  const [providerKind, setProviderKind] = useState<
    "openai_images" | "openai_images_edit" | "openai_compat_chat" | "cloud_video"
  >("openai_compat_chat");
  const [providerBaseUrl, setProviderBaseUrl] = useState("");
  const [providerModel, setProviderModel] = useState("");
  const [providerSecretRef, setProviderSecretRef] = useState("");
  const [providerAuthStyle, setProviderAuthStyle] = useState<"bearer" | "header" | "query">("bearer");
  const [providerAuthParam, setProviderAuthParam] = useState("");
  const [providerEnabled, setProviderEnabled] = useState(true);
  const [providerBusy, setProviderBusy] = useState(false);
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
  const [view, setView] = useState<"workbench" | "copy" | "video" | "export" | "console">("workbench");
  const [cockpitOpen, setCockpitOpen] = useState(false);
  const [cockpitRefreshing, setCockpitRefreshing] = useState(false);
  const [relistProducts, setRelistProducts] = useState<Product[]>([]);
  const [relistSelected, setRelistSelected] = useState<Record<string, boolean>>({});
  const [relistResults, setRelistResults] = useState<RelistItem[] | null>(() => loadStoredAcceptance().results);
  const [relistAdopt, setRelistAdopt] = useState<Record<string, boolean>>(() => loadStoredAcceptance().adopt);
  // Selected candidate index per relist item key (product_id), default 0.
  const [relistChoice, setRelistChoice] = useState<Record<string, number>>(() => loadStoredAcceptance().choice);
  // Module 1 intake (read-only): xlsx extract + drag-drop image import.
  const [relistImportBusy, setRelistImportBusy] = useState(false);
  const [relistImportError, setRelistImportError] = useState<string | null>(null);
  const [relistDragOver, setRelistDragOver] = useState(false);
  const [relistBusy, setRelistBusy] = useState(false);
  const [relistPushing, setRelistPushing] = useState(false);
  const [relistPushResults, setRelistPushResults] = useState<RelistPushResult[] | null>(
    () => loadStoredAcceptance().pushResults
  );
  const [module3Products, setModule3Products] = useState<Product[]>([]);
  const [module3Selected, setModule3Selected] = useState<Record<string, boolean>>({});
  const [module3Results, setModule3Results] = useState<Module3Item[] | null>(null);
  const [module3Busy, setModule3Busy] = useState(false);
  // Per-product reviewed/edited proposal (keyed by product_id), the adopt set,
  // the dry-run preview, and the executed-write results.
  const [module3Adopt, setModule3Adopt] = useState<Record<string, boolean>>({});
  const [module3Edited, setModule3Edited] = useState<Record<string, Module3Fields>>({});
  const [module3Previews, setModule3Previews] = useState<Module3PushItem[] | null>(null);
  const [module3Pushing, setModule3Pushing] = useState(false);
  const [module3PushResults, setModule3PushResults] = useState<Module3PushItem[] | null>(null);
  // Module 6 (cloud image-to-video) — async generation, no Ozon push in v1.
  const [videoFirstFrame, setVideoFirstFrame] = useState("");
  const [videoLastFrame, setVideoLastFrame] = useState("");
  const [videoPrompt, setVideoPrompt] = useState("");
  const [videoDuration, setVideoDuration] = useState(8);
  const [videoBusy, setVideoBusy] = useState(false);
  const [videoJob, setVideoJob] = useState<VideoJob | null>(null);
  const videoPollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  // Module 4 (export / delivery) — local only, no Ozon push.
  const [exportTemplatePath, setExportTemplatePath] = useState("");
  const [exportConfigPath, setExportConfigPath] = useState("");
  const [exportBusy, setExportBusy] = useState(false);
  const [exportResult, setExportResult] = useState<RelistExportResponse | null>(null);
  const [exportError, setExportError] = useState<string | null>(null);
  const realModeRequiresLease = configStatus?.real_ozon_enabled ?? true;
  const ozonConfigReady = configStatus?.ozon.configured === true;
  const openAiConfigReady = configStatus?.openai.configured === true;
  const leaseReady = !realModeRequiresLease || configStatus?.lease.valid === true;
  const canUseOzonReadTools = Boolean(configStatus && ozonConfigReady && leaseReady);
  const canGeneratePosterBackground = canUseOzonReadTools && openAiConfigReady && !imageGenerationIssue;
  const localServiceReady = Boolean(health && isConnectedRuntime(runtime));
  const openClawBridgeReady = localServiceReady && Boolean(manifest?.tools?.length);
  const relistSelectedCount = relistProducts.filter((product) => relistSelected[product.product_id]).length;
  const module3SelectedCount = module3Products.filter((product) => module3Selected[product.product_id]).length;
  const relistAdoptCount = relistResults
    ? relistResults.filter(
        (item) =>
          selectedCandidateUrl(item, relistChoice[item.product_id]) &&
          !item.error &&
          relistAdopt[item.product_id]
      ).length
    : 0;
  const module3AdoptCount = module3Results
    ? module3Results.filter((item) => item.proposal && !item.error && module3Adopt[item.product_id]).length
    : 0;
  // The live-write button stays disabled until a dry-run preview has been run
  // for the currently-adopted set (no one-click unconfirmed writes).
  const module3PreviewReady = Boolean(module3Previews && module3Previews.some((item) => item.preview));
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

  const cockpitNodes = useMemo(
    () =>
      buildCockpitNodes(c, {
        nodeUp: localServiceReady,
        bridge: openClawBridgeReady,
        lease: leaseReady,
        secretBackend: configStatus?.secret_store.backend ?? null,
        secretAvailable: configStatus?.secret_store.available ?? false,
        ozon: ozonConfigReady,
        openai: openAiConfigReady,
        connector: configStatus?.connector_mode ?? runtime.connector_mode,
        posterReady: Boolean(
          configStatus?.poster_generation?.openclaw_bridge_ready ||
            configStatus?.poster_generation?.api_fallback_configured
        ),
        imageRouting: Boolean(
          configStatus?.capabilities?.find((x) => x.capability === "image_gen")?.ready
        )
      }),
    [
      c,
      localServiceReady,
      openClawBridgeReady,
      leaseReady,
      configStatus,
      ozonConfigReady,
      openAiConfigReady,
      runtime.connector_mode
    ]
  );

  const cockpitSummary = useMemo<CockpitSummary>(() => {
    const counts = { live: 0, partial: 0, isolated: 0, missing: 0 };
    let wiredOk = 0;
    let wiredTotal = 0;
    for (const node of cockpitNodes) {
      counts[node.status] += 1;
      for (const chip of node.chips) {
        wiredTotal += 1;
        if (chip.state === "ok") {
          wiredOk += 1;
        } else if (chip.state === "wip") {
          wiredOk += 0.5;
        }
      }
    }
    return {
      counts,
      live: counts.live,
      total: cockpitNodes.length,
      wiredPct: wiredTotal ? Math.round((wiredOk / wiredTotal) * 100) : 0,
      buildCommit: health?.build_commit ?? null,
      version: health?.package_version ?? null,
      connector: configStatus?.connector_mode ?? runtime.connector_mode,
      lease: leaseReady,
      nodeUp: localServiceReady,
      checkedAt: configStatus?.checked_at ? new Date(configStatus.checked_at).toLocaleTimeString() : null
    };
  }, [cockpitNodes, health, configStatus, runtime.connector_mode, leaseReady, localServiceReady]);

  const cockpitLive = useMemo<Record<number, { label: string; value: string }[]>>(() => {
    const labels = c.cockpit.metrics;
    const dash = "—";
    const leaseExpiry = configStatus?.lease.expires_at
      ? new Date(configStatus.lease.expires_at).toLocaleDateString()
      : dash;
    return {
      0: [
        { label: labels.observing, value: "7" },
        { label: labels.tools, value: String(manifest?.tools?.length ?? 0) }
      ],
      1: [
        { label: labels.products, value: productCount != null ? String(productCount) : dash },
        { label: labels.connector, value: configStatus?.connector_mode ?? runtime.connector_mode }
      ],
      2: [
        { label: labels.pending, value: String(queueStats.pending) },
        { label: labels.queued, value: String(queueStats.queued) },
        { label: labels.adopted, value: String(relistAdoptCount) }
      ],
      3: [{ label: labels.model, value: openAiConfigReady ? openAiImageModel : dash }],
      4: [{ label: labels.runtime, value: labels.none }],
      5: [
        { label: labels.features, value: String(health?.features?.length ?? 0) },
        { label: labels.leaseExp, value: leaseExpiry },
        { label: labels.secret, value: configStatus?.secret_store.backend ?? dash }
      ],
      6: [{ label: labels.runtime, value: labels.none }]
    };
  }, [
    c,
    manifest,
    productCount,
    configStatus,
    runtime.connector_mode,
    queueStats,
    relistAdoptCount,
    openAiConfigReady,
    openAiImageModel,
    health
  ]);

  async function refreshCockpit() {
    if (cockpitRefreshing) {
      return;
    }
    setCockpitRefreshing(true);
    try {
      await Promise.all([checkHealth(), refresh()]);
    } finally {
      setCockpitRefreshing(false);
    }
  }

  useEffect(() => {
    window.localStorage.setItem("ozon-local-locale", locale);
    document.documentElement.lang = locale;
  }, [locale]);

  // Persist module 2 acceptance state (results + adopted + push outcomes) locally
  // so the review survives a refresh/restart. Best-effort; never blocks the UI.
  useEffect(() => {
    try {
      window.localStorage.setItem(
        ACCEPTANCE_STORAGE_KEY,
        JSON.stringify({
          results: relistResults,
          adopt: relistAdopt,
          pushResults: relistPushResults,
          choice: relistChoice
        })
      );
    } catch {
      // localStorage may be unavailable or full; persistence is non-critical.
    }
  }, [relistResults, relistAdopt, relistPushResults, relistChoice]);

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
      setSaveNotice({ tone: "warn", text: c.messages.fillOzonCredentials });
      return;
    }
    setSaveNotice({ tone: "info", text: c.config.saving });
    try {
      const response = await api("/config/ozon", {
        method: "POST",
        body: JSON.stringify({ client_id: clientId.trim(), api_key: apiKey.trim() })
      });
      const data = await response.json();
      if (!response.ok) {
        setMessage(c.messages.saveFailed(userFacingError(data.error)));
        setSaveNotice({ tone: "warn", text: c.messages.saveFailed(userFacingError(data.error)) });
        return;
      }
      setApiKey("");
      const refreshedConfig = await checkHealth();

      const validationResponse = await api("/config/ozon/validate", { method: "POST" });
      const validationData = await validationResponse.json();
      if (!validationResponse.ok) {
        setValidation(null);
        setMessage(c.messages.ozonSavedValidationFailed(userFacingError(validationData.error)));
        setSaveNotice({
          tone: "warn",
          text: c.messages.ozonSavedValidationFailed(userFacingError(validationData.error))
        });
        return;
      }
      setValidation(validationData);
      const fingerprint = refreshedConfig?.ozon.api_key_fingerprint ?? c.advanced.notSaved;
      setMessage(c.messages.ozonSavedValidated(data.client_id, fingerprint));
      setSaveNotice({ tone: "ok", text: c.messages.ozonSavedValidated(data.client_id, fingerprint) });
    } catch {
      setMessage(c.messages.saveFailed(c.messages.localServiceUnreachable));
      setSaveNotice({ tone: "warn", text: c.messages.saveFailed(c.messages.localServiceUnreachable) });
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

  // Provider config — Sub-form A: save a named secret. The key field is an empty
  // human input; the response returns ONLY a fingerprint, never the key.
  async function saveSecret() {
    const name = secretName.trim();
    if (!name) {
      setMessage(c.providers.msgNameRequired);
      return;
    }
    if (!/^[a-z0-9_]+$/.test(name)) {
      setMessage(c.providers.msgNamePolicy);
      return;
    }
    if (!secretValue) {
      setMessage(c.providers.msgValueRequired);
      return;
    }
    setProviderBusy(true);
    try {
      const response = await api("/config/secret", {
        method: "POST",
        body: JSON.stringify({ name, value: secretValue })
      });
      const data = await response.json();
      if (!response.ok) {
        setSecretFingerprint(null);
        setMessage(c.providers.msgSecretFailed(userFacingError(data.error)));
        return;
      }
      setSecretValue(""); // never retain the raw key in component state
      setSecretFingerprint(data.fingerprint as string);
      setMessage(c.providers.msgSecretSaved(data.name, data.fingerprint));
      await checkHealth();
    } catch {
      setMessage(c.providers.msgSecretFailed(c.messages.localServiceUnreachable));
    } finally {
      setProviderBusy(false);
    }
  }

  // Provider config — Sub-form B: add/replace a provider entry. Fetches the
  // current registry (GET), merges this entry into its capability vec (add or
  // replace by base_url+model), and POSTs the full merged registry back.
  async function saveProvider() {
    const baseUrl = providerBaseUrl.trim();
    const model = providerModel.trim();
    const ref = providerSecretRef.trim();
    if (!baseUrl || !model || !ref) {
      setMessage(c.providers.msgProviderFieldsRequired);
      return;
    }
    let auth: { bearer?: Record<string, never>; header?: { name: string }; query?: { name: string } } | string;
    if (providerAuthStyle === "bearer") {
      auth = "bearer";
    } else {
      const paramName = providerAuthParam.trim();
      if (!paramName) {
        setMessage(c.providers.msgAuthParamRequired);
        return;
      }
      auth =
        providerAuthStyle === "header"
          ? { header: { name: paramName } }
          : { query: { name: paramName } };
    }
    const entry = {
      kind: providerKind,
      base_url: baseUrl,
      model,
      secret_ref: ref,
      auth,
      enabled: providerEnabled
    };
    setProviderBusy(true);
    try {
      const getResponse = await api("/config/registry", { method: "GET" });
      const current = await getResponse.json();
      if (!getResponse.ok) {
        setMessage(c.providers.msgProviderFailed(userFacingError(current.error)));
        return;
      }
      const registry = {
        image_gen: Array.isArray(current.image_gen) ? current.image_gen : [],
        text_gen: Array.isArray(current.text_gen) ? current.text_gen : [],
        video_gen: Array.isArray(current.video_gen) ? current.video_gen : []
      } as Record<string, Record<string, unknown>[]>;
      const list = registry[providerCapability];
      const idx = list.findIndex((e) => e.base_url === baseUrl && e.model === model);
      if (idx >= 0) list[idx] = entry as unknown as Record<string, unknown>;
      else list.push(entry as unknown as Record<string, unknown>);

      const postResponse = await api("/config/registry", {
        method: "POST",
        body: JSON.stringify(registry)
      });
      const saved = await postResponse.json();
      if (!postResponse.ok) {
        setMessage(c.providers.msgProviderFailed(userFacingError(saved.error)));
        return;
      }
      setMessage(c.providers.msgProviderSaved(providerCapability));
      await checkHealth();
    } catch {
      setMessage(c.providers.msgProviderFailed(c.messages.localServiceUnreachable));
    } finally {
      setProviderBusy(false);
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

  async function loadRelistProducts() {
    if (!ensureOzonReadAccess(c.relist.loadProducts)) return;
    try {
      const response = await api("/tools/ozon.products.list", {
        method: "POST",
        body: JSON.stringify({ limit: 50, visibility: "VISIBLE" })
      });
      const data = await response.json();
      if (!response.ok) {
        setMessage(c.relist.msgLoadFailed(userFacingError(data.error)));
        return;
      }
      const list = data as ProductListResult;
      setRelistProducts(list.products);
      setRelistSelected({});
      setRelistResults(null);
      setRelistPushResults(null);
      setMessage(c.relist.msgLoaded(list.products.length));
    } catch {
      setMessage(c.relist.msgLoadFailed(c.messages.localServiceUnreachable));
    }
  }

  function toggleRelistSelect(productId: string) {
    setRelistSelected((prev) => ({ ...prev, [productId]: !prev[productId] }));
  }

  function toggleRelistAdopt(productId: string) {
    setRelistAdopt((prev) => ({ ...prev, [productId]: !prev[productId] }));
  }

  async function generateRelist() {
    const targets = relistProducts
      .filter((product) => relistSelected[product.product_id])
      .map((product) => ({ product_id: product.product_id, offer_id: product.offer_id }));
    if (targets.length === 0) {
      setMessage(c.relist.msgSelectFirst);
      return;
    }
    setRelistBusy(true);
    setRelistResults(null);
    setRelistPushResults(null);
    setMessage(c.relist.msgGenerating(targets.length));
    try {
      const response = await api("/tools/ozon.relist.generate", {
        method: "POST",
        body: JSON.stringify({ targets })
      });
      const data = await response.json();
      if (!response.ok) {
        setMessage(c.relist.msgGenFailed(userFacingError(data.error)));
        return;
      }
      const items = ((data.items ?? []) as RelistItem[]).map(migrateRelistItem);
      setRelistResults(items);
      const adopt: Record<string, boolean> = {};
      const choice: Record<string, number> = {};
      items.forEach((item) => {
        if (item.candidates.length > 0 && !item.error) {
          adopt[item.product_id] = true;
          choice[item.product_id] = 0;
        }
      });
      setRelistAdopt(adopt);
      setRelistChoice(choice);
      const ok = items.filter((item) => item.candidates.length > 0 && !item.error).length;
      setMessage(c.relist.msgGenerated(ok, items.length));
    } catch {
      setMessage(c.relist.msgGenFailed(c.messages.localServiceUnreachable));
    } finally {
      setRelistBusy(false);
    }
  }

  // Move the selected candidate index for an item by +/-1, wrapping within range.
  function stepRelistChoice(item: RelistItem, delta: number) {
    const n = item.candidates.length;
    if (n <= 1) return;
    setRelistChoice((prev) => {
      const current = prev[item.product_id] ?? 0;
      const next = ((current + delta) % n + n) % n;
      return { ...prev, [item.product_id]: next };
    });
  }

  // Module 1 intake: read a supplier .xlsx (read-only) and MERGE its rows into
  // the relist product list. Imported rows get a synthetic "xlsx:"+sku product_id
  // (they lack an Ozon product_id) and use sku as the offer_id.
  async function importXlsxFile(file: File) {
    if (!ensureOzonReadAccess(c.relist.intakeImportXlsx)) return;
    setRelistImportBusy(true);
    setRelistImportError(null);
    try {
      // The sidecar reads the workbook from disk; we forward the OS path the
      // browser exposes when available, else the bare name (operator picks the
      // file the engine can resolve in its working tree).
      const path = (file as File & { path?: string }).path ?? file.name;
      const response = await api("/tools/ozon.relist.extract", {
        method: "POST",
        body: JSON.stringify({ template_path: path })
      });
      const data = await response.json();
      if (!response.ok) {
        setRelistImportError(userFacingError(data.error));
        return;
      }
      const rows = (data.rows ?? []) as {
        sheet: string;
        row: number;
        sku: string | null;
        title: string | null;
        listing: string | null;
        images_main: string[];
        images_additional: string[];
      }[];
      const imported: Product[] = rows
        .filter((row) => row.sku || row.title)
        .map((row) => {
          const sku = (row.sku ?? `${row.sheet}-${row.row}`).trim();
          return {
            product_id: `xlsx:${sku}`,
            offer_id: sku,
            name: row.title,
            visibility: null,
            archived: null,
            has_fbo_stocks: null,
            has_fbs_stocks: null
          };
        });
      // MERGE: replace any prior import of the same synthetic id, keep the rest.
      setRelistProducts((prev) => {
        const byId = new Map(prev.map((p) => [p.product_id, p]));
        imported.forEach((p) => byId.set(p.product_id, p));
        return Array.from(byId.values());
      });
      setMessage(c.relist.intakeImportedRows(imported.length));
    } catch {
      setRelistImportError(c.messages.localServiceUnreachable);
    } finally {
      setRelistImportBusy(false);
    }
  }

  // Module 1 intake: host an operator-dropped PNG and attach it as a candidate on
  // the matching relist item (matched by product_id == "xlsx:"+<basename>, else
  // applied to the most-recently-imported row). PNG only (server enforces).
  async function importDroppedImage(file: File) {
    if (!ensureOzonReadAccess(c.relist.intakeDropImage)) return;
    setRelistImportBusy(true);
    setRelistImportError(null);
    try {
      const dataBase64 = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(String(reader.result ?? ""));
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(file);
      });
      const response = await api("/tools/ozon.relist.import-image", {
        method: "POST",
        body: JSON.stringify({ filename: file.name, data_base64: dataBase64 })
      });
      const data = await response.json();
      if (!response.ok) {
        setRelistImportError(userFacingError(data.error));
        return;
      }
      const newUrl = data.new_url as string;
      // Match by file stem to an imported sku ("name.png" -> sku "name"); else
      // attach to the first imported row that has no candidate yet.
      const stem = file.name.replace(/\.[^.]+$/, "").trim();
      setRelistResults((prev) => {
        const existing = prev ?? [];
        // Ensure an imported product carrying this image exists as a relist item.
        const matchIndex = existing.findIndex(
          (item) => item.imported && (item.offer_id === stem || item.product_id === `xlsx:${stem}`)
        );
        if (matchIndex >= 0) {
          const next = existing.slice();
          const item = next[matchIndex];
          next[matchIndex] = { ...item, candidates: [...item.candidates, newUrl], error: null };
          return next;
        }
        // No matching imported item yet: create a fresh imported relist row.
        const productId = `xlsx:${stem || "image"}`;
        const newItem: RelistItem = {
          product_id: productId,
          offer_id: stem || file.name,
          name: null,
          original_url: null,
          candidates: [newUrl],
          error: null,
          imported: true
        };
        return [...existing, newItem];
      });
      setRelistAdopt((prev) => ({ ...prev, [`xlsx:${stem || "image"}`]: true }));
      setMessage(c.relist.intakeImportedImage);
    } catch {
      setRelistImportError(c.messages.localServiceUnreachable);
    } finally {
      setRelistImportBusy(false);
    }
  }

  async function loadModule3Products() {
    if (!ensureOzonReadAccess(c.module3.loadProducts)) return;
    try {
      const response = await api("/tools/ozon.products.list", {
        method: "POST",
        body: JSON.stringify({ limit: 50, visibility: "VISIBLE" })
      });
      const data = await response.json();
      if (!response.ok) {
        setMessage(c.module3.msgLoadFailed(userFacingError(data.error)));
        return;
      }
      const list = data as ProductListResult;
      setModule3Products(list.products);
      setModule3Selected({});
      setModule3Results(null);
      setMessage(c.module3.msgLoaded(list.products.length));
    } catch {
      setMessage(c.module3.msgLoadFailed(c.messages.localServiceUnreachable));
    }
  }

  function toggleModule3Select(productId: string) {
    setModule3Selected((prev) => ({ ...prev, [productId]: !prev[productId] }));
  }

  async function module3Recognize() {
    if (!ensureOzonReadAccess(c.module3.recognize(0))) return;
    const targets = module3Products
      .filter((product) => module3Selected[product.product_id])
      .map((product) => ({ product_id: product.product_id, offer_id: product.offer_id }));
    if (targets.length === 0) {
      setMessage(c.module3.msgSelectFirst);
      return;
    }
    setModule3Busy(true);
    setModule3Results(null);
    setMessage(c.module3.msgRecognizing(targets.length));
    try {
      const response = await api("/tools/ozon.module3.recognize", {
        method: "POST",
        body: JSON.stringify({ targets, target_language: "ru" })
      });
      const data = await response.json();
      if (!response.ok) {
        setMessage(c.module3.msgRecognizeFailed(userFacingError(data.error)));
        return;
      }
      const items = (data.items ?? []) as Module3Item[];
      setModule3Results(items);
      const adopt: Record<string, boolean> = {};
      const edited: Record<string, Module3Fields> = {};
      for (const item of items) {
        if (item.proposal && !item.error) {
          adopt[item.product_id] = true;
          // Seed the editable buffer from the proposal so the operator edits a copy.
          edited[item.product_id] = {
            title: item.proposal.title,
            description: item.proposal.description,
            attributes: item.proposal.attributes.map((attr) => ({
              name: attr.name,
              values: [...attr.values]
            })),
            type_category: item.proposal.type_category
          };
        }
      }
      setModule3Adopt(adopt);
      setModule3Edited(edited);
      setModule3Previews(null);
      setModule3PushResults(null);
      const ok = items.filter((item) => item.proposal && !item.error).length;
      setMessage(c.module3.msgRecognized(ok, items.length));
    } catch {
      setMessage(c.module3.msgRecognizeFailed(c.messages.localServiceUnreachable));
    } finally {
      setModule3Busy(false);
    }
  }

  function toggleModule3Adopt(productId: string) {
    setModule3Adopt((prev) => ({ ...prev, [productId]: !prev[productId] }));
    // Editing the adopt set invalidates any prior dry-run preview.
    setModule3Previews(null);
    setModule3PushResults(null);
  }

  function editModule3Field(productId: string, field: "title" | "description", value: string) {
    setModule3Edited((prev) => {
      const current = prev[productId];
      if (!current) return prev;
      return { ...prev, [productId]: { ...current, [field]: value } };
    });
    setModule3Previews(null);
    setModule3PushResults(null);
  }

  function module3AdoptedTargets() {
    if (!module3Results) return [];
    return module3Results
      .filter((item) => item.proposal && !item.error && module3Adopt[item.product_id])
      .map((item) => ({
        product_id: item.product_id,
        proposal: module3Edited[item.product_id] ?? (item.proposal as Module3Fields)
      }));
  }

  async function previewModule3Push() {
    const items = module3AdoptedTargets();
    if (items.length === 0) {
      setMessage(c.module3.msgAdoptFirst);
      return;
    }
    setModule3Pushing(true);
    setModule3PushResults(null);
    setMessage(c.module3.msgPreviewing(items.length));
    try {
      // confirm omitted -> server returns dry-run previews and writes NOTHING.
      const response = await api("/tools/ozon.module3.push", {
        method: "POST",
        body: JSON.stringify({ items })
      });
      const data = await response.json();
      if (!response.ok) {
        setMessage(c.module3.msgPushFailed(userFacingError(data.error)));
        return;
      }
      const previews = (data.items ?? []) as Module3PushItem[];
      setModule3Previews(previews);
      const ok = previews.filter((item) => item.preview && !item.error).length;
      setMessage(c.module3.msgPreviewed(ok, previews.length));
    } catch {
      setMessage(c.module3.msgPushFailed(c.messages.localServiceUnreachable));
    } finally {
      setModule3Pushing(false);
    }
  }

  async function pushModule3() {
    const items = module3AdoptedTargets();
    if (items.length === 0) {
      setMessage(c.module3.msgAdoptFirst);
      return;
    }
    if (!module3PreviewReady) {
      setMessage(c.module3.msgPreviewFirst);
      return;
    }
    if (!window.confirm(c.module3.pushConfirm)) {
      return;
    }
    setModule3Pushing(true);
    setMessage(c.module3.msgPushing(items.length));
    try {
      // confirm=true -> server executes the live write after our explicit gate.
      const response = await api("/tools/ozon.module3.push", {
        method: "POST",
        body: JSON.stringify({ items, confirm: true })
      });
      const data = await response.json();
      if (!response.ok) {
        setMessage(c.module3.msgPushFailed(userFacingError(data.error)));
        return;
      }
      const results = (data.items ?? []) as Module3PushItem[];
      setModule3PushResults(results);
      const ok = results.filter((item) => item.written && !item.error).length;
      setMessage(c.module3.msgPushed(ok, results.length));
    } catch {
      setMessage(c.module3.msgPushFailed(c.messages.localServiceUnreachable));
    } finally {
      setModule3Pushing(false);
    }
  }

  // Module 6 — candidate frame URLs: prefer module-2 adopted new images, then
  // the product's primary/gallery images. Each option carries a human label.
  const videoFrameOptions = useMemo(() => {
    const options: { url: string; label: string }[] = [];
    const seen = new Set<string>();
    const push = (url: string | null | undefined, label: string) => {
      const value = (url ?? "").trim();
      if (!value || seen.has(value)) return;
      seen.add(value);
      options.push({ url: value, label });
    };
    (relistResults ?? []).forEach((item) => {
      const selected = selectedCandidateUrl(item, relistChoice[item.product_id]);
      if (selected && !item.error) {
        push(selected, c.video.frameNewImage(item.offer_id || item.product_id));
      }
      // The module-2 original primary is also a valid first frame.
      push(item.original_url, c.video.framePrimary(item.offer_id || item.product_id));
    });
    if (productDetail?.product) {
      push(productDetail.product.primary_image, c.video.framePrimary(productDetail.product.offer_id || productDetail.product.product_id));
      productDetail.product.gallery_images.forEach((url, index) =>
        push(url, c.video.frameGallery(index + 1))
      );
    }
    return options;
  }, [relistResults, relistChoice, productDetail, c]);

  // Stop any in-flight poll loop (component teardown, new job, or terminal job).
  function stopVideoPoll() {
    if (videoPollRef.current !== null) {
      clearInterval(videoPollRef.current);
      videoPollRef.current = null;
    }
  }

  useEffect(() => stopVideoPoll, []);

  async function createVideo() {
    if (!ensureOzonReadAccess(c.video.generate)) return;
    const firstFrame = videoFirstFrame.trim();
    if (!firstFrame) {
      setMessage(c.video.msgFirstFrameRequired);
      return;
    }
    stopVideoPoll();
    setVideoBusy(true);
    setVideoJob(null);
    setMessage(c.video.msgCreating);
    try {
      const response = await api("/tools/ozon.video.create", {
        method: "POST",
        body: JSON.stringify({
          first_frame_url: firstFrame,
          last_frame_url: videoLastFrame.trim() || undefined,
          prompt: videoPrompt.trim() || undefined,
          duration_seconds: videoDuration
        })
      });
      const data = await response.json();
      if (!response.ok) {
        // NotConfigured -> a clear bad_request; hint the operator to configure
        // a video provider in the model registry.
        setMessage(c.video.msgCreateFailed(userFacingError(data.error)));
        return;
      }
      const job = data as VideoJob;
      setVideoJob(job);
      setMessage(c.video.msgCreated(job.id));
      pollVideo(job.id);
    } catch {
      setMessage(c.video.msgCreateFailed(c.messages.localServiceUnreachable));
    } finally {
      setVideoBusy(false);
    }
  }

  // Poll the bounded server-side job until it reaches a terminal status. The
  // server poller is the real deadline; this UI interval just mirrors status.
  function pollVideo(jobId: string) {
    stopVideoPoll();
    videoPollRef.current = setInterval(async () => {
      try {
        const response = await api(`/tools/ozon.video.get/${jobId}`, { method: "GET" });
        if (!response.ok) return;
        const job = (await response.json()) as VideoJob;
        setVideoJob(job);
        if (job.status === "succeeded" || job.status === "failed") {
          stopVideoPoll();
          setMessage(
            job.status === "succeeded" ? c.video.msgSucceeded : c.video.msgFailed(job.error ?? "")
          );
        }
      } catch {
        // Transient fetch error; keep polling until the server job terminates.
      }
    }, 5000);
  }

  // Module 4 — assemble the export rows from the CURRENT accepted state:
  //   * module-2 adopted image (new_url) -> primary_image_url
  //   * module-3 redistributed title/description -> title / listing
  // joined by product_id, with fallbacks to the source title/description and the
  // product's original primary image when a piece is missing. The union of all
  // product_ids seen across module-2 and module-3 forms the row set.
  function assembleExportRows(): ExportRow[] {
    const ids = new Set<string>();
    (relistResults ?? []).forEach((item) => item.product_id && ids.add(item.product_id));
    (module3Results ?? []).forEach((item) => item.product_id && ids.add(item.product_id));

    const relistById = new Map((relistResults ?? []).map((item) => [item.product_id, item]));
    const m3ById = new Map((module3Results ?? []).map((item) => [item.product_id, item]));
    const productById = new Map(relistProducts.map((product) => [product.product_id, product]));

    const rows: ExportRow[] = [];
    for (const productId of ids) {
      const relist = relistById.get(productId);
      const m3 = m3ById.get(productId);
      const product = productById.get(productId);

      // Title/listing: prefer the adopted module-3 proposal (edited if any),
      // else the module-3 source, else the product/relist name.
      const m3Fields =
        m3 && m3.proposal && !m3.error && module3Adopt[productId]
          ? module3Edited[productId] ?? m3.proposal
          : null;
      const title =
        (m3Fields?.title || m3?.source.title || relist?.name || product?.name || "").trim();
      const listing = (m3Fields?.description || m3?.source.description || "").trim();

      // Primary image: prefer the adopted module-2 selected candidate, else the
      // product's original primary image.
      const selectedImage = relist
        ? selectedCandidateUrl(relist, relistChoice[productId])
        : null;
      const adoptedImage =
        relist && selectedImage && !relist.error && relistAdopt[productId] ? selectedImage : null;
      const primaryImageUrl = adoptedImage || relist?.original_url || null;

      rows.push({
        product_id: productId,
        offer_id: relist?.offer_id || m3?.offer_id || product?.offer_id || "",
        title,
        listing,
        primary_image_url: primaryImageUrl,
        additional_image_urls: []
      });
    }
    return rows;
  }

  async function runExport() {
    if (!ensureOzonReadAccess(c.export.generate)) return;
    const template = exportTemplatePath.trim();
    if (!template) {
      setMessage(c.export.msgTemplateRequired);
      return;
    }
    const rows = assembleExportRows();
    if (rows.length === 0) {
      setMessage(c.export.msgNoRows);
      return;
    }
    setExportBusy(true);
    setExportResult(null);
    setExportError(null);
    setMessage(c.export.msgExporting(rows.length));
    try {
      const config = exportConfigPath.trim();
      const response = await api("/tools/ozon.relist.export", {
        method: "POST",
        body: JSON.stringify({
          template_path: template,
          config_path: config || undefined,
          rows: rows.map((row) => ({
            title: row.title,
            listing: row.listing,
            primary_image_url: row.primary_image_url,
            additional_image_urls: row.additional_image_urls
          }))
        })
      });
      const data = await response.json();
      if (!response.ok) {
        // A verify failure (frozen-cell change) comes back as a hard error — the
        // deliverable is contaminated and the server never returns it.
        setExportError(userFacingError(data.error));
        setMessage(c.export.msgFailed(userFacingError(data.error)));
        return;
      }
      const result = data as RelistExportResponse;
      setExportResult(result);
      setMessage(c.export.msgDone(result.out_path));
    } catch {
      setExportError(c.messages.localServiceUnreachable);
      setMessage(c.export.msgFailed(c.messages.localServiceUnreachable));
    } finally {
      setExportBusy(false);
    }
  }

  async function pushRelist() {
    if (!relistResults) return;
    // Push-by-product_id only applies to API-sourced rows. Imported (.xlsx) rows
    // have a synthetic product_id and are excluded — they are export-only.
    const items = relistResults
      .filter((item) => {
        if (item.imported || item.error) return false;
        if (!relistAdopt[item.product_id]) return false;
        return Boolean(selectedCandidateUrl(item, relistChoice[item.product_id]));
      })
      .map((item) => ({
        product_id: item.product_id,
        new_primary_url: selectedCandidateUrl(item, relistChoice[item.product_id]) as string
      }));
    if (items.length === 0) {
      setMessage(c.relist.msgAdoptFirst);
      return;
    }
    setRelistPushing(true);
    setMessage(c.relist.msgPushing(items.length));
    try {
      const response = await api("/tools/ozon.relist.push", {
        method: "POST",
        body: JSON.stringify({ items })
      });
      const data = await response.json();
      if (!response.ok) {
        setMessage(c.relist.msgPushFailed(userFacingError(data.error)));
        return;
      }
      const results = (data.items ?? []) as RelistPushResult[];
      setRelistPushResults(results);
      const ok = results.filter((result) => result.ok).length;
      setMessage(c.relist.msgPushed(ok, results.length));
    } catch {
      setMessage(c.relist.msgPushFailed(c.messages.localServiceUnreachable));
    } finally {
      setRelistPushing(false);
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
        <button className="nav-cockpit" onClick={() => setCockpitOpen(true)} title={c.cockpit.title}>
          <Boxes size={15} /> {c.cockpit.tab}
        </button>
      </nav>

      <div className="view-tabs">
        <button className={view === "workbench" ? "active" : ""} onClick={() => setView("workbench")}>
          <Sparkles size={16} /> {c.relist.tab}
        </button>
        <button className={view === "copy" ? "active" : ""} onClick={() => setView("copy")}>
          <FileText size={16} /> {c.module3.tab}
        </button>
        <button className={view === "video" ? "active" : ""} onClick={() => setView("video")}>
          <Clapperboard size={16} /> {c.video.tab}
        </button>
        <button className={view === "export" ? "active" : ""} onClick={() => setView("export")}>
          <Download size={16} /> {c.export.tab}
        </button>
        <button className={view === "console" ? "active" : ""} onClick={() => setView("console")}>
          <SlidersHorizontal size={16} /> {c.relist.tabConsole}
        </button>
      </div>

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

      {view === "workbench" && (
        <section className="workspace-grid relist-grid" id="relist-workbench">
          <div className="panel relist-pick">
            <div className="section-title">
              <Sparkles />
              <div>
                <h2>{c.relist.title}</h2>
                <p>{c.relist.description}</p>
              </div>
            </div>
            {!canUseOzonReadTools && <p className="notice">{ozonReadGateMessage || c.gates.connectLocalFirst}</p>}
            {canUseOzonReadTools && !openAiConfigReady && (
              <p className="notice">{c.gates.useOpenClawWithoutImageApi}</p>
            )}
            <div className="relist-toolbar">
              <button onClick={loadRelistProducts} disabled={!canUseOzonReadTools}>
                <DatabaseZap size={16} /> {c.relist.loadProducts}
              </button>
              <button
                className="secondary-button"
                onClick={generateRelist}
                disabled={relistBusy || !canGeneratePosterBackground || relistSelectedCount === 0}
              >
                <Sparkles size={16} /> {relistBusy ? c.relist.generating : c.relist.generate(relistSelectedCount)}
              </button>
            </div>

            <div className="relist-intake">
              <div className="section-subtitle">{c.relist.intakeTitle}</div>
              <p className="hint">{c.relist.intakeDescription}</p>
              <label className="file-button">
                <input
                  type="file"
                  accept=".xlsx,.xlsm"
                  style={{ display: "none" }}
                  disabled={!canUseOzonReadTools || relistImportBusy}
                  onChange={(event) => {
                    const file = event.target.files?.[0];
                    if (file) void importXlsxFile(file);
                    event.target.value = "";
                  }}
                />
                {relistImportBusy ? c.relist.intakeBusy : c.relist.intakeImportXlsx}
              </label>
              <div
                className={`drop-zone ${relistDragOver ? "drag-over" : ""}`}
                onDragOver={(event) => {
                  event.preventDefault();
                  setRelistDragOver(true);
                }}
                onDragLeave={() => setRelistDragOver(false)}
                onDrop={(event) => {
                  event.preventDefault();
                  setRelistDragOver(false);
                  const file = event.dataTransfer.files?.[0];
                  if (file) void importDroppedImage(file);
                }}
              >
                {c.relist.intakeDropImage}
              </div>
              {relistImportError && <p className="notice error">{relistImportError}</p>}
            </div>
            <div className="product-list relist-list">
              {relistProducts.length === 0 && <p className="empty">{c.relist.emptyProducts}</p>}
              {relistProducts.map((product) => (
                <label
                  key={product.product_id}
                  className={`relist-row ${relistSelected[product.product_id] ? "picked" : ""}`}
                >
                  <input
                    type="checkbox"
                    checked={Boolean(relistSelected[product.product_id])}
                    onChange={() => toggleRelistSelect(product.product_id)}
                  />
                  <div>
                    <strong>{product.offer_id}</strong>
                    <span>{product.name ?? c.read.productFallback(product.product_id)}</span>
                  </div>
                </label>
              ))}
            </div>
          </div>

          <div className="panel relist-output">
            <div className="section-title compact-title">
              <ImageIcon />
              <div>
                <h2>{c.relist.previewTitle}</h2>
                <p>{c.relist.previewDescription}</p>
              </div>
            </div>
            {!relistResults && <p className="empty">{c.relist.previewEmpty}</p>}
            {relistResults && (
              <>
                <div className="relist-results">
                  {relistResults.map((item) => (
                    <div
                      className={`relist-card ${item.error ? "failed" : ""}`}
                      key={`${item.product_id}-${item.offer_id}`}
                    >
                      <div className="relist-card-head">
                        <strong>{item.offer_id || item.product_id}</strong>
                        {item.name && <span>{item.name}</span>}
                        {item.imported && <span className="badge">{c.relist.importedBadge}</span>}
                      </div>
                      {item.error ? (
                        <p className="notice error">{userFacingError(item.error)}</p>
                      ) : (
                        <>
                          <div className="relist-ba">
                            <figure>
                              {item.original_url && <img src={item.original_url} alt="before" />}
                              <figcaption>{c.relist.before}</figcaption>
                            </figure>
                            <figure>
                              {selectedCandidateUrl(item, relistChoice[item.product_id]) && (
                                <img
                                  src={selectedCandidateUrl(item, relistChoice[item.product_id]) as string}
                                  alt="after"
                                />
                              )}
                              <figcaption>{c.relist.after}</figcaption>
                            </figure>
                          </div>
                          {item.candidates.length > 1 && (
                            <div className="relist-candidate-nav">
                              <button
                                type="button"
                                className="secondary-button"
                                onClick={() => stepRelistChoice(item, -1)}
                              >
                                {c.relist.candidatePrev}
                              </button>
                              <span>
                                {c.relist.candidateCounter(
                                  (relistChoice[item.product_id] ?? 0) + 1,
                                  item.candidates.length
                                )}
                              </span>
                              <button
                                type="button"
                                className="secondary-button"
                                onClick={() => stepRelistChoice(item, 1)}
                              >
                                {c.relist.candidateNext}
                              </button>
                            </div>
                          )}
                          <label className="relist-adopt">
                            <input
                              type="checkbox"
                              checked={Boolean(relistAdopt[item.product_id])}
                              onChange={() => toggleRelistAdopt(item.product_id)}
                            />
                            {c.relist.adopt}
                          </label>
                          {item.imported && (
                            <p className="notice">{c.relist.importedPushHint}</p>
                          )}
                        </>
                      )}
                    </div>
                  ))}
                </div>
                <div className="relist-push-row">
                  <p className="relist-warn">{c.relist.pushWarning}</p>
                  <button onClick={pushRelist} disabled={relistPushing || relistAdoptCount === 0}>
                    <Play size={16} /> {relistPushing ? c.relist.pushing : c.relist.push(relistAdoptCount)}
                  </button>
                </div>
                {relistPushResults && (
                  <div className="relist-push-results">
                    {relistPushResults.map((result) => (
                      <div className={`badge ${result.ok ? "ok" : "error"}`} key={result.product_id}>
                        {result.ok
                          ? c.relist.pushedOne(result.product_id, result.image_count)
                          : c.relist.pushFailedOne(result.product_id, userFacingError(result.error ?? ""))}
                      </div>
                    ))}
                  </div>
                )}
              </>
            )}
          </div>
        </section>
      )}

      {view === "copy" && (
        <section className="workspace-grid relist-grid" id="module3-workbench">
          <div className="panel relist-pick">
            <div className="section-title">
              <FileText />
              <div>
                <h2>{c.module3.title}</h2>
                <p>{c.module3.description}</p>
              </div>
            </div>
            {!canUseOzonReadTools && <p className="notice">{ozonReadGateMessage || c.gates.connectLocalFirst}</p>}
            <div className="relist-toolbar">
              <button onClick={loadModule3Products} disabled={!canUseOzonReadTools}>
                <DatabaseZap size={16} /> {c.module3.loadProducts}
              </button>
              <button
                className="secondary-button"
                onClick={module3Recognize}
                disabled={module3Busy || !canUseOzonReadTools || module3SelectedCount === 0}
              >
                <FileText size={16} /> {module3Busy ? c.module3.recognizing : c.module3.recognize(module3SelectedCount)}
              </button>
            </div>
            <div className="product-list relist-list">
              {module3Products.length === 0 && <p className="empty">{c.module3.emptyProducts}</p>}
              {module3Products.map((product) => (
                <label
                  key={product.product_id}
                  className={`relist-row ${module3Selected[product.product_id] ? "picked" : ""}`}
                >
                  <input
                    type="checkbox"
                    checked={Boolean(module3Selected[product.product_id])}
                    onChange={() => toggleModule3Select(product.product_id)}
                  />
                  <div>
                    <strong>{product.offer_id}</strong>
                    <span>{product.name ?? c.read.productFallback(product.product_id)}</span>
                  </div>
                </label>
              ))}
            </div>
          </div>

          <div className="panel relist-output">
            <div className="section-title compact-title">
              <Languages />
              <div>
                <h2>{c.module3.previewTitle}</h2>
                <p>{c.module3.previewDescription}</p>
              </div>
            </div>
            {!module3Results && <p className="empty">{c.module3.previewEmpty}</p>}
            {module3Results && (
              <>
                <div className="relist-results">
                  {module3Results.map((item) => {
                    const edited = module3Edited[item.product_id];
                    const previewItem = module3Previews?.find((entry) => entry.product_id === item.product_id);
                    const writeResult = module3PushResults?.find((entry) => entry.product_id === item.product_id);
                    return (
                      <div
                        className={`relist-card ${item.error ? "failed" : ""}`}
                        key={`${item.product_id}-${item.offer_id}`}
                      >
                        <div className="relist-card-head">
                          <strong>{item.offer_id || item.product_id}</strong>
                        </div>
                        {item.error && <p className="notice error">{userFacingError(item.error)}</p>}
                        <div className="module3-ba">
                          <div className="module3-col">
                            <h4>{c.module3.before}</h4>
                            {renderModule3Fields(item.source, c)}
                          </div>
                          <div className="module3-col">
                            <h4>{c.module3.after}</h4>
                            {item.proposal && edited ? (
                              <div className="module3-edit">
                                <label>
                                  <span>{c.module3.fieldTitle}</span>
                                  <input
                                    type="text"
                                    value={edited.title}
                                    onChange={(event) => editModule3Field(item.product_id, "title", event.target.value)}
                                  />
                                </label>
                                <label>
                                  <span>{c.module3.fieldDescription}</span>
                                  <textarea
                                    rows={4}
                                    value={edited.description}
                                    onChange={(event) => editModule3Field(item.product_id, "description", event.target.value)}
                                  />
                                </label>
                                {renderModule3Fields(
                                  { ...item.proposal, title: edited.title, description: edited.description },
                                  c
                                )}
                              </div>
                            ) : (
                              <p className="empty">{c.module3.noProposal}</p>
                            )}
                          </div>
                        </div>
                        {item.proposal && !item.error && (
                          <label className="relist-adopt">
                            <input
                              type="checkbox"
                              checked={Boolean(module3Adopt[item.product_id])}
                              onChange={() => toggleModule3Adopt(item.product_id)}
                            />
                            {c.module3.adopt}
                          </label>
                        )}
                        {previewItem?.preview && (
                          <div className="module3-preview">
                            <h5>{c.module3.previewHeading}</h5>
                            <dl className="module3-fields">
                              <dt>{c.module3.fieldTitle}</dt>
                              <dd>
                                <span className="module3-before">{previewItem.preview.title.before || "—"}</span>
                                {" → "}
                                <span className="module3-after">{previewItem.preview.title.after || "—"}</span>
                              </dd>
                              <dt>{c.module3.fieldDescription}</dt>
                              <dd>
                                <span className="module3-before">{previewItem.preview.description.before || "—"}</span>
                                {" → "}
                                <span className="module3-after">{previewItem.preview.description.after || "—"}</span>
                              </dd>
                            </dl>
                            <p className="module3-matched-head">{c.module3.attributesToWrite(previewItem.preview.attributes_to_write.length)}</p>
                            {previewItem.preview.attributes_to_write.length > 0 ? (
                              <ul className="module3-attrs">
                                {previewItem.preview.attributes_to_write.map((attr) => (
                                  <li key={attr.attribute_id}>
                                    <strong>{attr.name}</strong> (#{attr.attribute_id}): {attr.values.join(", ")}
                                  </li>
                                ))}
                              </ul>
                            ) : (
                              <p className="empty">{c.module3.noneToWrite}</p>
                            )}
                            {previewItem.preview.dropped.length > 0 && (
                              <>
                                <p className="module3-dropped-head">{c.module3.droppedHeading(previewItem.preview.dropped.length)}</p>
                                <ul className="module3-dropped">
                                  {previewItem.preview.dropped.map((dropped, index) => (
                                    <li key={`${dropped.name}-${dropped.value ?? ""}-${index}`}>
                                      <strong>{dropped.name || "—"}</strong>
                                      {dropped.value ? ` = ${dropped.value}` : ""} — {dropped.reason}
                                    </li>
                                  ))}
                                </ul>
                              </>
                            )}
                          </div>
                        )}
                        {writeResult && (
                          <div className={`badge ${writeResult.written && !writeResult.error ? "ok" : "error"}`}>
                            {writeResult.written && !writeResult.error
                              ? c.module3.pushedOne(writeResult.offer_id || writeResult.product_id)
                              : c.module3.pushFailedOne(writeResult.offer_id || writeResult.product_id, userFacingError(writeResult.error ?? ""))}
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
                <div className="relist-push-row">
                  <p className="relist-warn">{c.module3.pushWarning}</p>
                  <div className="module3-actions">
                    <button
                      className="secondary-button"
                      onClick={previewModule3Push}
                      disabled={module3Pushing || module3AdoptCount === 0}
                    >
                      {module3Pushing && !module3PreviewReady ? c.module3.previewing : c.module3.previewPush(module3AdoptCount)}
                    </button>
                    <button
                      onClick={pushModule3}
                      disabled={module3Pushing || module3AdoptCount === 0 || !module3PreviewReady}
                    >
                      <Play size={16} /> {module3Pushing && module3PreviewReady ? c.module3.pushing : c.module3.push(module3AdoptCount)}
                    </button>
                  </div>
                  {!module3PreviewReady && <p className="hint">{c.module3.previewFirstHint}</p>}
                </div>
              </>
            )}
          </div>
        </section>
      )}

      {view === "video" && (
        <section className="workspace-grid relist-grid" id="module6-workbench">
          <div className="panel relist-pick">
            <div className="section-title">
              <Clapperboard />
              <div>
                <h2>{c.video.title}</h2>
                <p>{c.video.description}</p>
              </div>
            </div>

            {!canUseOzonReadTools && <p className="hint">{ozonReadGateMessage}</p>}
            <p className="hint">{c.video.frameHint}</p>

            <label className="field">
              <span>{c.video.firstFrameLabel}</span>
              {videoFrameOptions.length > 0 && (
                <select
                  value=""
                  onChange={(event) => {
                    if (event.target.value) setVideoFirstFrame(event.target.value);
                  }}
                >
                  <option value="">{c.video.framePick}</option>
                  {videoFrameOptions.map((option) => (
                    <option key={`first-${option.url}`} value={option.url}>
                      {option.label}
                    </option>
                  ))}
                </select>
              )}
              <input
                type="text"
                value={videoFirstFrame}
                placeholder={c.video.firstFramePlaceholder}
                onChange={(event) => setVideoFirstFrame(event.target.value)}
              />
            </label>

            <label className="field">
              <span>{c.video.lastFrameLabel}</span>
              {videoFrameOptions.length > 0 && (
                <select
                  value=""
                  onChange={(event) => setVideoLastFrame(event.target.value)}
                >
                  <option value="">{c.video.lastFrameNone}</option>
                  {videoFrameOptions.map((option) => (
                    <option key={`last-${option.url}`} value={option.url}>
                      {option.label}
                    </option>
                  ))}
                </select>
              )}
              <input
                type="text"
                value={videoLastFrame}
                placeholder={c.video.lastFramePlaceholder}
                onChange={(event) => setVideoLastFrame(event.target.value)}
              />
            </label>

            <label className="field">
              <span>{c.video.promptLabel}</span>
              <textarea
                rows={3}
                value={videoPrompt}
                placeholder={c.video.promptPlaceholder}
                onChange={(event) => setVideoPrompt(event.target.value)}
              />
            </label>

            <label className="field">
              <span>{c.video.durationLabel}</span>
              <input
                type="number"
                min={1}
                max={15}
                value={videoDuration}
                onChange={(event) => {
                  const next = Number(event.target.value);
                  if (Number.isFinite(next)) {
                    setVideoDuration(Math.min(15, Math.max(1, Math.round(next))));
                  }
                }}
              />
            </label>

            <button
              className="primary-button"
              disabled={!canUseOzonReadTools || videoBusy || !videoFirstFrame.trim()}
              onClick={createVideo}
            >
              <Clapperboard size={16} /> {videoBusy ? c.video.generating : c.video.generate}
            </button>
          </div>

          <div className="panel relist-results">
            <div className="section-title">
              <Clapperboard />
              <div>
                <h2>{c.video.resultTitle}</h2>
                <p>{c.video.resultSubtitle}</p>
              </div>
            </div>

            {!videoJob ? (
              <p className="hint">{c.video.noJob}</p>
            ) : (
              <div className="video-job">
                <div className={`result-banner ${videoJob.status === "succeeded" ? "result-ok" : videoJob.status === "failed" ? "result-error" : "result-warn"}`}>
                  <strong>{c.video.statusLabel}:</strong> {c.video.statusText(videoJob.status)}
                </div>

                {videoJob.first_frame_url && (
                  <p className="hint">{c.video.firstFrameShown}: {videoJob.first_frame_url}</p>
                )}

                {videoJob.status === "succeeded" && videoJob.video_url && (
                  <div className="video-result">
                    <video controls src={videoJob.video_url} style={{ maxWidth: "100%" }} />
                    <p className="export-path">
                      <strong>{c.video.urlLabel}:</strong>{" "}
                      <a href={videoJob.video_url} target="_blank" rel="noreferrer">{videoJob.video_url}</a>
                    </p>
                    <p className="hint">{c.video.reviewHint}</p>
                  </div>
                )}

                {videoJob.status === "failed" && videoJob.error && (
                  <div className="result-banner result-error">
                    <XCircle size={16} /> {videoJob.error}
                  </div>
                )}
              </div>
            )}
          </div>
        </section>
      )}

      {view === "export" && (() => {
        const exportRows = assembleExportRows();
        return (
        <section className="workspace-grid relist-grid" id="module4-workbench">
          <div className="panel relist-pick">
            <div className="section-title">
              <Download />
              <div>
                <h2>{c.export.title}</h2>
                <p>{c.export.description}</p>
              </div>
            </div>

            {!canUseOzonReadTools && <p className="hint">{ozonReadGateMessage}</p>}
            <p className="hint">{c.export.engineHint}</p>

            <label className="field">
              <span>{c.export.templateLabel}</span>
              <input
                type="text"
                value={exportTemplatePath}
                placeholder={c.export.templatePlaceholder}
                onChange={(event) => setExportTemplatePath(event.target.value)}
              />
            </label>
            <label className="field">
              <span>{c.export.configLabel}</span>
              <input
                type="text"
                value={exportConfigPath}
                placeholder={c.export.configPlaceholder}
                onChange={(event) => setExportConfigPath(event.target.value)}
              />
            </label>

            <button
              className="primary-button"
              disabled={!canUseOzonReadTools || exportBusy || exportRows.length === 0}
              onClick={runExport}
            >
              <Download size={16} /> {exportBusy ? c.export.generating : c.export.generate}
            </button>
          </div>

          <div className="panel relist-results">
            <div className="section-title">
              <FileSpreadsheet />
              <div>
                <h2>{c.export.rowsTitle}</h2>
                <p>{c.export.rowsSubtitle(exportRows.length)}</p>
              </div>
            </div>

            {exportRows.length === 0 ? (
              <p className="hint">{c.export.noRows}</p>
            ) : (
              <table className="export-rows">
                <thead>
                  <tr>
                    <th>{c.export.colProduct}</th>
                    <th>{c.export.colTitle}</th>
                    <th>{c.export.colListing}</th>
                    <th>{c.export.colImage}</th>
                  </tr>
                </thead>
                <tbody>
                  {exportRows.map((row) => (
                    <tr key={row.product_id}>
                      <td>{row.offer_id || row.product_id}</td>
                      <td>{row.title || <em>{c.export.missing}</em>}</td>
                      <td>{row.listing ? `${row.listing.slice(0, 80)}${row.listing.length > 80 ? "…" : ""}` : <em>{c.export.missing}</em>}</td>
                      <td>{row.primary_image_url ? <span className="ok-pill">{c.export.haveImage}</span> : <em>{c.export.missing}</em>}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}

            {exportError && (
              <div className="result-banner result-error">
                <XCircle size={16} /> {c.export.hardFail}: {exportError}
              </div>
            )}

            {exportResult && (
              <div className={`result-banner ${exportResult.verify?.ok ? "result-ok" : "result-warn"}`}>
                <div>
                  <CheckCircle2 size={16} /> {c.export.doneTitle}
                </div>
                <div className="export-path"><strong>{c.export.filePath}:</strong> {exportResult.out_path}</div>
                {exportResult.verify && (
                  <div className="export-verify">
                    {exportResult.verify.ok ? c.export.verifyOk : c.export.verifyFail}
                    {" — "}
                    {c.export.verifyDetail(
                      exportResult.verify.expected_changes,
                      exportResult.verify.unexpected_changes,
                      exportResult.verify.frozen_cells_compared
                    )}
                  </div>
                )}
                {exportResult.warnings.length > 0 && (
                  <ul className="export-warnings">
                    {exportResult.warnings.map((warning, index) => (
                      <li key={index}>{warning}</li>
                    ))}
                  </ul>
                )}
              </div>
            )}
          </div>
        </section>
        );
      })()}

      {view === "console" && (
        <>
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
              <div className="input-with-reveal">
                <input
                  autoComplete="off"
                  placeholder={c.config.apiKeyPlaceholder}
                  value={apiKey}
                  onChange={(event) => setApiKey(event.target.value)}
                  type={showApiKey ? "text" : "password"}
                />
                <button
                  type="button"
                  className="reveal-toggle"
                  onClick={() => setShowApiKey((value) => !value)}
                  title={showApiKey ? c.config.hideKey : c.config.revealKey}
                  aria-label={showApiKey ? c.config.hideKey : c.config.revealKey}
                >
                  {showApiKey ? <EyeOff size={16} /> : <Eye size={16} />}
                </button>
              </div>
            </label>
          </div>
          <button onClick={saveConfig}>
            <CheckCircle2 size={18} /> {c.config.saveAndValidate}
          </button>
          {saveNotice && <p className={`notice save-notice ${saveNotice.tone}`}>{saveNotice.text}</p>}
          {configStatus?.ozon.configured && !saveNotice && (
            <p className="notice save-notice ok">
              {c.config.savedFingerprint(configStatus.ozon.api_key_fingerprint ?? c.advanced.notSaved)}
            </p>
          )}
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

            <div className="panel provider-panel">
              <div className="section-title">
                <SlidersHorizontal />
                <div>
                  <h2>{c.providers.title}</h2>
                  <p>{c.providers.description}</p>
                </div>
              </div>

              {/* Sub-form A: save a named secret (returns fingerprint only). */}
              <div className="provider-subform">
                <div className="section-subtitle">{c.providers.secretFormTitle}</div>
                <p className="hint">{c.providers.secretFormHint}</p>
                <div className="form-grid compact">
                  <label>
                    {c.providers.secretNameLabel}
                    <input
                      autoComplete="off"
                      placeholder="my_provider_key"
                      value={secretName}
                      onChange={(event) => setSecretName(event.target.value)}
                    />
                  </label>
                  <label>
                    {c.providers.secretValueLabel}
                    <input
                      autoComplete="off"
                      type="password"
                      placeholder={c.providers.secretValuePlaceholder}
                      value={secretValue}
                      onChange={(event) => setSecretValue(event.target.value)}
                    />
                  </label>
                </div>
                <button className="secondary-button" onClick={saveSecret} disabled={providerBusy}>
                  <CheckCircle2 size={18} /> {c.providers.saveSecret}
                </button>
                {secretFingerprint && (
                  <p className="notice">{c.providers.savedFingerprint(secretFingerprint)}</p>
                )}
              </div>

              {/* Sub-form B: add/replace a provider entry (GET-merge-POST). */}
              <div className="provider-subform">
                <div className="section-subtitle">{c.providers.providerFormTitle}</div>
                <p className="hint">{c.providers.providerFormHint}</p>
                <div className="form-grid compact">
                  <label>
                    {c.providers.capabilityLabel}
                    <select
                      value={providerCapability}
                      onChange={(event) =>
                        setProviderCapability(event.target.value as typeof providerCapability)
                      }
                    >
                      <option value="image_gen">image_gen</option>
                      <option value="text_gen">text_gen</option>
                      <option value="video_gen">video_gen</option>
                    </select>
                  </label>
                  <label>
                    {c.providers.kindLabel}
                    <select
                      value={providerKind}
                      onChange={(event) => setProviderKind(event.target.value as typeof providerKind)}
                    >
                      <option value="openai_images">openai_images</option>
                      <option value="openai_images_edit">openai_images_edit</option>
                      <option value="openai_compat_chat">openai_compat_chat</option>
                      <option value="cloud_video">cloud_video</option>
                    </select>
                  </label>
                  <label>
                    {c.providers.baseUrlLabel}
                    <input
                      autoComplete="off"
                      placeholder="https://api.example.com/v1"
                      value={providerBaseUrl}
                      onChange={(event) => setProviderBaseUrl(event.target.value)}
                    />
                  </label>
                  <label>
                    {c.providers.modelLabel}
                    <input
                      autoComplete="off"
                      placeholder="model-id"
                      value={providerModel}
                      onChange={(event) => setProviderModel(event.target.value)}
                    />
                  </label>
                  <label>
                    {c.providers.secretRefLabel}
                    <input
                      autoComplete="off"
                      placeholder="my_provider_key"
                      value={providerSecretRef}
                      onChange={(event) => setProviderSecretRef(event.target.value)}
                    />
                  </label>
                  <label>
                    {c.providers.authLabel}
                    <select
                      value={providerAuthStyle}
                      onChange={(event) =>
                        setProviderAuthStyle(event.target.value as typeof providerAuthStyle)
                      }
                    >
                      <option value="bearer">bearer</option>
                      <option value="header">header</option>
                      <option value="query">query</option>
                    </select>
                  </label>
                  {providerAuthStyle !== "bearer" && (
                    <label>
                      {c.providers.authParamLabel}
                      <input
                        autoComplete="off"
                        placeholder={providerAuthStyle === "header" ? "X-Api-Key" : "api_key"}
                        value={providerAuthParam}
                        onChange={(event) => setProviderAuthParam(event.target.value)}
                      />
                    </label>
                  )}
                  <label className="checkbox-label">
                    <input
                      type="checkbox"
                      checked={providerEnabled}
                      onChange={(event) => setProviderEnabled(event.target.checked)}
                    />
                    {c.providers.enabledLabel}
                  </label>
                </div>
                <button className="secondary-button" onClick={saveProvider} disabled={providerBusy}>
                  <CheckCircle2 size={18} /> {c.providers.saveProvider}
                </button>
              </div>

              {/* Current providers, sourced from configStatus.capabilities. */}
              <div className="provider-current">
                <div className="section-subtitle">{c.providers.currentTitle}</div>
                {(configStatus?.capabilities ?? []).map((cap) => (
                  <div className="status-item" key={cap.capability}>
                    <span>{cap.capability}</span>
                    <strong>
                      {cap.provider_kind ?? c.providers.none}
                      {cap.model ? ` · ${cap.model}` : ""}
                    </strong>
                    <em className={cap.ready ? "badge ok-badge" : "badge warn-badge"}>
                      {cap.ready ? c.providers.ready : c.providers.notReady}
                    </em>
                  </div>
                ))}
              </div>
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

        </>
      )}

      {cockpitOpen && (
        <CockpitView
          c={c}
          nodes={cockpitNodes}
          summary={cockpitSummary}
          live={cockpitLive}
          refreshing={cockpitRefreshing}
          onRefresh={refreshCockpit}
          onClose={() => setCockpitOpen(false)}
          eventState={eventState}
        />
      )}

      <footer>
        <TerminalSquare size={18} />
        <span>{message}</span>
        {health ? <CheckCircle2 size={18} /> : <XCircle size={18} />}
      </footer>
    </main>
  );
}

type CockpitStatus = "live" | "partial" | "isolated" | "missing";
type CockpitChipState = "ok" | "wip" | "gap";
type CockpitChip = { label: string; state: CockpitChipState };
type CockpitLane = "observer" | "pipeline" | "foundation" | "branch";

type CockpitNode = {
  id: number;
  lane: CockpitLane;
  status: CockpitStatus;
  runtime: "ok" | "warn" | "off";
  title: string;
  role: string;
  next: string;
  gaps: string[];
  backing: string[];
  chips: CockpitChip[];
};

type CockpitSignals = {
  nodeUp: boolean;
  bridge: boolean;
  lease: boolean;
  secretBackend: string | null;
  secretAvailable: boolean;
  ozon: boolean;
  openai: boolean;
  connector: string;
  posterReady: boolean;
  imageRouting: boolean;
};

type CockpitSummary = {
  counts: Record<CockpitStatus, number>;
  live: number;
  total: number;
  wiredPct: number;
  buildCommit: string | null;
  version: string | null;
  connector: string;
  lease: boolean;
  nodeUp: boolean;
  checkedAt: string | null;
};

const COCKPIT_ICONS: Record<number, React.ComponentType<{ size?: number | string }>> = {
  0: Boxes,
  1: PackagePlus,
  2: ImageIcon,
  3: Languages,
  4: FileSpreadsheet,
  5: Cpu,
  6: Clapperboard
};

function cockpitStatusLabel(c: LocalNodeCopy, status: CockpitStatus) {
  switch (status) {
    case "live":
      return c.cockpit.statusLive;
    case "partial":
      return c.cockpit.statusPartial;
    case "isolated":
      return c.cockpit.statusIsolated;
    default:
      return c.cockpit.statusMissing;
  }
}

// Build the 7-module project map. The structural `status` is the honest "is this
// wired end-to-end" verdict from the codebase survey; the `runtime` dot reflects
// whether the part that IS wired is actually up right now (from live /health +
// /diagnostics). Chips break each module into concrete capabilities (ok / wip / gap).
function buildCockpitNodes(c: LocalNodeCopy, sig: CockpitSignals): CockpitNode[] {
  const m = c.cockpit.modules;
  const ch = c.cockpit.chips;
  const chip = (label: string, state: CockpitChipState): CockpitChip => ({ label, state });
  const secretLabel = sig.secretBackend ? `${ch.secretStore} · ${sig.secretBackend}` : ch.secretStore;
  return [
    {
      id: 0,
      lane: "observer",
      status: "live",
      runtime: sig.nodeUp ? "ok" : "off",
      title: m.m0.title,
      role: m.m0.role,
      next: m.m0.next,
      gaps: m.m0.gaps,
      backing: m.m0.backing,
      chips: [chip(ch.observe, "ok")]
    },
    {
      id: 1,
      lane: "pipeline",
      status: "partial",
      runtime: sig.ozon ? "ok" : sig.nodeUp ? "warn" : "off",
      title: m.m1.title,
      role: m.m1.role,
      next: m.m1.next,
      gaps: m.m1.gaps,
      backing: m.m1.backing,
      chips: [chip(ch.ozonRead, sig.ozon ? "ok" : "gap"), chip(ch.excelReader, "gap"), chip(ch.dropImport, "gap")]
    },
    {
      id: 2,
      lane: "pipeline",
      status: "partial",
      runtime: sig.ozon && sig.posterReady ? "ok" : sig.nodeUp ? "warn" : "off",
      title: m.m2.title,
      role: m.m2.role,
      next: m.m2.next,
      gaps: m.m2.gaps,
      backing: m.m2.backing,
      chips: [chip(ch.genChain, "ok"), chip(ch.adoptUi, "ok"), chip(ch.persist, "gap")]
    },
    {
      id: 3,
      lane: "pipeline",
      status: "live",
      runtime: sig.openai ? "ok" : sig.nodeUp ? "warn" : "off",
      title: m.m3.title,
      role: m.m3.role,
      next: m.m3.next,
      gaps: m.m3.gaps,
      backing: m.m3.backing,
      chips: [chip(ch.rewrite, "ok"), chip(ch.multimodal, "ok"), chip(ch.yandex, "gap")]
    },
    {
      id: 4,
      lane: "pipeline",
      status: "live",
      runtime: sig.nodeUp ? "ok" : "off",
      title: m.m4.title,
      role: m.m4.role,
      next: m.m4.next,
      gaps: m.m4.gaps,
      backing: m.m4.backing,
      chips: [chip(ch.excelEngine, "ok"), chip(ch.verifier, "ok"), chip(ch.httpExpose, "ok")]
    },
    {
      id: 5,
      lane: "foundation",
      status: "live",
      runtime: sig.lease && sig.secretAvailable ? "ok" : sig.nodeUp ? "warn" : "off",
      title: m.m5.title,
      role: m.m5.role,
      next: m.m5.next,
      gaps: m.m5.gaps,
      backing: m.m5.backing,
      chips: [
        chip(ch.lease, sig.lease ? "ok" : "gap"),
        chip(secretLabel, sig.secretAvailable ? "ok" : "gap"),
        chip(ch.modelRouting, sig.imageRouting ? "ok" : "wip"),
        chip(ch.singleShop, "gap")
      ]
    },
    {
      id: 6,
      lane: "branch",
      status: "partial",
      runtime: "off",
      title: m.m6.title,
      role: m.m6.role,
      next: m.m6.next,
      gaps: m.m6.gaps,
      backing: m.m6.backing,
      chips: [chip(ch.firstLastFrame, "ok"), chip(ch.videoApi, "wip"), chip(ch.videoPush, "gap")]
    }
  ];
}

const COCKPIT_EDGES: [number, number][] = [[1,2],[2,3],[3,4],[5,1],[5,2],[5,3],[5,4],[6,2],[6,4],[5,6],[0,1],[0,2],[0,3],[0,4],[0,5],[0,6]];

function cockpitRelated(id: number): Set<number> {
  const s = new Set<number>([id]);
  for (const [a, b] of COCKPIT_EDGES) {
    if (a === id) s.add(b);
    if (b === id) s.add(a);
  }
  return s;
}

function CockpitNodeCard({
  node,
  c,
  selected,
  onSelect,
  dark,
  tag,
  related,
  dim
}: {
  node: CockpitNode;
  c: LocalNodeCopy;
  selected: boolean;
  onSelect: (id: number) => void;
  dark?: boolean;
  tag?: string;
  related?: boolean;
  dim?: boolean;
}) {
  const Icon = COCKPIT_ICONS[node.id] ?? Boxes;
  return (
    <button
      type="button"
      className={`cockpit-node is-${node.status}${dark ? " on-dark" : ""}${selected ? " is-selected" : ""}${related ? " cockpit-related" : ""}${dim ? " cockpit-dim" : ""}`}
      onClick={() => onSelect(node.id)}
      aria-pressed={selected}
    >
      {tag && <span className="cockpit-node-tag">{tag}</span>}
      <div className="cockpit-node-top">
        <span className="cockpit-node-num">{node.id}</span>
        <span className="cockpit-node-icon">
          <Icon size={18} />
        </span>
        <h3>{node.title}</h3>
        <span className={`cockpit-dot ${node.runtime}`} aria-hidden />
        <span className={`cockpit-badge is-${node.status}`}>{cockpitStatusLabel(c, node.status)}</span>
      </div>
      <p className="cockpit-node-role">{node.role}</p>
      <div className="cockpit-chips">
        {node.chips.map((entry, index) => (
          <span key={index} className={`cockpit-chip ${entry.state}`}>
            {entry.label}
          </span>
        ))}
      </div>
    </button>
  );
}

function CockpitView({
  c,
  nodes,
  summary,
  live,
  refreshing,
  onRefresh,
  onClose,
  eventState
}: {
  c: LocalNodeCopy;
  nodes: CockpitNode[];
  summary: CockpitSummary;
  live: Record<number, { label: string; value: string }[]>;
  refreshing: boolean;
  onRefresh: () => void;
  onClose: () => void;
  eventState: string;
}) {
  const [selected, setSelected] = useState(0);
  const [todoCopied, setTodoCopied] = useState(false);
  const observer = nodes.find((node) => node.lane === "observer");
  const pipeline = nodes.filter((node) => node.lane === "pipeline");
  const branch = nodes.find((node) => node.lane === "branch");
  const foundation = nodes.find((node) => node.lane === "foundation");
  const active = nodes.find((node) => node.id === selected) ?? nodes[0];
  const related = cockpitRelated(active.id);

  useEffect(() => {
    setTodoCopied(false);
  }, [selected]);

  async function copyTodo() {
    const md = [
      "## [模块" + active.id + "] " + active.title + " — " + cockpitStatusLabel(c, active.status),
      "",
      c.cockpit.detailGaps + ":",
      ...active.gaps.map((g) => "- [ ] " + g),
      "",
      c.cockpit.detailNext + ": " + active.next,
      c.cockpit.detailBacking + ": " + active.backing.join("; ")
    ].join("\n");
    try {
      await navigator.clipboard.writeText(md);
      setTodoCopied(true);
      window.setTimeout(() => setTodoCopied(false), 2000);
    } catch {
      setTodoCopied(false);
    }
  }

  return (
    <div
      className="cockpit-overlay"
      role="dialog"
      aria-modal="true"
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          onClose();
        }
      }}
    >
    <section className="cockpit">
      <div className="cockpit-head">
        <div className="section-title">
          <Boxes />
          <div>
            <h2>{c.cockpit.title}</h2>
            <p>{c.cockpit.subtitle}</p>
          </div>
        </div>
        <div className="cockpit-head-actions">
          {summary.checkedAt && <span className="cockpit-checked">{c.cockpit.checkedAt(summary.checkedAt)}</span>}
          <button className="secondary-button" onClick={onRefresh} disabled={refreshing}>
            <RefreshCcw size={16} /> {refreshing ? c.cockpit.refreshing : c.cockpit.refresh}
          </button>
          <button className="cockpit-close" onClick={onClose} aria-label={c.cockpit.close}>
            <XCircle size={20} />
          </button>
        </div>
      </div>

      <div className="cockpit-pulse">
        <div className="cockpit-pulse-counts">
          <span className="cockpit-tally live">
            <strong>{summary.counts.live}</strong>
            {c.cockpit.statusLive}
          </span>
          <span className="cockpit-tally partial">
            <strong>{summary.counts.partial}</strong>
            {c.cockpit.statusPartial}
          </span>
          <span className="cockpit-tally isolated">
            <strong>{summary.counts.isolated}</strong>
            {c.cockpit.statusIsolated}
          </span>
          <span className="cockpit-tally missing">
            <strong>{summary.counts.missing}</strong>
            {c.cockpit.statusMissing}
          </span>
        </div>
        <div className="cockpit-pulse-meta">
          <div>
            <span>{c.cockpit.pulseBuild}</span>
            <strong>{summary.buildCommit ? summary.buildCommit.slice(0, 7) : "—"}</strong>
          </div>
          <div>
            <span>{c.cockpit.pulseConnector}</span>
            <strong>{summary.connector}</strong>
          </div>
          <div>
            <span>{summary.lease ? c.cockpit.pulseLeaseOk : c.cockpit.pulseLeaseOff}</span>
            <strong className={summary.lease ? "ok" : "warn"}>{summary.lease ? "✓" : "—"}</strong>
          </div>
          <div>
            <span>{summary.nodeUp ? c.cockpit.pulseNodeOk : c.cockpit.pulseNodeOff}</span>
            <strong className={summary.nodeUp ? "ok" : "warn"}>{summary.nodeUp ? "✓" : "—"}</strong>
          </div>
          <div>
            <span>{eventState === "connected" ? c.cockpit.liveOn : eventState === "connecting" ? c.cockpit.liveConnecting : c.cockpit.liveOff}</span>
            <strong className={eventState === "connected" ? "ok" : "warn"}>{eventState === "connected" ? "●" : "○"}</strong>
          </div>
        </div>
      </div>

      <div className="cockpit-progress">
        <div className="cockpit-progress-track">
          <div className="cockpit-progress-fill" style={{ width: `${summary.wiredPct}%` }} />
        </div>
        <span>
          {c.cockpit.wired} {summary.wiredPct}%
        </span>
      </div>

      <div className="cockpit-legend">
        <span className="cockpit-legend-item">
          <i className="cockpit-swatch live" />
          {c.cockpit.statusLive}
        </span>
        <span className="cockpit-legend-item">
          <i className="cockpit-swatch partial" />
          {c.cockpit.statusPartial}
        </span>
        <span className="cockpit-legend-item">
          <i className="cockpit-swatch isolated" />
          {c.cockpit.statusIsolated}
        </span>
        <span className="cockpit-legend-item">
          <i className="cockpit-swatch missing" />
          {c.cockpit.statusMissing}
        </span>
      </div>

      {observer && (
        <div className="cockpit-lane cockpit-observer">
          <div className="cockpit-lane-label">
            <Eye size={14} /> {c.cockpit.laneObserver}
          </div>
          <CockpitNodeCard
            node={observer}
            c={c}
            selected={active.id === observer.id}
            onSelect={setSelected}
            tag={c.cockpit.youAreHere}
            related={observer.id !== active.id && related.has(observer.id)}
            dim={!related.has(observer.id)}
          />
        </div>
      )}

      <div className="cockpit-lane">
        <div className="cockpit-lane-label">
          <ArrowRight size={14} /> {c.cockpit.lanePipeline}
        </div>
        <div className="cockpit-pipeline">
          {pipeline.map((node, index) => (
            <React.Fragment key={node.id}>
              <CockpitNodeCard
                node={node}
                c={c}
                selected={active.id === node.id}
                onSelect={setSelected}
                related={node.id !== active.id && related.has(node.id)}
                dim={!related.has(node.id)}
              />
              {index < pipeline.length - 1 && (
                <div className="cockpit-arrow" aria-hidden>
                  <ArrowRight size={20} />
                </div>
              )}
            </React.Fragment>
          ))}
        </div>
      </div>

      {branch && (
        <div className="cockpit-lane">
          <div className="cockpit-lane-label">
            <Clapperboard size={14} /> {c.cockpit.laneBranch}
          </div>
          <div className="cockpit-branch">
            <CockpitNodeCard
              node={branch}
              c={c}
              selected={active.id === branch.id}
              onSelect={setSelected}
              related={branch.id !== active.id && related.has(branch.id)}
              dim={!related.has(branch.id)}
            />
            <span className="cockpit-branch-hint">{c.cockpit.branchHint}</span>
          </div>
        </div>
      )}

      {foundation && (
        <div className="cockpit-lane">
          <div className="cockpit-lane-label">
            <Layers size={14} /> {c.cockpit.laneFoundation}
          </div>
          <CockpitNodeCard
            node={foundation}
            c={c}
            selected={active.id === foundation.id}
            onSelect={setSelected}
            dark
            related={foundation.id !== active.id && related.has(foundation.id)}
            dim={!related.has(foundation.id)}
          />
        </div>
      )}

      {active && (
        <div className="cockpit-detail">
          <div className="cockpit-detail-head">
            <span className="cockpit-node-num">{active.id}</span>
            <h3>{active.title}</h3>
            <span className={`cockpit-badge is-${active.status}`}>{cockpitStatusLabel(c, active.status)}</span>
          </div>
          <p className="cockpit-detail-role">{active.role}</p>
          <div className="cockpit-detail-cols">
            <div>
              <h4>{c.cockpit.detailBacking}</h4>
              <ul className="cockpit-mono">
                {active.backing.map((item, index) => (
                  <li key={index}>{item}</li>
                ))}
              </ul>
            </div>
            <div>
              <h4>{c.cockpit.detailGaps}</h4>
              <ul>
                {active.gaps.map((item, index) => (
                  <li key={index}>{item}</li>
                ))}
              </ul>
            </div>
          </div>
          {nodes.filter((n) => n.id !== active.id && related.has(n.id)).length > 0 && (
            <div className="cockpit-related-row">
              <h4>{c.cockpit.related}</h4>
              <div className="cockpit-related-chips">
                {nodes
                  .filter((n) => n.id !== active.id && related.has(n.id))
                  .map((n) => (
                    <button
                      key={n.id}
                      className="cockpit-related-chip"
                      onClick={() => setSelected(n.id)}
                    >
                      {n.id} · {n.title}
                    </button>
                  ))}
              </div>
            </div>
          )}
          {live[active.id] && live[active.id].length > 0 && (
            <div className="cockpit-live">
              <h4>{c.cockpit.liveReadout}</h4>
              <div className="cockpit-live-grid">
                {live[active.id].map((metric, index) => (
                  <div key={index} className="cockpit-metric">
                    <span>{metric.label}</span>
                    <strong>{metric.value}</strong>
                  </div>
                ))}
              </div>
            </div>
          )}
          <div className="cockpit-next">
            <span>{c.cockpit.detailNext}</span>
            <strong>{active.next}</strong>
            {active.status !== "live" && (
              <button className="secondary-button cockpit-todo-btn" onClick={copyTodo}>
                {c.cockpit.genTodo}
              </button>
            )}
            {todoCopied && <span className="cockpit-todo-ok">{c.cockpit.todoCopied}</span>}
          </div>
        </div>
      )}
    </section>
    </div>
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

function renderModule3Fields(fields: Module3Fields, c: LocalNodeCopy) {
  return (
    <dl className="module3-fields">
      <dt>{c.module3.fieldTitle}</dt>
      <dd>{fields.title || <span className="empty">—</span>}</dd>
      <dt>{c.module3.fieldDescription}</dt>
      <dd>{fields.description || <span className="empty">—</span>}</dd>
      <dt>{c.module3.fieldAttributes}</dt>
      <dd>
        {fields.attributes.length === 0 ? (
          <span className="empty">—</span>
        ) : (
          <ul className="module3-attrs">
            {fields.attributes.map((attribute, index) => (
              <li key={`${attribute.name}-${index}`}>
                <strong>{attribute.name || "—"}:</strong> {attribute.values.join("; ")}
              </li>
            ))}
          </ul>
        )}
      </dd>
      <dt>{c.module3.fieldTypeCategory}</dt>
      <dd>{fields.type_category || <span className="empty">—</span>}</dd>
    </dl>
  );
}

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
