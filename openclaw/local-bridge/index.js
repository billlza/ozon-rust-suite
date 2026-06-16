import { definePluginEntry } from "openclaw/plugin-sdk/plugin-entry";
import { loadConfig, saveConfig } from "./src/config-store.js";
import { callOzonLocalTool, claimOzonLocalPairing } from "./src/local-http.js";

const IMPORT_PATH = "/openclaw/import";
const CLAIM_PATH = "/openclaw/import/claim";
const MAX_BODY_BYTES = 64 * 1024;
const IMPORT_NONCE_TTL_MS = 5 * 60 * 1000;
const LOOKUP_FIELDS = ["offer_id", "product_id", "sku"];
const importNonces = new Map();

const tools = [
  {
    name: "ozon_products_count",
    label: "Ozon Products Count",
    description: "Count real Ozon products through the paired Ozon Local node.",
    method: "POST",
    path: "/tools/ozon.products.count",
    parameters: objectSchema({})
  },
  {
    name: "ozon_products_list",
    label: "Ozon Products List",
    description: "List real Ozon product summaries through the paired Ozon Local node.",
    method: "POST",
    path: "/tools/ozon.products.list",
    parameters: objectSchema({
      limit: { type: "number", minimum: 1, maximum: 100 },
      last_id: { type: "string" },
      visibility: { type: "string" },
      include_archived_if_empty: { type: "boolean" }
    })
  },
  {
    name: "ozon_products_get",
    label: "Ozon Product Details",
    description: "Read one real Ozon product detail package and image URLs through Ozon Local. Pass one identifier; if a list item provides both product_id and offer_id, offer_id is preferred and the other id is verified against the returned product.",
    method: "POST",
    path: "/tools/ozon.products.get",
    parameters: objectSchema({
      product_id: { type: "string", description: "Ozon product_id. Use this only when offer_id is unavailable." },
      offer_id: { type: "string", description: "Seller offer_id from ozon_products_list. Preferred identifier for follow-up reads." },
      sku: { type: "string", description: "Ozon SKU. Use this only when product_id and offer_id are unavailable." }
    })
  },
  {
    name: "ozon_poster_handoff",
    label: "Ozon Poster Handoff",
    description: "Build a product-grounded poster prompt package from real Ozon facts and images. Pass one identifier; if a list item provides both product_id and offer_id, offer_id is preferred and the other id is verified against the returned product.",
    method: "POST",
    path: "/poster/handoff",
    parameters: objectSchema({
      product_id: { type: "string", description: "Ozon product_id. Use this only when offer_id is unavailable." },
      offer_id: { type: "string", description: "Seller offer_id from ozon_products_list. Preferred identifier for poster handoff." },
      sku: { type: "string", description: "Ozon SKU. Use this only when product_id and offer_id are unavailable." },
      theme: { type: "string" },
      locale: { type: "string" }
    })
  }
];

function objectSchema(properties) {
  return {
    type: "object",
    additionalProperties: false,
    properties
  };
}

function respond(res, status, body, contentType = "text/plain; charset=utf-8") {
  res.statusCode = status;
  res.setHeader("content-type", contentType);
  res.setHeader("cache-control", "no-store");
  res.end(body);
  return true;
}

function respondJson(res, status, body) {
  return respond(res, status, JSON.stringify(body), "application/json; charset=utf-8");
}

function parseRequestUrl(rawUrl) {
  try {
    return new URL(rawUrl ?? "/", "http://127.0.0.1:18789");
  } catch {
    return null;
  }
}

async function readBody(req) {
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > MAX_BODY_BYTES) {
      throw new Error("request body too large");
    }
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf8");
}

function normalizeClaim(raw) {
  if (!raw || typeof raw !== "object") {
    throw new Error("claim response must be an object");
  }
  const manifestUrl = requireLocalUrl(raw.manifest_url, "manifest_url");
  const baseUrl = requireLocalUrl(raw.base_url, "base_url");
  const authHeader = requireString(raw.auth_header, "auth_header");
  const authToken = requireString(raw.auth_token, "auth_token");
  if (!/^[-A-Za-z0-9_]+$/.test(authHeader)) {
    throw new Error("auth_header contains unsupported characters");
  }
  return {
    status: "paired",
    paired_at: typeof raw.paired_at === "string" ? raw.paired_at : new Date().toISOString(),
    manifest_url: manifestUrl,
    base_url: baseUrl,
    auth_header: authHeader,
    auth_token: authToken,
    auth_token_fingerprint: typeof raw.auth_token_fingerprint === "string" ? raw.auth_token_fingerprint : null,
    expires_at: typeof raw.expires_at === "string" ? raw.expires_at : null,
    safety_rules: Array.isArray(raw.safety_rules) ? raw.safety_rules.filter((rule) => typeof rule === "string") : []
  };
}

