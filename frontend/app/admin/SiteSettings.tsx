"use client";

import Image from "next/image";
import { useCallback, useEffect, useState } from "react";
import { createPortal } from "react-dom";

import {
  AdminPlaylist,
  AdminAsset,
  formatAssetSize,
  Notify,
  PlaylistPayload,
  RaimentSchedule,
  RaimentSchedulePeriod,
  responseMessage,
  SitePayload,
} from "./shared";

type RaimentOption = {
  id: string;
  name: string;
  color_scheme: "day" | "night";
  enabled: boolean;
  is_default: boolean;
};

type RaimentPayload = {
  items: RaimentOption[];
};

type BrandingSlot = "logo" | "favicon";

const BRANDING_ASSET_PAGE_SIZE = 12;

const featureOptions: Array<[keyof SitePayload["features"], string]> = [
  ["splash", "开屏页"],
  ["comments", "全站评论"],
  ["kanban", "看板娘"],
  ["music", "背景音乐"],
  ["stats", "访问统计"],
  ["easter_egg", "Konami 彩蛋（↑ ↑ ↓ ↓ ← → ← → B A）"],
];

const newPeriodId = () => `period-${typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : Date.now()}`;

export function SiteSettings({ notify }: { notify: Notify }) {
  const [schedule, setSchedule] = useState<RaimentSchedule | null>(null);
  const [site, setSite] = useState<SitePayload | null>(null);
  const [raiments, setRaiments] = useState<RaimentOption[]>([]);
  const [playlists, setPlaylists] = useState<AdminPlaylist[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [assetPicker, setAssetPicker] = useState<BrandingSlot | null>(null);

  const load = useCallback(async (signal?: AbortSignal) => {
    setLoading(true);
    try {
      const [siteResponse, scheduleResponse, raimentResponse, playlistResponse] = await Promise.all([
        fetch("/api/v1/admin/site/settings", { credentials: "include", signal }),
        fetch("/api/v1/admin/site/raiment-schedule", { credentials: "include", signal }),
        fetch("/api/v1/admin/raiments", { credentials: "include", signal }),
        fetch("/api/v1/admin/playlists", { credentials: "include", signal }),
      ]);
      if (!siteResponse.ok) {
        throw new Error(await responseMessage(siteResponse, "站点基本信息加载失败"));
      }
      if (!scheduleResponse.ok) {
        throw new Error(await responseMessage(scheduleResponse, "灵衣时间段加载失败"));
      }
      if (!raimentResponse.ok) {
        throw new Error(await responseMessage(raimentResponse, "灵衣列表加载失败"));
      }
      if (!playlistResponse.ok) {
        throw new Error(await responseMessage(playlistResponse, "歌单列表加载失败"));
      }
      const [nextSite, nextSchedule, nextRaiments, nextPlaylists] = await Promise.all([
        siteResponse.json() as Promise<SitePayload>,
        scheduleResponse.json() as Promise<RaimentSchedule>,
        raimentResponse.json() as Promise<RaimentPayload>,
        playlistResponse.json() as Promise<PlaylistPayload>,
      ]);
      setSite(nextSite);
      setSchedule(nextSchedule);
      setRaiments(nextRaiments.items.filter((item) => item.enabled));
      setPlaylists(nextPlaylists.items.filter((item) => item.enabled));
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") return;
      notify(error instanceof Error ? error.message : "站点设置加载失败", "danger");
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

  const updatePeriod = (id: string, patch: Partial<RaimentSchedulePeriod>) => {
    setSchedule((current) => current ? {
      ...current,
      periods: current.periods.map((period) => period.id === id ? { ...period, ...patch } : period),
    } : current);
  };

  const addPeriod = () => {
    if (!raiments.length) {
      notify("请先添加至少一套灵衣", "danger");
      return;
    }
    setSchedule((current) => current ? {
      ...current,
      periods: [...current.periods, {
        id: newPeriodId(),
        start_at: "08:00",
        end_at: "18:00",
        raiment_id: raiments[0].id,
        playlist_id: null,
      }],
    } : current);
  };

  const removePeriod = (id: string) => {
    setSchedule((current) => current ? {
      ...current,
      periods: current.periods.filter((period) => period.id !== id),
    } : current);
  };

  const updateBasic = <Key extends keyof SitePayload["basic"]>(key: Key, value: SitePayload["basic"][Key]) => {
    setSite((current) => current ? { ...current, basic: { ...current.basic, [key]: value } } : current);
  };

  const updateFeature = (key: keyof SitePayload["features"], value: boolean) => {
    setSite((current) => current ? { ...current, features: { ...current.features, [key]: value } } : current);
  };

  const chooseAsset = (slot: BrandingSlot, asset: AdminAsset | null) => {
    const assetId = asset?.id ?? null;
    if (slot === "logo") {
      updateBasic("logo_asset_id", assetId);
      updateBasic("logo_url", asset?.file.url ?? null);
    } else {
      updateBasic("favicon_asset_id", assetId);
      updateBasic("favicon_url", asset?.file.url ?? null);
    }
    setAssetPicker(null);
  };

  const save = async () => {
    if (!schedule || !site || saving) return;
    setSaving(true);
    try {
      const [siteResponse, scheduleResponse] = await Promise.all([
        fetch("/api/v1/admin/site/settings", {
          method: "PUT",
          credentials: "include",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({
            basic: {
              name: site.basic.name,
              tagline: site.basic.tagline,
              footer_text: site.basic.footer_text,
              footer_copyright: site.basic.footer_copyright,
              hero_eyebrow: site.basic.hero_eyebrow,
              icp: site.basic.icp,
              logo_asset_id: site.basic.logo_asset_id,
              favicon_asset_id: site.basic.favicon_asset_id,
            },
            features: site.features,
            updated_at: site.updated_at,
          }),
        }),
        fetch("/api/v1/admin/site/raiment-schedule", {
          method: "PUT",
          credentials: "include",
          headers: { "content-type": "application/json" },
          body: JSON.stringify(schedule),
        }),
      ]);
      if (!siteResponse.ok) {
        throw new Error(await responseMessage(siteResponse, "站点基本信息保存失败"));
      }
      if (!scheduleResponse.ok) {
        throw new Error(await responseMessage(scheduleResponse, "灵衣时间段保存失败"));
      }
      const [savedSite, savedSchedule] = await Promise.all([
        siteResponse.json() as Promise<SitePayload>,
        scheduleResponse.json() as Promise<RaimentSchedule>,
      ]);
      setSite(savedSite);
      setSchedule(savedSchedule);
      window.dispatchEvent(new Event("helt:site-settings-updated"));
      window.dispatchEvent(new Event("helt:raiments-updated"));
      notify("站点设置已与博客同步", "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "站点设置保存失败", "danger");
    } finally {
      setSaving(false);
    }
  };

  return <>
    <div className="admin-title">
      <div><h1>站点设置</h1><p>BRANDING · FEATURES · 24-HOUR RAIMENT SCHEDULE</p></div>
      <button className="admin-primary" type="button" disabled={!site || !schedule || saving} onClick={() => void save()}>{saving ? "保存中…" : "保存设置"}</button>
    </div>
    <div className="settings-grid">
      <section className="admin-panel site-branding-panel">
        <h2>站点品牌 <small>{loading ? "LOADING" : "LIVE LINKED"}</small></h2>
        <div className="branding-assets">
          <BrandingAssetField
            label="站点 Logo"
            hint="替代导航栏、开屏与页脚中的文字站名，建议使用透明背景横版 PNG/WebP。"
            value={site?.basic.logo_asset_id ?? null}
            previewUrl={site?.basic.logo_url ?? null}
            fallback={site?.basic.name || "helt."}
            onOpen={() => setAssetPicker("logo")}
            onClear={() => chooseAsset("logo", null)}
          />
          <BrandingAssetField
            label="浏览器图标"
            hint="显示在浏览器标签页左侧；推荐正方形 PNG、WebP 或 ICO。"
            value={site?.basic.favicon_asset_id ?? null}
            previewUrl={site?.basic.favicon_url ?? null}
            fallback="◆"
            compact
            onOpen={() => setAssetPicker("favicon")}
            onClear={() => chooseAsset("favicon", null)}
          />
        </div>
        <label><span>站点简介 <small>用于浏览器标题；关于页未填写站点寄语时作为回退</small></span><textarea maxLength={300} value={site?.basic.tagline ?? ""} disabled={!site} placeholder="一句话概括站点内容" onChange={(event) => updateBasic("tagline", event.target.value)} /></label>
        <label><span>页脚介绍 <small>仅显示在页脚品牌下方，与站点简介相互独立</small></span><textarea maxLength={500} value={site?.basic.footer_text ?? ""} disabled={!site} placeholder="可留空；支持换行" onChange={(event) => updateBasic("footer_text", event.target.value)} /></label>
        <label><span>页脚底部文字 <small>支持 {'{year}'}（当前年份）和 {'{site_name}'}（站点名称）</small></span><input maxLength={300} value={site?.basic.footer_copyright ?? ""} disabled={!site} placeholder="可留空；备案号会单独追加在其后" onChange={(event) => updateBasic("footer_copyright", event.target.value)} /></label>
        <label>封面标识文字<input maxLength={120} value={site?.basic.hero_eyebrow ?? ""} disabled={!site} placeholder="SINCE 2020 · HELT'S BLOG" onChange={(event) => updateBasic("hero_eyebrow", event.target.value)} /></label>
        <div className="site-basic-row">
          <label>站点地址<input value={site?.basic.domain ?? ""} readOnly aria-readonly="true" /></label>
          <label>ICP 备案号<input maxLength={100} value={site?.basic.icp ?? ""} disabled={!site} placeholder="可留空" onChange={(event) => updateBasic("icp", event.target.value)} /></label>
        </div>
        <small className="branding-note">图片无法加载时仍会使用“{site?.basic.name || "helt."}”作为无障碍名称和标题回退。</small>
      </section>
      <section className="admin-panel toggles">
        <h2>功能开关</h2>
        {featureOptions.map(([key, label]) => <div key={key}>
          <span><b>{label}</b></span>
          <label className="toggle"><input type="checkbox" checked={site?.features[key] ?? false} disabled={!site} onChange={(event) => updateFeature(key, event.target.checked)} /><i /></label>
        </div>)}
      </section>
    </div>

    <section className="admin-panel raiment-schedule-panel">
      <header>
        <div><span>AUTOMATION</span><h2>灵衣与背景音乐时间段</h2></div>
        <button type="button" onClick={addPeriod} disabled={loading || !schedule}>＋ 新增时间段</button>
      </header>
      {loading && <div className="raiment-schedule-empty">正在读取时间段…</div>}
      {!loading && schedule?.periods.map((period, index) => {
        return <div className="raiment-schedule-row" key={period.id}>
          <b>{String(index + 1).padStart(2, "0")}</b>
          <label>开始<input type="time" step={60} value={period.start_at} onChange={(event) => updatePeriod(period.id, { start_at: event.target.value })} /></label>
          <span>→</span>
          <label>结束<input type="time" step={60} value={period.end_at} onChange={(event) => updatePeriod(period.id, { end_at: event.target.value })} /></label>
          <label>使用灵衣<select value={period.raiment_id} onChange={(event) => updatePeriod(period.id, { raiment_id: event.target.value })}>{raiments.map((raiment) => <option value={raiment.id} key={raiment.id}>{raiment.name}</option>)}</select></label>
          <label>背景音乐<select value={period.playlist_id ?? ""} onChange={(event) => updatePeriod(period.id, { playlist_id: event.target.value ? Number(event.target.value) : null })}>
            <option value="">不播放</option>
            {playlists.map((playlist) => <option value={playlist.id} key={playlist.id}>{playlist.name}{playlist.status === "unavailable" ? "（暂不可用）" : ""}</option>)}
          </select></label>
          <button type="button" aria-label={`删除第 ${index + 1} 个时间段`} onClick={() => removePeriod(period.id)}>×</button>
        </div>;
      })}
      {!loading && schedule && !schedule.periods.length && <div className="raiment-schedule-empty">尚未设置自动时间段；前台将使用默认灵衣，访客仍可手动切换。</div>}
      <footer>时间段不可重叠；开始时间包含在内，结束时间不包含在内。未覆盖的时刻使用默认灵衣且不播放背景音乐。</footer>
    </section>
    {assetPicker && <BrandingAssetPicker
      slot={assetPicker}
      currentId={assetPicker === "logo" ? site?.basic.logo_asset_id ?? null : site?.basic.favicon_asset_id ?? null}
      onClose={() => setAssetPicker(null)}
      onSelect={(asset) => chooseAsset(assetPicker, asset)}
    />}
  </>;
}

function BrandingAssetField({
  label,
  hint,
  value,
  previewUrl,
  fallback,
  compact = false,
  onOpen,
  onClear,
}: {
  label: string;
  hint: string;
  value: number | null;
  previewUrl: string | null;
  fallback: string;
  compact?: boolean;
  onOpen: () => void;
  onClear: () => void;
}) {
  return <article className="branding-asset-field">
    <div className={compact ? "branding-preview compact" : "branding-preview"}>
      {previewUrl ? <Image src={previewUrl} width={320} height={120} unoptimized alt={`${label}预览`} /> : <span>{fallback}</span>}
    </div>
    <div className="branding-asset-copy"><b>{label}</b><small>{hint}</small></div>
    <div className="branding-asset-actions">
      <button type="button" onClick={onOpen}>{value ? "更换素材" : "从素材库选择"}</button>
      {value && <button type="button" onClick={onClear}>移除</button>}
    </div>
  </article>;
}

function BrandingAssetPicker({
  slot,
  currentId,
  onClose,
  onSelect,
}: {
  slot: BrandingSlot;
  currentId: number | null;
  onClose: () => void;
  onSelect: (asset: AdminAsset) => void;
}) {
  const [assets, setAssets] = useState<AdminAsset[]>([]);
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const pageCount = Math.max(1, Math.ceil(total / BRANDING_ASSET_PAGE_SIZE));
  const label = slot === "logo" ? "站点 Logo" : "浏览器图标";

  useEffect(() => {
    const controller = new AbortController();
    const timer = window.setTimeout(async () => {
      setLoading(true);
      setError("");
      const params = new URLSearchParams({
        page: String(page),
        per_page: String(BRANDING_ASSET_PAGE_SIZE),
        media_type: "image",
        sort: "updated_at",
        order: "desc",
      });
      if (query.trim()) params.set("search", query.trim());
      try {
        const response = await fetch(`/api/v1/admin/assets?${params}`, {
          credentials: "include",
          signal: controller.signal,
        });
        if (!response.ok) throw new Error(await responseMessage(response, "图片素材加载失败"));
        const payload = await response.json() as { items: AdminAsset[]; total: number };
        setAssets(payload.items);
        setTotal(payload.total);
      } catch (reason) {
        if (reason instanceof DOMException && reason.name === "AbortError") return;
        setAssets([]);
        setTotal(0);
        setError(reason instanceof Error ? reason.message : "图片素材加载失败");
      } finally {
        if (!controller.signal.aborted) setLoading(false);
      }
    }, 180);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [page, query]);

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [onClose]);

  if (typeof document === "undefined") return null;
  return createPortal(
    <div className="branding-asset-overlay" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
      <section className="branding-asset-dialog" role="dialog" aria-modal="true" aria-labelledby="branding-asset-title">
        <header>
          <div><span>ASSET LIBRARY</span><h2 id="branding-asset-title">选择{label}</h2><p>这里只显示素材库中的图片；新增素材请统一前往素材库上传。</p></div>
          <button type="button" aria-label="关闭素材选择" onClick={onClose}>×</button>
        </header>
        <label className="branding-asset-search"><span>⌕</span><input autoFocus aria-label="搜索图片素材" placeholder="搜索素材名称或原始文件名…" value={query} onChange={(event) => { setQuery(event.target.value); setPage(1); }} /></label>
        <div className="branding-asset-results" aria-live="polite">
          {loading && <p>正在读取图片素材…</p>}
          {!loading && error && <p className="error">{error}</p>}
          {!loading && !error && assets.map((asset) => <button type="button" key={asset.id} className={currentId === asset.id ? "selected" : ""} onClick={() => onSelect(asset)}>
            <Image src={asset.file.url} width={240} height={150} unoptimized alt="" />
            <span><b>{asset.name}</b><small>{asset.file.original_filename || asset.file.mime} · {formatAssetSize(asset.file.size_bytes)}</small></span>
            <i>{currentId === asset.id ? "当前使用" : "选择"}</i>
          </button>)}
          {!loading && !error && !assets.length && <p>没有找到匹配的图片素材。</p>}
        </div>
        <footer>
          <span>共 {total} 张图片</span>
          <div>
            <button type="button" disabled={page <= 1 || loading} onClick={() => setPage((value) => Math.max(1, value - 1))}>上一页</button>
            <span>{page} / {pageCount}</span>
            <button type="button" disabled={page >= pageCount || loading} onClick={() => setPage((value) => Math.min(pageCount, value + 1))}>下一页</button>
          </div>
        </footer>
      </section>
    </div>,
    document.body,
  );
}
