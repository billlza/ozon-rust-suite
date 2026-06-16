export async function callOzonLocalTool(config, tool, params) {
  const target = new URL(tool.path, config.base_url);
  const response = await fetch(target, {
    method: tool.method,
    headers: {
      "content-type": "application/json",
      [config.auth_header]: config.auth_token
    },
    body: tool.method === "GET" ? undefined : JSON.stringify(params ?? {})
  });
  const text = await response.text();
  let details = null;
  try {
    details = JSON.parse(text);
  } catch {}
  if (!response.ok) {
    const message = (details?.error ?? text) || `HTTP ${response.status}`;
    throw new Error(`Ozon Local rejected ${tool.name}: ${message}`);
  }
  return details ?? { text };
}

export async function claimOzonLocalPairing(claimUrl, code, origin) {
  const response = await fetch(claimUrl, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      origin
    },
    body: JSON.stringify({ code })
  });
  const text = await response.text();
  let details = null;
  try {
    details = JSON.parse(text);
  } catch {}
  if (!response.ok) {
    const message = (details?.error ?? text) || `HTTP ${response.status}`;
    throw new Error(`Ozon Local rejected pairing claim: ${message}`);
  }
  if (!details || typeof details !== "object") {
    throw new Error("pairing claim response must be JSON");
  }
  return details;
}