function requireString(value, name) {
  if (typeof value !== "string" || !value.trim()) {
    throw new Error(`${name} is required`);
  }
  return value.trim();
}

function requireLocalUrl(value, name) {
  const text = requireString(value, name);
  const parsed = new URL(text);
  const localHost = parsed.hostname === "127.0.0.1" || parsed.hostname === "localhost";
  if (parsed.protocol !== "http:" || !localHost || parsed.username || parsed.password) {
    throw new Error(`${name} must be a local http URL`);
  }
  return parsed.toString().replace(/\/$/, "");
}

function requireLocalEndpointUrl(value, name, pathname) {
  const text = requireLocalUrl(value, name);
  const parsed = new URL(text);
  if (parsed.pathname !== pathname) {
    throw new Error(`${name} must point to ${pathname}`);
  }
  return text;
}

function requestLocalOrigin(req) {
  const host = String(req.headers.host ?? "");
  const parsed = new URL(`http://${host}`);
  const localHost = parsed.hostname === "127.0.0.1" || parsed.hostname === "localhost";
  if (!localHost || parsed.port !== "18789" || parsed.username || parsed.password) {
    throw new Error("OpenClaw gateway must be served from local port 18789");
  }
  return parsed.origin;
}

function issueImportNonce() {
  const now = Date.now();
  for (const [nonce, expiresAt] of importNonces.entries()) {
    if (expiresAt <= now) importNonces.delete(nonce);
  }
  const nonce = crypto.randomUUID();
  importNonces.set(nonce, now + IMPORT_NONCE_TTL_MS);
  return nonce;
}

function consumeImportNonce(nonce) {
  if (typeof nonce !== "string" || !nonce) return false;
  const expiresAt = importNonces.get(nonce);
  importNonces.delete(nonce);
  return typeof expiresAt === "number" && expiresAt > Date.now();
}

function importPageHtml(nonce) {
  return `<!doctype html>
<html lang="zh-CN">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Ozon66 本机绑定</title>
<style>
body{margin:0;font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;background:#f6faf7;color:#13231b}
main{max-width:760px;margin:8vh auto;padding:32px;border:1px solid #bfe4d2;border-radius:18px;background:#fff;box-shadow:0 16px 60px rgba(29,72,50,.12)}
.eyebrow{color:#138760;font-weight:800;letter-spacing:.04em;text-transform:uppercase}
h1{font-size:38px;margin:12px 0 10px}
p{font-size:18px;line-height:1.6;color:#4b5d53}
.status{margin-top:24px;padding:18px 20px;border-radius:14px;background:#eefaf4;border:1px solid #bfe4d2;font-weight:800}
.error{background:#fff4f2;border-color:#f0b4aa;color:#8d281f}
</style>
<main>
  <div class="eyebrow">Ozon Local</div>
  <h1>正在绑定这台电脑</h1>
  <p>本页只接收 5 分钟有效的一次性配对码。长期访问令牌不会出现在网址里，也不会返回给浏览器页面。</p>
  <div id="status" class="status">正在读取配对码...</div>
</main>
<script>
const statusEl = document.getElementById("status");
const saveNonce = ${JSON.stringify(nonce)};
function setStatus(text, error) {
  statusEl.textContent = text;
  statusEl.className = error ? "status error" : "status";
}
function requireLocalEndpoint(value, pathname, label) {
  const parsed = new URL(value);
  const localHost = parsed.hostname === "127.0.0.1" || parsed.hostname === "localhost";
  if (parsed.protocol !== "http:" || !localHost || parsed.username || parsed.password || parsed.pathname !== pathname) {
    throw new Error(label + " 不是本机授权地址，请回到 Ozon Local 重新点击绑定。");
  }
  return parsed.toString().replace(/\\/$/, "");
}
async function run() {
  const fragment = new URLSearchParams(location.hash.slice(1));
  const code = fragment.get("ozon66_pairing_code");
  const claimUrl = requireLocalEndpoint(fragment.get("claim_url") || "", "/openclaw/pairing/claim", "claim_url");
  const manifestUrl = requireLocalEndpoint(fragment.get("manifest_url") || "", "/openclaw/manifest", "manifest_url");
  if (!code || !claimUrl || !manifestUrl) throw new Error("缺少配对参数，请回到 Ozon Local 重新点击绑定。");
  history.replaceState(null, "", location.pathname);
  setStatus("正在向 OpenClaw 本地服务确认授权...");
  const claimResponse = await fetch("${CLAIM_PATH}", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ nonce: saveNonce, code, claim_url: claimUrl, manifest_url: manifestUrl }),
    credentials: "omit"
  });
  if (!claimResponse.ok) throw new Error("绑定失败。配对码可能已失效，或本机节点拒绝授权，请重新点击绑定。");
  setStatus("绑定完成。现在可以在 OpenClaw/Codex 里读取 Ozon 商品并生成海报。");
}
run().catch((error) => setStatus(error instanceof Error ? error.message : String(error), true));
</script>`;
}

