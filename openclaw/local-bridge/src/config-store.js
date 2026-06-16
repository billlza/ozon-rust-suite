import fs from "node:fs/promises";
import path from "node:path";
import os from "node:os";

const CONFIG_PATH = path.join(os.homedir(), ".openclaw", "ozon66-local-bridge.json");

export async function saveConfig(config) {
  await fs.mkdir(path.dirname(CONFIG_PATH), { recursive: true, mode: 0o700 });
  const tmpPath = `${CONFIG_PATH}.${process.pid}.tmp`;
  await fs.writeFile(tmpPath, `${JSON.stringify(config, null, 2)}\n`, { mode: 0o600 });
  await fs.rename(tmpPath, CONFIG_PATH);
}

export async function loadConfig() {
  const raw = await fs.readFile(CONFIG_PATH, "utf8");
  return JSON.parse(raw);
}
