"use client";

import { FormEvent, useEffect, useMemo, useState } from "react";

import { Notify, cx, responseMessage } from "./shared";

type UseCaseKey = "kanban_chat" | "comment_review" | "article_assistant";
type UseCaseConfig = {
  enabled: boolean;
  system_prompt: string;
  connection_id: number | null;
  model: string;
};
type ModelOption = { id: string; name: string };
type ConnectionStatus = {
  state: "untested" | "online" | "error";
  tested_at: string | null;
  latency_ms: number | null;
  error: string | null;
};
type LlmConnection = {
  id: number;
  display_name: string;
  base_url: string;
  // Compatibility fields are preserved when saving older records. Model
  // selection is owned by UseCaseConfig and is not edited on a credential.
  model: string;
  api_key_configured: boolean;
  temperature: number;
  max_tokens: number;
  enabled: boolean;
  status: ConnectionStatus;
  updated_at: string;
};
type LlmSettingsPayload = {
  revision: number;
  connections: LlmConnection[];
  display_name: string;
  base_url: string;
  model: string;
  api_key_configured: boolean;
  temperature: number;
  max_tokens: number;
  enabled: boolean;
  use_cases: Record<UseCaseKey, UseCaseConfig>;
  status: ConnectionStatus;
  updated_at: string;
};
type NewKeyDraft = {
  display_name: string;
  base_url: string;
  api_key: string;
};

const EMPTY_STATUS: ConnectionStatus = {
  state: "untested",
  tested_at: null,
  latency_ms: null,
  error: null,
};

const EMPTY_SETTINGS: LlmSettingsPayload = {
  revision: 1,
  connections: [],
  display_name: "",
  base_url: "",
  model: "",
  api_key_configured: false,
  temperature: 0.7,
  max_tokens: 512,
  enabled: false,
  use_cases: {
    kanban_chat: { enabled: true, system_prompt: "", connection_id: null, model: "" },
    comment_review: { enabled: false, system_prompt: "", connection_id: null, model: "" },
    article_assistant: { enabled: false, system_prompt: "", connection_id: null, model: "" },
  },
  status: EMPTY_STATUS,
  updated_at: "",
};

const USE_CASES: Array<{
  id: UseCaseKey;
  label: string;
  source: string;
}> = [
  { id: "kanban_chat", label: "看板娘对话", source: "灵衣 / 看板娘" },
  { id: "comment_review", label: "评论预审", source: "评论审核" },
];
const USE_CASE_KEYS: UseCaseKey[] = ["kanban_chat", "comment_review", "article_assistant"];

function normalizePayload(payload: LlmSettingsPayload): LlmSettingsPayload {
  return {
    ...payload,
    connections: payload.connections ?? [],
    use_cases: Object.fromEntries(
      USE_CASE_KEYS.map((id) => [
        id,
        {
          enabled: payload.use_cases?.[id]?.enabled ?? false,
          system_prompt: payload.use_cases?.[id]?.system_prompt ?? "",
          connection_id: payload.use_cases?.[id]?.connection_id ?? null,
          model: payload.use_cases?.[id]?.model ?? "",
        },
      ]),
    ) as Record<UseCaseKey, UseCaseConfig>,
  };
}

function connectionKey(id: number) {
  return String(id);
}

function statusLabel(status: ConnectionStatus) {
  if (status.state === "online") return "已验证";
  if (status.state === "error") return "验证失败";
  return "待验证";
}

