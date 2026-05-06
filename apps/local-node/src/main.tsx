import React, { useEffect, useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import {
  Activity,
  CheckCircle2,
  Clipboard,
  DatabaseZap,
  KeyRound,
  ListChecks,
  PauseCircle,
  Play,
  Radio,
  RefreshCcw,
  Repeat2,
  ShieldCheck,
  SlidersHorizontal,
  TerminalSquare,
  Workflow,
  XCircle
} from "lucide-react";
import "./styles.css";

const LOCAL_API = import.meta.env.VITE_LOCAL_SKILL_API ?? "http://127.0.0.1:8790";
const AGENT_API = import.meta.env.VITE_LOCAL_AGENT_API ?? "http://127.0.0.1:17870";
const LOCAL_TOKEN = import.meta.env.VITE_LOCAL_TOKEN ?? "dev-local-token";

type Health = {
  service: string;
  status: string;
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
  const [health, setHealth] = useState<Health | null>(null);
  const [manifest, setManifest] = useState<Manifest | null>(null);
  const [configStatus, setConfigStatus] = useState<ConfigStatus | null>(null);
  const [clientId, setClientId] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [products, setProducts] = useState<Product[]>([]);
  const [productCount, setProductCount] = useState<number | null>(null);
  const [productListMeta, setProductListMeta] = useState<ProductListResult | null>(null);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [selectedOperation, setSelectedOperation] = useState("ozon_update_price_mock");
  const [eventState, setEventState] = useState("connecting");
  const [message, setMessage] = useState("本地节点尚未连接");
  const [validation, setValidation] = useState<ValidationResult | null>(null);
  const [schedule, setSchedule] = useState<ScheduleStatus | null>(null);
  const [scheduleInterval, setScheduleInterval] = useState(900);
  const [scheduleLimit, setScheduleLimit] = useState(20);

  const queueStats = useMemo(() => {
    const pending = tasks.filter((task) => task.state === "pending_approval").length;
    const queued = tasks.filter((task) => task.state === "queued").length;
    const done = tasks.filter((task) => task.state === "succeeded").length;
    return { pending, queued, done };
  }, [tasks]);

  async function api(path: string, init: RequestInit = {}) {
    return fetch(`${LOCAL_API}${path}`, {
      ...init,
      headers: {
        "Content-Type": "application/json",
        "x-local-token": LOCAL_TOKEN,
        ...(init.headers ?? {})
      }
    });
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

  async function checkHealth() {
    try {
      const [healthResponse, manifestResponse, configResponse] = await Promise.all([
        fetch(`${LOCAL_API}/health`),
        fetch(`${LOCAL_API}/openclaw/manifest`),
        api("/config/status")
      ]);
      setHealth(healthResponse.ok ? await healthResponse.json() : null);
      setManifest(manifestResponse.ok ? await manifestResponse.json() : null);
      setConfigStatus(configResponse.ok ? await configResponse.json() : null);
      setMessage(healthResponse.ok ? "本地服务已连接" : "本地服务未就绪");
    } catch {
      setHealth(null);
      setConfigStatus(null);
      setMessage("无法连接 127.0.0.1:8790");
    }
  }

  async function saveConfig() {
    if (!clientId.trim() || !apiKey.trim()) {
      setMessage("请填写真实的 Ozon Client ID 和 API Key");
      return;
    }
    const response = await api("/config/ozon", {
      method: "POST",
      body: JSON.stringify({ client_id: clientId.trim(), api_key: apiKey.trim() })
    });
    const data = await response.json();
    setMessage(response.ok ? `Ozon 凭据已保存：${data.client_id} / ${data.api_key}` : `保存失败：${data.error}`);
    if (response.ok) {
      await checkHealth();
    }
  }

  async function validateConfig() {
    const response = await api("/config/ozon/validate", { method: "POST" });
    const data = await response.json();
    if (!response.ok) {
      setValidation(null);
      setMessage(`Ozon 凭据校验失败：${data.error}`);
      return;
    }
    setValidation(data);
    setMessage(data.message);
    await checkHealth();
  }

  async function loadProducts() {
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
      setMessage(`Ozon 读取失败：${countData.error ?? listData.error}`);
      return;
    }
    setProductCount(countData.count);
    setProducts(listData.products);
    setProductListMeta(listData);
    setMessage(
      listData.connector_mode === "real"
        ? `真实 Ozon 商品读取完成：总数 ${countData.count}，当前样本 ${listData.products.length}`
        : "开发模式 mock 商品读取完成；上线请使用 OZON_CONNECTOR_MODE=real"
    );
  }

  async function createDryRun() {
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
    setMessage(response.ok ? "OpenClaw dry-run 提案已创建，等待本地审批" : `创建失败：${data.error}`);
    await refresh();
  }

  async function approve(taskId: string) {
    const response = await api(`/tasks/${taskId}/approve`, {
      method: "POST",
      body: JSON.stringify({ approved_by: "local-ui", note: "approved in local operator console" })
    });
    const data = await response.json();
    setMessage(response.ok ? "任务已审批并进入队列" : `审批失败：${data.error}`);
    await refresh();
  }

  async function execute(taskId: string) {
    const response = await api(`/tasks/${taskId}/execute-mock`, { method: "POST" });
    const data = await response.json();
    setMessage(response.ok ? "dry-run 执行完成，没有发送真实 Ozon 写操作" : `执行失败：${data.error}`);
    await refresh();
  }

  async function configureSchedule(enabled: boolean) {
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
      setMessage(`定时读取配置失败：${data.error}`);
      return;
    }
    setSchedule(data);
    setMessage(enabled ? "只读定时采集已启用" : "只读定时采集已停止");
  }

  async function runScheduleNow() {
    const response = await api("/schedules/ecommerce-read/run-now", { method: "POST" });
    const data = await response.json();
    if (!response.ok) {
      setMessage(`手动采集失败：${data.error}`);
      return;
    }
    setProducts(data.run.products);
    setProductCount(data.run.product_count);
    setMessage(`采集完成：${data.run.sample_size} 个样本，${data.run.duration_ms}ms`);
    await refresh();
  }

  async function copyManifest() {
    await navigator.clipboard.writeText(`${LOCAL_API}/openclaw/manifest`);
    setMessage("OpenClaw manifest URL 已复制");
  }

  useEffect(() => {
    checkHealth();
    refresh();
    let cancelled = false;

    async function connectEvents() {
      try {
        const response = await fetch(`${AGENT_API}/events`, {
          headers: { "x-local-token": LOCAL_TOKEN }
        });
        if (!response.ok || !response.body) {
          setEventState("offline");
          return;
        }
        setEventState("connected");
        const reader = response.body.getReader();
        const decoder = new TextDecoder();
        while (!cancelled) {
          const { value, done } = await reader.read();
          if (done) break;
          const chunk = decoder.decode(value, { stream: true });
          if (chunk.includes("event: task.changed")) {
            refresh();
          }
        }
      } catch {
        if (!cancelled) setEventState("offline");
      }
    }

    connectEvents();
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <main>
      <nav>
        <div>
          <strong>Ozon Rust Local</strong>
          <span>Seller API console</span>
        </div>
        <span className={health ? "ok" : "warn"}>{health ? "ready" : "offline"}</span>
      </nav>

      <section className="overview">
        <div>
          <Activity />
          <span>Skill API</span>
          <strong>{LOCAL_API.replace("http://", "")}</strong>
        </div>
        <div>
          <Radio />
          <span>Agent stream</span>
          <strong>{eventState}</strong>
        </div>
        <div>
          <ListChecks />
          <span>Pending approval</span>
          <strong>{queueStats.pending}</strong>
        </div>
        <div>
          <ShieldCheck />
          <span>Write policy</span>
          <strong>dry-run gated</strong>
        </div>
        <div>
          <SlidersHorizontal />
          <span>Ozon mode</span>
          <strong>{configStatus?.connector_mode === "real" ? "real API" : (configStatus?.connector_mode ?? "checking")}</strong>
        </div>
        <div>
          <Repeat2 />
          <span>Read schedule</span>
          <strong>{schedule?.enabled ? "enabled" : "paused"}</strong>
        </div>
      </section>

      <section className="grid">
        <div className="panel bridge-panel">
          <div className="section-title">
            <Workflow />
            <div>
              <h1>OpenClaw Bridge</h1>
              <p>读取真实 Ozon 商品；写操作只能提交提案，审批留在操作台。</p>
            </div>
          </div>
          <div className="bridge-endpoint">
            <code>{LOCAL_API}/openclaw/manifest</code>
            <button className="icon-button" onClick={copyManifest} title="复制 manifest URL">
              <Clipboard size={18} />
            </button>
          </div>
          <div className="tool-list">
            {(manifest?.tools ?? []).map((tool) => (
              <div key={tool.name}>
                <span>{tool.risk}</span>
                <strong>{tool.name}</strong>
                <em>{tool.approval_required ? "requires approval" : "read-only"}</em>
              </div>
            ))}
          </div>
        </div>

        <div className="panel config-panel">
          <div className="section-title">
            <KeyRound />
            <div>
              <h2>本地密钥</h2>
              <p>保存用户自己的 Ozon Seller API 凭据；写入 OS keyring。</p>
            </div>
          </div>
          <p className="notice mode-notice">
            {configStatus?.real_ozon_enabled
              ? "当前是真实 API 模式：未保存凭据时会拒绝读取，不会回退到假商品。"
              : "当前是开发 mock 模式：仅用于本机演示，上线请用 OZON_CONNECTOR_MODE=real 启动。"}
          </p>
          <div className="form-grid compact">
            <label>
              Ozon Client ID
              <input
                autoComplete="off"
                placeholder="从 Ozon Seller 后台复制"
                value={clientId}
                onChange={(event) => setClientId(event.target.value)}
              />
            </label>
            <label>
              Ozon API Key
              <input
                autoComplete="off"
                placeholder="保存后只显示指纹"
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
                type="password"
              />
            </label>
          </div>
          <button onClick={saveConfig}>
            <CheckCircle2 size={18} /> 保存凭据
          </button>
        </div>

        <div className="panel diagnostics-panel">
          <div className="section-title">
            <SlidersHorizontal />
            <div>
              <h2>本地自检</h2>
              <p>确认连接、密钥来源和 Ozon connector 模式。</p>
            </div>
          </div>
          <div className="inline-actions">
            <button onClick={checkHealth}>
              <RefreshCcw size={18} /> 刷新自检
            </button>
            <button onClick={validateConfig}>
              <ShieldCheck size={18} /> 校验 Ozon 凭据
            </button>
          </div>
          <div className="status-list">
            <div className="status-item">
              <span>Connector</span>
              <strong>{configStatus?.connector_mode ?? "unknown"}</strong>
              <em className={configStatus?.real_ozon_enabled ? "badge warn-badge" : "badge ok-badge"}>
                {configStatus?.real_ozon_enabled ? "real API" : "mock"}
              </em>
            </div>
            <div className="status-item">
              <span>Secret store</span>
              <strong>{configStatus?.secret_store.backend ?? "system_keyring"}</strong>
              <em className={configStatus?.secret_store.available ? "badge ok-badge" : "badge warn-badge"}>
                {configStatus?.secret_store.available ? "available" : "unavailable"}
              </em>
            </div>
            <div className="status-item">
              <span>Ozon config</span>
              <strong>{configStatus?.ozon.configured ? "configured" : "not configured"}</strong>
              <em className="badge neutral-badge">{configStatus?.ozon.source ?? "checking"}</em>
            </div>
            <div className="status-item">
              <span>Client ID</span>
              <strong>{configStatus?.ozon.client_id ?? "未保存"}</strong>
            </div>
            <div className="status-item">
              <span>API key fingerprint</span>
              <strong>{configStatus?.ozon.api_key_fingerprint ?? "未保存"}</strong>
            </div>
          </div>
          {configStatus?.ozon.issue && <p className="notice warn-text">{configStatus.ozon.issue}</p>}
          {validation && (
            <p className="notice">
              {validation.checked_at} · {validation.message}
            </p>
          )}
        </div>
      </section>

      <section className="workspace-grid">
        <div className="panel read-panel">
          <div className="section-title">
            <DatabaseZap />
            <div>
              <h2>真实商品读取</h2>
              <p>真实模式直接调用 Ozon Seller API；未配置凭据时失败关闭。</p>
            </div>
          </div>
          <button onClick={loadProducts}>读取 Ozon 商品</button>
          <div className="metric-row">
            <span>Product count</span>
            <strong>{productCount ?? "未读取"}</strong>
          </div>
          <div className="read-meta">
            <span className={productListMeta?.connector_mode === "real" ? "badge ok-badge" : "badge neutral-badge"}>
              {productListMeta?.connector_mode === "real" ? "real seller data" : "not loaded"}
            </span>
            {productListMeta?.last_id && <code>next: {productListMeta.last_id}</code>}
          </div>
          <div className="product-list">
            {products.map((product) => (
              <div key={product.product_id}>
                <strong>{product.offer_id}</strong>
                <span>{product.name ?? `Product ${product.product_id}`}</span>
                <em>
                  {(product.visibility ?? "visibility n/a")} · FBO {product.has_fbo_stocks ? "yes" : "no"} · FBS{" "}
                  {product.has_fbs_stocks ? "yes" : "no"}
                  {product.archived ? " · archived" : ""}
                </em>
              </div>
            ))}
          </div>
        </div>

        <div className="panel schedule-panel">
          <div className="section-title">
            <Repeat2 />
            <div>
              <h2>只读定时采集</h2>
              <p>按固定间隔调用官方 Ozon read-only API；OpenClaw 只能提议，不能启用。</p>
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
              title="采集间隔秒数"
            />
            <input
              min={1}
              max={100}
              type="number"
              value={scheduleLimit}
              onChange={(event) => setScheduleLimit(Number(event.target.value))}
              title="每次读取样本数量"
            />
          </div>
          <div className="inline-actions">
            <button onClick={() => configureSchedule(true)}>
              <Play size={18} /> 启用
            </button>
            <button onClick={() => configureSchedule(false)}>
              <PauseCircle size={18} /> 停止
            </button>
            <button onClick={runScheduleNow}>
              <RefreshCcw size={18} /> 立即采集
            </button>
          </div>
          <div className="status-list schedule-status">
            <div className="status-item">
              <span>Status</span>
              <strong>{schedule?.enabled ? "enabled" : "paused"}</strong>
              <em className={schedule?.enabled ? "badge ok-badge" : "badge neutral-badge"}>
                {schedule?.connector_mode ?? "mock"}
              </em>
            </div>
            <div className="status-item">
              <span>Last count</span>
              <strong>{schedule?.last_run?.product_count ?? "未运行"}</strong>
            </div>
            <div className="status-item">
              <span>Last sample</span>
              <strong>{schedule?.last_run?.sample_size ?? "未运行"}</strong>
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
              <h2>提案、审批、执行</h2>
              <p>OpenClaw 只创建 dry-run 提案，本地操作员审批后才进入执行队列。</p>
            </div>
          </div>
          <div className="task-command">
            <select value={selectedOperation} onChange={(event) => setSelectedOperation(event.target.value)}>
              <option value="ozon_update_price_mock">改价提案</option>
              <option value="ozon_update_inventory_mock">改库存提案</option>
              <option value="ozon_join_promotion_mock">参加促销提案</option>
              <option value="draft_upload_mock">草稿上传预演</option>
              <option value="import1688_mock">1688 导入预演</option>
            </select>
            <button onClick={createDryRun}>创建 dry-run</button>
          </div>
          <div className="task-list">
            {tasks.length === 0 && <p className="empty">还没有任务。先创建一个 dry-run。</p>}
            {tasks.map((task) => (
              <article key={task.id}>
                <div className="task-copy">
                  <span>{task.operation}</span>
                  <strong>{task.dry_run.summary}</strong>
                  <p>{task.dry_run.warnings.join(" · ") || "No warnings"}</p>
                  {task.receipt && <code>{task.receipt.result_summary}</code>}
                </div>
                <div className="task-actions">
                  <em className={`state ${task.state}`}>{task.state}</em>
                  <em>{task.risk}</em>
                  {task.state === "pending_approval" && <button onClick={() => approve(task.id)}>审批</button>}
                  {task.state === "queued" && <button onClick={() => execute(task.id)}>执行 dry-run</button>}
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

createRoot(document.getElementById("root")!).render(<App />);