async function handleImportRoute(req, res) {
  const parsed = parseRequestUrl(req.url);
  if (!parsed) return false;
  if (parsed.pathname === IMPORT_PATH) {
    if (req.method !== "GET" && req.method !== "HEAD") {
      return respond(res, 405, "Method not allowed");
    }
    return respond(res, 200, req.method === "HEAD" ? "" : importPageHtml(issueImportNonce()), "text/html; charset=utf-8");
  }
  if (parsed.pathname === CLAIM_PATH) {
    if (req.method !== "POST") {
      return respond(res, 405, "Method not allowed");
    }
    try {
      const payload = JSON.parse(await readBody(req));
      if (!consumeImportNonce(payload.nonce)) {
        throw new Error("binding page nonce expired; reopen the binding link");
      }
      const manifestUrl = requireLocalEndpointUrl(payload.manifest_url, "manifest_url", "/openclaw/manifest");
      const claimUrl = requireLocalEndpointUrl(payload.claim_url, "claim_url", "/openclaw/pairing/claim");
      const code = requireString(payload.code, "ozon66_pairing_code");
      const claim = normalizeClaim(await claimOzonLocalPairing(claimUrl, code, requestLocalOrigin(req)));
      if (claim.manifest_url !== manifestUrl) {
        throw new Error("claim manifest_url does not match the binding link");
      }
      if (new URL(claim.base_url).origin !== new URL(claimUrl).origin) {
        throw new Error("claim base_url does not match the binding link");
      }
      await saveConfig(claim);
      return respondJson(res, 200, {
        ok: true,
        manifest_url: claim.manifest_url,
        base_url: claim.base_url,
        auth_header: claim.auth_header,
        auth_token_fingerprint: claim.auth_token_fingerprint
      });
    } catch (error) {
      return respondJson(res, 400, { ok: false, error: error instanceof Error ? error.message : String(error) });
    }
  }
  return false;
}

function createOzonTool(tool) {
  return {
    name: tool.name,
    label: tool.label,
    description: tool.description,
    parameters: tool.parameters,
    async execute(_toolCallId, params) {
      const config = await loadConfig();
      const prepared = prepareToolParams(tool, params);
      const result = await callOzonLocalTool(config, tool, prepared.params);
      verifyLookupMatchesResult(tool, prepared.lookup, result);
      return {
        content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
        details: result
      };
    }
  };
}

function prepareToolParams(tool, params) {
  const original = params && typeof params === "object" ? params : {};
  if (tool.name !== "ozon_products_get" && tool.name !== "ozon_poster_handoff") {
    return { params: original, lookup: {} };
  }
  const lookup = {};
  const next = { ...original };
  for (const field of LOOKUP_FIELDS) {
    const value = typeof original[field] === "string" ? original[field].trim() : "";
    if (value) {
      lookup[field] = value;
      next[field] = value;
    } else {
      delete next[field];
    }
  }
  const selectedField = LOOKUP_FIELDS.find((field) => lookup[field]);
  if (!selectedField) {
    return { params: next, lookup };
  }
  for (const field of LOOKUP_FIELDS) {
    if (field !== selectedField) delete next[field];
  }
  return { params: next, lookup };
}

function verifyLookupMatchesResult(tool, lookup, result) {
  if (tool.name !== "ozon_products_get" && tool.name !== "ozon_poster_handoff") return;
  const product = result?.product ?? result;
  if (!product || typeof product !== "object") return;
  for (const field of LOOKUP_FIELDS) {
    if (!lookup[field] || product[field] == null) continue;
    if (String(product[field]) !== lookup[field]) {
      throw new Error(`Ozon returned ${field}=${product[field]}, which does not match the requested ${field}=${lookup[field]}`);
    }
  }
}

export default definePluginEntry({
  id: "ozon66-local-bridge",
  name: "Ozon66 Local Bridge",
  description: "One-click local bridge for Ozon Local product reads and poster handoff.",
  register(api) {
    api.registerHttpRoute({
      path: IMPORT_PATH,
      auth: "plugin",
      match: "exact",
      handler: handleImportRoute
    });
    api.registerHttpRoute({
      path: CLAIM_PATH,
      auth: "plugin",
      match: "exact",
      handler: handleImportRoute
    });
    for (const tool of tools) {
      api.registerTool(() => createOzonTool(tool), { name: tool.name });
    }
  }
});