export function LlmSettings({ notify }: { notify: Notify }) {
  const [settings, setSettings] = useState<LlmSettingsPayload>(EMPTY_SETTINGS);
  const [newKey, setNewKey] = useState<NewKeyDraft | null>(null);
  const [modelOptions, setModelOptions] = useState<Record<string, ModelOption[]>>({});
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [creating, setCreating] = useState(false);
  const [testing, setTesting] = useState<number | null>(null);
  const [fetchingModels, setFetchingModels] = useState<number | null>(null);
  const [dirty, setDirty] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [testReplies, setTestReplies] = useState<Record<string, string>>({});
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    const controller = new AbortController();
    void fetch("/api/v1/admin/llm", { credentials: "include", signal: controller.signal })
      .then(async (response) => {
        if (!response.ok) throw new Error(await responseMessage(response, "无法读取 LLM 配置"));
        return response.json() as Promise<LlmSettingsPayload>;
      })
      .then((payload) => {
        setSettings(normalizePayload(payload));
        setModelOptions({});
        setDirty(false);
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted) return;
        setLoadError(error instanceof Error ? error.message : "无法读取 LLM 配置");
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, [reloadKey]);

  const updateConnectionEnabled = (id: number, enabled: boolean) => {
    setSettings((current) => ({
      ...current,
      connections: current.connections.map((connection) =>
        connection.id === id ? { ...connection, enabled } : connection,
      ),
    }));
    setDirty(true);
  };

  const startAddingKey = () => {
    if (dirty) {
      notify("请先保存当前场景配置，再新增 Key");
      return;
    }
    setNewKey({
      display_name: `LLM Key ${settings.connections.length + 1}`,
      base_url: "https://api.openai.com/v1",
      api_key: "",
    });
  };

  const removeConnection = (id: number) => {
    setSettings((current) => ({
      ...current,
      connections: current.connections.filter((connection) => connection.id !== id),
      use_cases: Object.fromEntries(
        USE_CASE_KEYS.map((useCaseId) => [
          useCaseId,
          current.use_cases[useCaseId].connection_id === id
            ? { ...current.use_cases[useCaseId], connection_id: null, model: "", enabled: false }
            : current.use_cases[useCaseId],
        ]),
      ) as Record<UseCaseKey, UseCaseConfig>,
    }));
    setDirty(true);
  };

  const updateUseCase = <K extends keyof UseCaseConfig>(
    id: UseCaseKey,
    key: K,
    value: UseCaseConfig[K],
  ) => {
    setSettings((current) => ({
      ...current,
      use_cases: {
        ...current.use_cases,
        [id]: { ...current.use_cases[id], [key]: value },
      },
    }));
    setDirty(true);
  };

  const fetchModels = async (connection: LlmConnection, announce = true) => {
    setFetchingModels(connection.id);
    try {
      const response = await fetch("/api/v1/admin/llm/models", {
        method: "POST",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ connection_id: connection.id, base_url: connection.base_url }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "获取模型失败"));
      const payload = await response.json() as { items: ModelOption[] };
      setModelOptions((current) => ({ ...current, [connectionKey(connection.id)]: payload.items }));
      if (announce) {
        notify(
          payload.items.length ? `已获取 ${payload.items.length} 个模型` : "API 未返回可用模型",
          payload.items.length ? "success" : "danger",
        );
      }
      return payload.items;
    } catch (error) {
      notify(error instanceof Error ? error.message : "获取模型失败", "danger");
      return [];
    } finally {
      setFetchingModels(null);
    }
  };

  const selectUseCaseConnection = (id: UseCaseKey, rawValue: string) => {
    const connectionId = rawValue ? Number(rawValue) : null;
    const connection = connectionId === null
      ? undefined
      : settings.connections.find((item) => item.id === connectionId);
    setSettings((current) => ({
      ...current,
      use_cases: {
        ...current.use_cases,
        [id]: {
          ...current.use_cases[id],
          connection_id: connectionId,
          model: "",
        },
      },
    }));
    setDirty(true);
    if (connection) void fetchModels(connection, false);
  };

  const testAndSaveNewKey = async () => {
    if (!newKey) return;
    if (dirty) {
      notify("请先保存当前场景配置，再测试并保存 Key");
      return;
    }
    const displayName = newKey.display_name.trim();
    const baseUrl = newKey.base_url.trim();
    const apiKey = newKey.api_key.trim();
    if (!displayName || !baseUrl || !apiKey) {
      notify("请填写连接名称、API 地址和 API Key", "danger");
      return;
    }
    setCreating(true);
    try {
      const response = await fetch("/api/v1/admin/llm/connections", {
        method: "POST",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          revision: settings.revision,
          display_name: displayName,
          base_url: baseUrl,
          api_key: apiKey,
        }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "Key 测试失败，未保存"));
      setSettings(normalizePayload(await response.json() as LlmSettingsPayload));
      setNewKey(null);
      setDirty(false);
      notify("Key 测试通过并已保存", "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "Key 测试失败，未保存", "danger");
    } finally {
      setCreating(false);
    }
  };

  const save = async (event: FormEvent) => {
    event.preventDefault();
    setSaving(true);
    try {
      const response = await fetch("/api/v1/admin/llm", {
        method: "PUT",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          revision: settings.revision,
          connections: settings.connections.map((connection) => ({
            id: connection.id,
            display_name: connection.display_name,
            base_url: connection.base_url,
            model: connection.model,
            clear_api_key: false,
            temperature: connection.temperature,
            max_tokens: connection.max_tokens,
            enabled: connection.enabled,
          })),
          use_cases: settings.use_cases,
        }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "保存场景配置失败"));
      setSettings(normalizePayload(await response.json() as LlmSettingsPayload));
      setDirty(false);
      notify("场景配置已保存", "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "保存场景配置失败", "danger");
    } finally {
      setSaving(false);
    }
  };

  const testConnection = async (connectionId: number) => {
    setTesting(connectionId);
    setTestReplies((current) => ({ ...current, [connectionKey(connectionId)]: "" }));
    try {
      const response = await fetch("/api/v1/admin/llm/test", {
        method: "POST",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ connection_id: connectionId }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "LLM Key 验证失败"));
      const payload = await response.json() as { reply: string; latency_ms: number };
      setTestReplies((current) => ({ ...current, [connectionKey(connectionId)]: payload.reply }));
      setSettings((current) => ({
        ...current,
        connections: current.connections.map((connection) =>
          connection.id === connectionId
            ? {
                ...connection,
                status: {
                  state: "online",
                  tested_at: new Date().toISOString(),
                  latency_ms: payload.latency_ms,
                  error: null,
                },
              }
            : connection,
        ),
      }));
      notify(`Key 验证通过 · ${payload.latency_ms} ms`, "success");
    } catch (error) {
      const message = error instanceof Error ? error.message : "LLM Key 验证失败";
      setSettings((current) => ({
        ...current,
        connections: current.connections.map((connection) =>
          connection.id === connectionId
            ? { ...connection, status: { ...connection.status, state: "error", error: message } }
            : connection,
        ),
      }));
      notify(message, "danger");
    } finally {
      setTesting(null);
    }
  };

  const availableConnections = useMemo(
    () => settings.connections.filter((connection) => connection.enabled && connection.api_key_configured),
    [settings.connections],
  );

  if (loading) return <div className="empty-panel llm-loading" role="status">正在读取 LLM Key 配置…</div>;
  if (loadError) return <div className="empty-panel llm-loading"><p>{loadError}</p><button type="button" onClick={() => { setLoading(true); setLoadError(""); setReloadKey((key) => key + 1); }}>重新加载</button></div>;

  return (
    <form className="llm-settings" onSubmit={save}>
      <div className="admin-title">
        <div><h1>LLM</h1><p>KEY VAULT / USE CASE ROUTING</p></div>
        <button className="admin-primary" type="submit" disabled={saving || !dirty}>{saving ? "正在保存…" : "保存场景配置"}</button>
      </div>
      <section className="admin-panel llm-connection-panel">
        <header>
          <div><h2>已保存的 Key</h2><p>API CREDENTIALS</p></div>
          <div className="llm-connection-actions">
            <span className="llm-key-count">{settings.connections.length} SAVED</span>
            <button type="button" onClick={startAddingKey} disabled={newKey !== null}>＋ 新增 Key</button>
          </div>
        </header>
        <p className="llm-help">新增时只验证并保存连接凭据，不需要选择模型。模型由下方每个具体场景独立选择。</p>
        {newKey && <article className="llm-new-key-card">
          <div className="llm-connection-card-header"><div><code>NEW KEY</code><h3>测试通过后自动保存</h3></div></div>
          <div className="llm-form-grid">
            <label>连接名称<input value={newKey.display_name} maxLength={80} onChange={(event) => setNewKey((current) => current && { ...current, display_name: event.target.value })} placeholder="例如：OpenAI 主账号" /></label>
            <label>API 地址<input type="url" value={newKey.base_url} onChange={(event) => setNewKey((current) => current && { ...current, base_url: event.target.value })} placeholder="https://api.openai.com/v1" /></label>
            <label className="llm-wide">API Key<input type="password" autoComplete="new-password" value={newKey.api_key} onChange={(event) => setNewKey((current) => current && { ...current, api_key: event.target.value })} placeholder="填写待验证的 Key" /></label>
          </div>
          <footer><span>不会要求预先选择模型</span><div><button type="button" onClick={() => setNewKey(null)} disabled={creating}>取消</button><button className="admin-primary" type="button" onClick={() => void testAndSaveNewKey()} disabled={creating}>{creating ? "正在测试…" : "测试并保存"}</button></div></footer>
        </article>}
        <div className="llm-connections">
          {settings.connections.length === 0 && <div className="llm-empty-connections">还没有已保存的 Key，点击右上角新增。</div>}
          {settings.connections.map((connection) => {
            const reply = testReplies[connectionKey(connection.id)];
            return <article className={cx("llm-connection-card", connection.enabled && "enabled")} key={connection.id}>
              <div className="llm-connection-card-header">
                <div><code>KEY-{connection.id}</code><h3>{connection.display_name}</h3></div>
                <label className="toggle" aria-label={`启用${connection.display_name}`}><input type="checkbox" checked={connection.enabled} onChange={(event) => updateConnectionEnabled(connection.id, event.target.checked)} /><i /></label>
              </div>
              <dl className="llm-key-details">
                <div><dt>API 地址</dt><dd>{connection.base_url}</dd></div>
                <div><dt>API Key</dt><dd>{connection.api_key_configured ? "******** · 已加密保存" : "未保存"}</dd></div>
              </dl>
              <footer>
                <span className={cx("llm-status", connection.status.state)}>{statusLabel(connection.status)}</span>
                <div><button type="button" onClick={() => void testConnection(connection.id)} disabled={testing === connection.id}>{testing === connection.id ? "正在验证…" : "重新验证"}</button><button className="llm-danger-button" type="button" onClick={() => removeConnection(connection.id)}>删除</button></div>
              </footer>
              {(connection.status.error || reply) && <div className={cx("llm-test-result", connection.status.state)}><b>{connection.status.error ? "最近错误" : "验证结果"}</b><p>{connection.status.error || reply}</p>{connection.status.latency_ms !== null && <small>{connection.status.latency_ms} ms</small>}</div>}
            </article>;
          })}
        </div>
      </section>
      <section className="llm-use-cases">
        <header><div><h2>场景绑定</h2><p>USE CASE ROUTING</p></div><small>长期运行的场景在这里绑定；文章润色在撰写页按次选择</small></header>
        <div>
          {USE_CASES.map((item) => {
            const useCase = settings.use_cases[item.id];
            const connection = settings.connections.find((candidate) => candidate.id === useCase.connection_id);
            const options = connection ? (modelOptions[connectionKey(connection.id)] ?? []) : [];
            return <article className={cx("admin-panel", useCase.enabled && "enabled")} key={item.id}>
              <header><div><code>{item.id}</code><h3>{item.label}</h3><small>{item.source}</small></div><label className="toggle" aria-label={`启用${item.label}`}><input type="checkbox" checked={useCase.enabled} onChange={(event) => updateUseCase(item.id, "enabled", event.target.checked)} /><i /></label></header>
              <label>使用 Key<select value={useCase.connection_id ?? ""} onChange={(event) => selectUseCaseConnection(item.id, event.target.value)}><option value="">请选择已保存的 Key</option>{availableConnections.map((candidate) => <option value={candidate.id} key={candidate.id}>{candidate.display_name}</option>)}</select></label>
              <label>使用模型<span className="llm-model-row"><select value={useCase.model} disabled={!connection} onChange={(event) => updateUseCase(item.id, "model", event.target.value)}><option value="">{connection ? (fetchingModels === connection.id ? "正在获取模型…" : "请选择模型") : "先选择 Key"}</option>{useCase.model && !options.some((option) => option.id === useCase.model) && <option value={useCase.model}>{useCase.model}</option>}{options.map((option) => <option value={option.id} key={option.id}>{option.name === option.id ? option.id : `${option.name} · ${option.id}`}</option>)}</select><button type="button" onClick={() => connection && void fetchModels(connection)} disabled={!connection || fetchingModels === connection.id}>{fetchingModels === connection?.id ? "获取中…" : "刷新模型"}</button></span></label>
              <label>系统提示词<textarea value={useCase.system_prompt} maxLength={12000} onChange={(event) => updateUseCase(item.id, "system_prompt", event.target.value)} placeholder={`${item.label}的统一系统提示词`} /></label>
            </article>;
          })}
        </div>
      </section>
    </form>
  );
}
