"use client";

import Image from "next/image";
import Link from "next/link";
import { useCallback, useEffect, useMemo, useState } from "react";

import {
  AdminAsset,
  cx,
  Notify,
  Theme,
  ThemeTokens,
  responseMessage,
} from "./shared";

type AdminRaiment = {
  id: string;
  name: string;
  cover_asset_id: number;
  cover_asset: LinkedRaimentAsset;
  theme: ThemeTokens;
  enabled: boolean;
  sort_order: number;
  is_default: boolean;
  color_scheme: Theme;
  cover_title: string;
  cover_subtitle: string;
  cover_character_name: string;
  cover_dialogue: string;
  cover_voice_label: string;
  cover_voice_asset_id: number | null;
  cover_voice_asset: LinkedRaimentAsset | null;
  login_success_voice_asset_id: number | null;
  login_success_voice_asset: LinkedRaimentAsset | null;
  kanban_asset_id: number | null;
  is_builtin: boolean;
  revision: number;
  created_at: string;
  updated_at: string;
};

type LinkedRaimentAsset = Pick<AdminAsset, "id" | "name" | "media_type" | "file">;

type AdminRaimentPayload = {
  items: AdminRaiment[];
};

type AssetPayload = {
  items: AdminAsset[];
  page: number;
  per_page: number;
  total: number;
};

type CreateDraft = {
  name: string;
  enabled: boolean;
  is_default: boolean;
  sort_order: number;
};

const inferColorScheme = (background: string, fallback: Theme): Theme => {
  const match = /^#([0-9a-f]{6})$/i.exec(background.trim());
  if (!match) return fallback;
  const value = Number.parseInt(match[1], 16);
  const red = (value >> 16) & 0xff;
  const green = (value >> 8) & 0xff;
  const blue = value & 0xff;
  return (red * 299 + green * 587 + blue * 114) / 1000 < 140 ? "night" : "day";
};

const clonePayload = (payload: AdminRaimentPayload): AdminRaimentPayload => ({
  items: payload.items.map((item) => ({ ...item, theme: { ...item.theme } })),
});

const sortRaiments = (items: AdminRaiment[]) =>
  [...items].sort((left, right) => left.sort_order - right.sort_order || left.created_at.localeCompare(right.created_at) || left.id.localeCompare(right.id));

const dialogueItems = (value: string) => {
  const items = value.split(/\r?\n/);
  return items.length ? items : [""];
};

const fetchAllAssets = async (
  usableFor: "raiment_cover" | "raiment_voice",
  signal?: AbortSignal,
) => {
  const items: AdminAsset[] = [];
  for (let page = 1; ; page += 1) {
    const params = new URLSearchParams({
      page: String(page),
      per_page: "100",
      sort: "uploaded_at",
      order: "desc",
      usable_for: usableFor,
    });
    const response = await fetch(`/api/v1/admin/assets?${params}`, {
      credentials: "include",
      signal,
    });
    if (!response.ok) {
      throw new Error(await responseMessage(response, usableFor === "raiment_cover" ? "封面素材加载失败" : "语音素材加载失败"));
    }
    const payload = await response.json() as AssetPayload;
    items.push(...payload.items);
    if (items.length >= payload.total || payload.items.length < payload.per_page) return items;
  }
};

