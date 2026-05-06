import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { BadgeCheck, ClipboardCheck, KeyRound, ScrollText } from "lucide-react";
import "./styles.css";

const API_BASE = import.meta.env.VITE_CLOUD_API ?? "http://127.0.0.1:8080";
const ADMIN_TOKEN_STORAGE_KEY = "ozon-rust-suite.admin.token";

function App() {
  const [adminToken, setAdminToken] = useState(() => localStorage.getItem(ADMIN_TOKEN_STORAGE_KEY) ?? "");
  const [orderId, setOrderId] = useState("");
  const [paymentReference, setPaymentReference] = useState("");
  const [confirmMode, setConfirmMode] = useState<"id" | "reference">("id");
  const [generatedKey, setGeneratedKey] = useState("");
  const [audit, setAudit] = useState<string[]>([]);
  const [message, setMessage] = useState("等待管理员操作");

  async function createKeys() {
    rememberAdminToken(adminToken);
    const response = await fetch(`${API_BASE}/admin/card-keys`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        "x-admin-token": adminToken
      },
      body: JSON.stringify({ count: 1, plan_code: "standard_30d", duration_days: 30, max_devices: 1 })
    });
    const data = await response.json();
    if (!response.ok) {
      setMessage(`生成失败：${data.error}`);
      return;
    }
    setGeneratedKey(data.card_keys[0]);
    setMessage("已生成一张可兑换卡密");
  }

  async function confirmOrder() {
    rememberAdminToken(adminToken);
    const target = confirmMode === "id" ? orderId.trim() : paymentReference.trim();
    if (!target) {
      setMessage(confirmMode === "id" ? "请填写订单 UUID" : "请填写支付备注");
      return;
    }
    const path =
      confirmMode === "id"
        ? `/admin/orders/${encodeURIComponent(target)}/confirm`
        : `/admin/orders/by-reference/${encodeURIComponent(target)}/confirm`;
    const response = await fetch(`${API_BASE}${path}`, {
      method: "POST",
      headers: { "x-admin-token": adminToken }
    });
    const data = await response.json();
    if (!response.ok) {
      setMessage(`确认失败：${data.error}`);
      return;
    }
    setGeneratedKey(data.card_key);
    setMessage("订单已确认，卡密已生成");
  }

  async function loadAudit() {
    rememberAdminToken(adminToken);
    const response = await fetch(`${API_BASE}/audit`, {
      headers: { "x-admin-token": adminToken }
    });
    const data = await response.json();
    if (!response.ok) {
      setMessage(`审计读取失败：${data.error}`);
      return;
    }
    setAudit(data.map((item: any) => `${item.created_at} · ${item.action} · ${item.summary}`));
  }

  return (
    <main>
      <aside>
        <strong>Ozon Rust Admin</strong>
        <span>Orders</span>
        <span>Card keys</span>
        <span>Devices</span>
        <span>Audit</span>
      </aside>
      <section className="surface">
        <header>
          <div>
            <p>Operator console</p>
            <h1>商业后台最小闭环</h1>
          </div>
          <BadgeCheck />
        </header>

        <div className="ops">
          <label>
            管理员 Token
            <input
              value={adminToken}
              onChange={(event) => setAdminToken(event.target.value)}
              placeholder="输入部署环境配置的管理员 Token"
              type="password"
            />
          </label>
          <label>
            订单 UUID
            <input value={orderId} onChange={(event) => setOrderId(event.target.value)} placeholder="POST /orders 返回的 id" />
          </label>
          <label>
            支付备注
            <input value={paymentReference} onChange={(event) => setPaymentReference(event.target.value)} placeholder="OZON-..." />
          </label>
          <label>
            确认方式
            <select value={confirmMode} onChange={(event) => setConfirmMode(event.target.value as "id" | "reference")}>
              <option value="id">按订单 UUID</option>
              <option value="reference">按支付备注</option>
            </select>
          </label>
        </div>

        <div className="actions">
          <button onClick={confirmOrder}>
            <ClipboardCheck size={18} /> 确认订单并发卡
          </button>
          <button onClick={createKeys}>
            <KeyRound size={18} /> 直接生成卡密
          </button>
          <button onClick={loadAudit}>
            <ScrollText size={18} /> 读取审计
          </button>
        </div>

        <div className="output">
          <span>状态</span>
          <strong>{message}</strong>
          <code>{generatedKey || "卡密只在生成响应中出现一次"}</code>
        </div>

        <div className="audit">
          {audit.map((line) => (
            <p key={line}>{line}</p>
          ))}
        </div>
      </section>
    </main>
  );
}

function rememberAdminToken(token: string) {
  const trimmed = token.trim();
  if (trimmed) {
    localStorage.setItem(ADMIN_TOKEN_STORAGE_KEY, trimmed);
  } else {
    localStorage.removeItem(ADMIN_TOKEN_STORAGE_KEY);
  }
}

createRoot(document.getElementById("root")!).render(<App />);