export function RaimentSettings({ notify }: { notify: Notify }) {
  const [data, setData] = useState<AdminRaimentPayload | null>(null);
  const [assets, setAssets] = useState<AdminAsset[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState<"save" | "create" | "delete" | null>(null);
  const [assetPickerOpen, setAssetPickerOpen] = useState<"cover" | "voice" | "successVoice" | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [createDraft, setCreateDraft] = useState<CreateDraft>({
    name: "新灵衣",
    enabled: true,
    is_default: false,
    sort_order: 0,
  });

  const load = useCallback(async (signal?: AbortSignal) => {
    setLoading(true);
    try {
      const [raimentResponse, coverAssets, voiceAssets] = await Promise.all([
        fetch("/api/v1/admin/raiments", { credentials: "include", signal }),
        fetchAllAssets("raiment_cover", signal),
        fetchAllAssets("raiment_voice", signal),
      ]);
      if (!raimentResponse.ok) {
        throw new Error(await responseMessage(raimentResponse, "灵衣配置加载失败"));
      }
      const raimentPayload = await raimentResponse.json() as AdminRaimentPayload;
      const cloned = clonePayload(raimentPayload);
      setData({ items: sortRaiments(cloned.items) });
      setSelectedId((current) => cloned.items.some((item) => item.id === current)
        ? current
        : cloned.items[0]?.id || "");
      setAssets([...coverAssets, ...voiceAssets]);
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") return;
      notify(error instanceof Error ? error.message : "灵衣配置加载失败", "danger");
    } finally {
      if (!signal?.aborted) setLoading(false);
    }
  }, [notify]);

  useEffect(() => {
    const controller = new AbortController();
    const timer = window.setTimeout(() => void load(controller.signal), 0);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [load]);

  const selected = useMemo(
    () => data?.items.find((item) => item.id === selectedId) || data?.items[0] || null,
    [data, selectedId],
  );
  const pickerMediaType = assetPickerOpen === "cover" ? "image" : "audio";
  const pickerAssetId = assetPickerOpen === "cover"
    ? selected?.cover_asset_id
    : assetPickerOpen === "successVoice"
      ? selected?.login_success_voice_asset_id
      : selected?.cover_voice_asset_id;

  const updateSelected = (patch: Partial<AdminRaiment>) => {
    if (!selected || !data) return;
    setData({
      items: data.items.map((item) => item.id === selected.id ? { ...item, ...patch } : item),
    });
  };

  const updateDialogue = (index: number, value: string) => {
    if (!selected) return;
    const items = dialogueItems(selected.cover_dialogue);
    items[index] = value.replace(/[\r\n]+/g, " ");
    updateSelected({ cover_dialogue: items.join("\n") });
  };

  const addDialogue = () => {
    if (!selected) return;
    const items = dialogueItems(selected.cover_dialogue);
    if (items.length >= 20) {
      notify("每套灵衣最多添加 20 条封面对话", "danger");
      return;
    }
    updateSelected({ cover_dialogue: [...items, ""].join("\n") });
  };

  const removeDialogue = (index: number) => {
    if (!selected) return;
    const items = dialogueItems(selected.cover_dialogue).filter((_, itemIndex) => itemIndex !== index);
    updateSelected({ cover_dialogue: (items.length ? items : [""]).join("\n") });
  };

  const updateColor = (key: keyof ThemeTokens, value: string) => {
    if (!selected) return;
    const nextTheme = { ...selected.theme, [key]: value };
    updateSelected({
      theme: nextTheme,
      color_scheme: key === "background"
        ? inferColorScheme(value, selected.color_scheme)
        : selected.color_scheme,
    });
  };

  const saveRaiment = async () => {
    if (!selected || saving) return;
    setSaving("save");
    try {
      const response = await fetch(`/api/v1/admin/raiments/${encodeURIComponent(selected.id)}`, {
        method: "PUT",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          revision: selected.revision,
          name: selected.name,
          cover_asset_id: selected.cover_asset_id,
          theme: selected.theme,
          enabled: selected.enabled,
          sort_order: selected.sort_order,
          is_default: selected.is_default,
          color_scheme: inferColorScheme(selected.theme.background, selected.color_scheme),
          cover_title: selected.cover_title,
          cover_subtitle: selected.cover_subtitle,
          cover_character_name: selected.cover_character_name,
          cover_dialogue: selected.cover_dialogue,
          cover_voice_label: selected.cover_voice_label,
          cover_voice_asset_id: selected.cover_voice_asset_id,
          login_success_voice_asset_id: selected.login_success_voice_asset_id,
          kanban_asset_id: selected.kanban_asset_id,
        }),
      });
      if (!response.ok) {
        throw new Error(await responseMessage(response, "灵衣保存失败"));
      }
      const saved = await response.json() as AdminRaiment;
      setData((current) => current ? {
        items: sortRaiments(current.items.map((item) => {
          if (item.id === saved.id) return saved;
          if (saved.is_default && item.is_default) {
            return { ...item, is_default: false, revision: item.revision + 1 };
          }
          return item;
        })),
      } : current);
      window.dispatchEvent(new Event("helt:raiments-updated"));
      notify(`${saved.name} 已保存并同步到博客`, "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "灵衣保存失败", "danger");
    } finally {
      setSaving(null);
    }
  };

  const openCreate = () => {
    if (!data || !selected) return;
    setCreateDraft({
      name: "新灵衣",
      enabled: true,
      is_default: false,
      sort_order: Math.max(-1, ...data.items.map((item) => item.sort_order)) + 1,
    });
    setCreateOpen(true);
  };

  const createRaiment = async () => {
    if (!selected || saving) return;
    setSaving("create");
    try {
      const response = await fetch("/api/v1/admin/raiments", {
        method: "POST",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          ...createDraft,
          color_scheme: inferColorScheme(selected.theme.background, selected.color_scheme),
          cover_asset_id: selected.cover_asset_id,
          theme: selected.theme,
          cover_title: selected.cover_title,
          cover_subtitle: selected.cover_subtitle,
          cover_character_name: selected.cover_character_name,
          cover_dialogue: selected.cover_dialogue,
          cover_voice_label: selected.cover_voice_label,
          cover_voice_asset_id: selected.cover_voice_asset_id,
          login_success_voice_asset_id: selected.login_success_voice_asset_id,
          kanban_asset_id: null,
        }),
      });
      if (!response.ok) {
        throw new Error(await responseMessage(response, "灵衣创建失败"));
      }
      const created = await response.json() as AdminRaiment;
      setData((current) => current ? {
        items: sortRaiments([
          ...current.items.map((item) => created.is_default && item.is_default
            ? { ...item, is_default: false, revision: item.revision + 1 }
            : item),
          created,
        ]),
      } : current);
      setSelectedId(created.id);
      setCreateOpen(false);
      window.dispatchEvent(new Event("helt:raiments-updated"));
      notify(`${created.name} 已创建`, "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "灵衣创建失败", "danger");
    } finally {
      setSaving(null);
    }
  };

  const deleteRaiment = async () => {
    if (!selected || !data || saving) return;
    if (data.items.length <= 1) {
      notify("至少需要保留一套灵衣供博客展示", "danger");
      return;
    }
    if (selected.is_default) {
      notify("请先将另一套已启用灵衣设为默认", "danger");
      return;
    }
    if (!window.confirm(`确定删除“${selected.name}”吗？删除后无法恢复。`)) return;
    setSaving("delete");
    try {
      const response = await fetch(`/api/v1/admin/raiments/${encodeURIComponent(selected.id)}`, {
        method: "DELETE",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ revision: selected.revision }),
      });
      if (!response.ok) {
        throw new Error(await responseMessage(response, "灵衣删除失败"));
      }
      const remaining = data.items.filter((item) => item.id !== selected.id);
      setData({ items: remaining });
      setSelectedId(remaining[0]?.id || "");
      window.dispatchEvent(new Event("helt:raiments-updated"));
      notify(`${selected.name} 已删除`, "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "灵衣删除失败", "danger");
    } finally {
      setSaving(null);
    }
  };

  const chooseAsset = (asset: AdminAsset) => {
    if (assetPickerOpen === "cover") {
      updateSelected({ cover_asset_id: asset.id, cover_asset: asset });
    } else if (assetPickerOpen === "voice") {
      updateSelected({ cover_voice_asset_id: asset.id, cover_voice_asset: asset });
    } else if (assetPickerOpen === "successVoice") {
      updateSelected({ login_success_voice_asset_id: asset.id, login_success_voice_asset: asset });
    }
    setAssetPickerOpen(null);
  };

  if (loading && !data) {
    return <>
      <div className="admin-title"><div><h1>灵衣</h1></div></div>
      <section className="admin-panel raiment-loading" aria-live="polite">正在读取灵衣与素材库…</section>
    </>;
  }

  if (!data || !selected) {
    return <>
      <div className="admin-title"><div><h1>灵衣</h1></div></div>
      <section className="admin-panel raiment-loading">
        <p>没有可编辑的灵衣。</p>
        <button type="button" onClick={() => void load()}>重新加载</button>
      </section>
    </>;
  }

  return <>
    <div className="admin-title raiment-page-title">
      <div><h1>灵衣</h1></div>
      <button className="admin-primary" type="button" disabled={saving !== null} onClick={openCreate}>＋ 添加灵衣</button>
    </div>

    <div className="raiment-mode-switch" role="tablist" aria-label="灵衣列表">
      {data.items.map((item) => <button
        key={item.id}
        type="button"
        role="tab"
        aria-selected={selected.id === item.id}
        className={selected.id === item.id ? "active" : ""}
        onClick={() => setSelectedId(item.id)}
      >
        <b>{item.name}</b>
        <small>{item.is_default ? "默认" : item.enabled ? "已启用" : "已停用"}</small>
      </button>)}
    </div>

    <section className="admin-panel raiment-editor-panel">
      <header className="raiment-editor-header">
        <div>
          <h2>{selected.name}</h2>
        </div>
        <div>
          <button type="button" className="raiment-delete-button" disabled={saving !== null || data.items.length <= 1 || selected.is_default} onClick={() => void deleteRaiment()}>
            {saving === "delete" ? "删除中…" : "删除"}
          </button>
          <button type="button" className="admin-primary" disabled={saving !== null} onClick={() => void saveRaiment()}>
            {saving === "save" ? "保存中…" : "保存当前灵衣"}
          </button>
        </div>
      </header>

      <div className="raiment-editor-grid">
        <div className="raiment-editor-media">
          <div
            className="raiment-editor-preview"
            style={{
              "--raiment-primary": selected.theme.primary,
              "--raiment-secondary": selected.theme.secondary,
            } as React.CSSProperties}
          >
            <Image
              src={selected.cover_asset.file.url}
              width={5120}
              height={2160}
              sizes="(max-width: 1100px) 100vw, 42vw"
              unoptimized
              alt={`${selected.name} 灵衣预览`}
            />
            <div>
              <span>{selected.name}</span>
              <h3>{selected.cover_title || selected.name}</h3>
              <p>{selected.cover_subtitle || "封面文字、图像、语音和主题会作为同一套灵衣同步到博客前台。"}</p>
              <button type="button" onClick={() => setAssetPickerOpen("cover")}>更换封面</button>
            </div>
          </div>

          <section className="raiment-form-section raiment-name-section">
            <h3>灵衣名称</h3>
            <label>显示名称<input value={selected.name} maxLength={80} onChange={(event) => updateSelected({ name: event.target.value })} /></label>
            <div className="raiment-form-row">
              <label>排序值<input type="number" min={0} step={1} value={selected.sort_order} onChange={(event) => updateSelected({ sort_order: Math.max(0, Number.parseInt(event.target.value, 10) || 0) })} /></label>
              <label className="raiment-checkbox"><input type="checkbox" checked={selected.enabled} disabled={selected.is_default} onChange={(event) => updateSelected({ enabled: event.target.checked })} />公开启用</label>
              <label className="raiment-checkbox"><input type="radio" name="default-raiment" checked={selected.is_default} disabled={selected.is_default || !selected.enabled} onChange={() => updateSelected({ is_default: true })} />设为默认</label>
            </div>
          </section>

          <section className="raiment-form-section raiment-asset-summary">
            <h3>关联素材</h3>
            <div><span>封面图像</span><b>{selected.cover_asset.name}</b><button type="button" onClick={() => setAssetPickerOpen("cover")}>更换</button></div>
            <div><span>封面语音</span><b>{selected.cover_voice_asset?.name || "未配置"}</b><button type="button" onClick={() => setAssetPickerOpen("voice")}>选择</button>{selected.cover_voice_asset_id && <button type="button" onClick={() => updateSelected({ cover_voice_asset_id: null, cover_voice_asset: null })}>移除</button>}</div>
            <div><span>登录成功语音</span><b>{selected.login_success_voice_asset?.name || "未配置"}</b><button type="button" onClick={() => setAssetPickerOpen("successVoice")}>选择</button>{selected.login_success_voice_asset_id && <button type="button" onClick={() => updateSelected({ login_success_voice_asset_id: null, login_success_voice_asset: null })}>移除</button>}</div>
            <Link href="/admin/assets">管理素材库 →</Link>
          </section>

          <section className="raiment-kanban-slot">
            <div><p><b>看板娘</b></p></div>
            <p>{selected.kanban_asset_id ? "已绑定看板娘素材" : "功能位置已预留，等待看板娘模块接入。"}</p>
            <button type="button" disabled>暂未开放</button>
          </section>
        </div>

        <div className="raiment-editor-controls">
          <section className="raiment-form-section">
            <h3>封面文字</h3>
            <label>封面标题<textarea value={selected.cover_title} maxLength={240} rows={3} onChange={(event) => updateSelected({ cover_title: event.target.value })} /></label>
            <label>封面副标题<input value={selected.cover_subtitle} maxLength={240} onChange={(event) => updateSelected({ cover_subtitle: event.target.value })} /></label>
            <div className="raiment-form-row">
              <label>对话角色名<input value={selected.cover_character_name} maxLength={80} onChange={(event) => updateSelected({ cover_character_name: event.target.value })} /></label>
              <label>语音按钮文字<input value={selected.cover_voice_label} maxLength={120} onChange={(event) => updateSelected({ cover_voice_label: event.target.value })} /></label>
            </div>
            <div className="raiment-dialogue-editor">
              <div><span>封面左下角对白</span><button type="button" onClick={addDialogue}>＋ 添加对白</button></div>
              {dialogueItems(selected.cover_dialogue).map((dialogue, index) => <label key={index}>
                <span>{String(index + 1).padStart(2, "0")}</span>
                <input value={dialogue} maxLength={240} placeholder="输入一条封面对话" onChange={(event) => updateDialogue(index, event.target.value)} />
                <button type="button" aria-label={`删除第 ${index + 1} 条封面对话`} onClick={() => removeDialogue(index)}>×</button>
              </label>)}
              <small>每套灵衣独立保存；首页点击对话框或等待 6 秒会显示下一条。</small>
            </div>
          </section>

          <section className="raiment-form-section">
            <h3>页面配色</h3>
            <div className="raiment-color-grid">
              {([
                ["primary", "按钮、链接与高亮"],
                ["secondary", "次要强调色"],
                ["background", "页面背景"],
                ["surface", "卡片背景"],
                ["surface_alt", "输入框与次级区域"],
                ["text", "主要文字"],
                ["text_secondary", "说明文字"],
                ["muted", "弱化文字"],
                ["faint", "占位与辅助文字"],
                ["border", "边框与分隔线"],
                ["danger", "删除与错误提示"],
                ["success", "成功提示"],
              ] as Array<[keyof ThemeTokens, string]>).map(([key, label]) => <div className="color-token" key={key}>
                <label className="raiment-color-swatch" style={{ background: selected.theme[key] }}>
                  <span className="sr-only">{label}取色器</span>
                  <input type="color" value={selected.theme[key]} onChange={(event) => updateColor(key, event.target.value.toUpperCase())} />
                </label>
                <label>{label}<input value={selected.theme[key]} maxLength={7} onChange={(event) => updateColor(key, event.target.value)} /></label>
              </div>)}
            </div>
          </section>
        </div>
      </div>
    </section>

    {createOpen && <div className="raiment-create-modal" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && setCreateOpen(false)}>
      <form onSubmit={(event) => { event.preventDefault(); void createRaiment(); }} role="dialog" aria-modal="true" aria-labelledby="raiment-create-title">
        <header><div><h2 id="raiment-create-title">添加灵衣</h2></div><button type="button" aria-label="关闭添加灵衣" onClick={() => setCreateOpen(false)}>×</button></header>
        <p>新灵衣会复制当前灵衣的主题、封面图像、文字和语音，创建后可继续单独调整。</p>
        <label>显示名称<input autoFocus value={createDraft.name} maxLength={80} onChange={(event) => setCreateDraft({ ...createDraft, name: event.target.value })} /></label>
        <label>排序值<input type="number" min={0} step={1} value={createDraft.sort_order} onChange={(event) => setCreateDraft({ ...createDraft, sort_order: Math.max(0, Number.parseInt(event.target.value, 10) || 0) })} /></label>
        <label className="raiment-checkbox"><input type="checkbox" checked={createDraft.enabled} disabled={createDraft.is_default} onChange={(event) => setCreateDraft({ ...createDraft, enabled: event.target.checked })} />创建后公开启用</label>
        <label className="raiment-checkbox"><input type="checkbox" checked={createDraft.is_default} onChange={(event) => setCreateDraft({ ...createDraft, is_default: event.target.checked, enabled: event.target.checked ? true : createDraft.enabled })} />同时设为默认灵衣</label>
        <footer><button type="button" onClick={() => setCreateOpen(false)}>取消</button><button className="admin-primary" disabled={saving !== null}>{saving === "create" ? "创建中…" : "创建并编辑"}</button></footer>
      </form>
    </div>}

    {assetPickerOpen && <div className="raiment-asset-modal" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && setAssetPickerOpen(null)}>
      <section role="dialog" aria-modal="true" aria-labelledby="raiment-cover-picker-title">
        <header>
          <div><h2 id="raiment-cover-picker-title">选择{assetPickerOpen === "cover" ? "封面图像" : assetPickerOpen === "successVoice" ? "登录成功语音" : "封面语音"}</h2></div>
          <button type="button" aria-label="关闭素材选择" onClick={() => setAssetPickerOpen(null)}>×</button>
        </header>
        <div className="raiment-asset-grid">
          {assets.filter((asset) => asset.media_type === pickerMediaType).map((asset) => <button
            type="button"
            key={asset.id}
            className={cx(asset.id === pickerAssetId && "selected")}
            onClick={() => chooseAsset(asset)}
          >
            {asset.media_type === "image" ? <Image src={asset.file.url} width={220} height={130} unoptimized alt={asset.name} /> : <span className="raiment-audio-asset">♪</span>}
            <b>{asset.name}</b><small>{asset.media_type === "image" ? "图片" : "音频"}</small>
          </button>)}
          {!assets.some((asset) => asset.media_type === pickerMediaType) && <p>素材库中暂无可用素材，请先上传。</p>}
        </div>
        <footer><Link href="/admin/assets">去素材库上传 →</Link><button type="button" onClick={() => setAssetPickerOpen(null)}>取消</button></footer>
      </section>
    </div>}
  </>;
}
