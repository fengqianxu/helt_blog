"use client";

import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";

import {
  AdminAsset,
  AdminPlaylist,
  formatAssetSize,
  Notify,
  PlaylistPayload,
  PlaylistTracksPayload,
  responseMessage,
} from "./shared";

const sourceLabels: Record<AdminPlaylist["source_kind"], string> = {
  local: "素材库",
  netease: "网易云音乐",
  qq: "QQ 音乐",
};

const ASSET_PAGE_SIZE = 12;
const TRACK_PAGE_SIZE = 10;

function paginationWindow(page: number, pageCount: number) {
  const start = Math.max(1, Math.min(page - 2, pageCount - 4));
  const end = Math.min(pageCount, start + 4);
  return Array.from({ length: end - start + 1 }, (_, index) => start + index);
}

export function PlaylistSettings({ notify }: { notify: Notify }) {
  const [payload, setPayload] = useState<PlaylistPayload | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [sourceKind, setSourceKind] = useState<AdminPlaylist["source_kind"]>("local");
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [externalReference, setExternalReference] = useState("");
  const [renameOpen, setRenameOpen] = useState(false);
  const [renameName, setRenameName] = useState("");
  const [assetPickerOpen, setAssetPickerOpen] = useState(false);
  const [audioAssets, setAudioAssets] = useState<AdminAsset[]>([]);
  const [assetQuery, setAssetQuery] = useState("");
  const [assetPage, setAssetPage] = useState(1);
  const [assetTotal, setAssetTotal] = useState(0);
  const [assetLoading, setAssetLoading] = useState(false);
  const [selectedAsset, setSelectedAsset] = useState<AdminAsset | null>(null);
  const [trackTitle, setTrackTitle] = useState("");
  const [trackArtist, setTrackArtist] = useState("");
  const [trackPayload, setTrackPayload] = useState<PlaylistTracksPayload | null>(null);
  const [trackPage, setTrackPage] = useState(1);
  const [trackLoading, setTrackLoading] = useState(false);
  const [busy, setBusy] = useState("");
  const [deleteConfirm, setDeleteConfirm] = useState<number | null>(null);
  const trackRequestId = useRef(0);

  const request = async (url: string, init: RequestInit, fallback: string) => {
    const response = await fetch(url, { credentials: "include", ...init });
    if (!response.ok) throw new Error(await responseMessage(response, fallback));
    return response;
  };

  const load = useCallback(async () => {
    try {
      const response = await fetch("/api/v1/admin/playlists", { credentials: "include" });
      if (!response.ok) throw new Error(await responseMessage(response, "歌单加载失败"));
      const nextPayload = await response.json() as PlaylistPayload;
      setPayload(nextPayload);
      setSelectedId((current) => nextPayload.items.some((item) => item.id === current)
        ? current
        : nextPayload.items[0]?.id ?? null);
    } catch (error) {
      notify(error instanceof Error ? error.message : "歌单加载失败", "danger");
    }
  }, [notify]);

  const loadAssets = useCallback(async (signal?: AbortSignal) => {
    setAssetLoading(true);
    const params = new URLSearchParams({
      media_type: "audio",
      page: String(assetPage),
      per_page: String(ASSET_PAGE_SIZE),
    });
    if (assetQuery.trim()) params.set("search", assetQuery.trim());
    try {
      const response = await fetch(`/api/v1/admin/assets?${params}`, {
        credentials: "include",
        signal,
      });
      if (!response.ok) throw new Error(await responseMessage(response, "音频素材加载失败"));
      const result = await response.json() as { items: AdminAsset[]; total: number };
      setAudioAssets(result.items);
      setAssetTotal(result.total);
    } catch (error) {
      if (!(error instanceof DOMException && error.name === "AbortError")) {
        notify(error instanceof Error ? error.message : "音频素材加载失败", "danger");
      }
    } finally {
      setAssetLoading(false);
    }
  }, [assetPage, assetQuery, notify]);

  const loadTracks = useCallback(async (playlistId: number, page: number, signal?: AbortSignal) => {
    const requestId = ++trackRequestId.current;
    setTrackLoading(true);
    const params = new URLSearchParams({
      page: String(page),
      per_page: String(TRACK_PAGE_SIZE),
    });
    try {
      const response = await fetch(`/api/v1/admin/playlists/${playlistId}/tracks?${params}`, {
        credentials: "include",
        signal,
      });
      if (!response.ok) throw new Error(await responseMessage(response, "歌曲加载失败"));
      const result = await response.json() as PlaylistTracksPayload;
      if (requestId === trackRequestId.current) setTrackPayload(result);
      return result;
    } catch (error) {
      if (!(error instanceof DOMException && error.name === "AbortError")) {
        notify(error instanceof Error ? error.message : "歌曲加载失败", "danger");
      }
      return null;
    } finally {
      if (requestId === trackRequestId.current) setTrackLoading(false);
    }
  }, [notify]);

  useEffect(() => {
    const timer = window.setTimeout(() => void load(), 0);
    return () => window.clearTimeout(timer);
  }, [load]);

  useEffect(() => {
    if (selectedId === null) return;
    const controller = new AbortController();
    const timer = window.setTimeout(
      () => void loadTracks(selectedId, trackPage, controller.signal),
      0,
    );
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [loadTracks, selectedId, trackPage]);

  useEffect(() => {
    if (!assetPickerOpen) return;
    const controller = new AbortController();
    const timer = window.setTimeout(() => void loadAssets(controller.signal), 220);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [assetPickerOpen, loadAssets]);

  useEffect(() => {
    if (!assetPickerOpen) return;
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setAssetPickerOpen(false);
    };
    window.addEventListener("keydown", closeOnEscape);
    return () => window.removeEventListener("keydown", closeOnEscape);
  }, [assetPickerOpen]);

  const selected = useMemo(
    () => payload?.items.find((item) => item.id === selectedId) || null,
    [payload, selectedId],
  );
  const assetPageCount = Math.max(1, Math.ceil(assetTotal / ASSET_PAGE_SIZE));
  const selectedIndex = selected && payload
    ? payload.items.findIndex((item) => item.id === selected.id)
    : -1;
  const trackTotal = trackPayload?.total ?? selected?.track_count ?? null;
  const trackPageCount = Math.max(1, Math.ceil((trackTotal ?? 0) / TRACK_PAGE_SIZE));
  const visibleTrackPages = paginationWindow(trackPage, trackPageCount);
  const trackStatus = trackPayload?.status ?? selected?.status ?? "ready";
  const trackStatusMessage = trackPayload?.status_message ?? selected?.status_message ?? null;

  const createPlaylist = async (event: FormEvent) => {
    event.preventDefault();
    setBusy("create");
    try {
      const response = await request("/api/v1/admin/playlists", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          name: name.trim() || null,
          description,
          source_kind: sourceKind,
          external_reference: sourceKind === "local" ? null : externalReference,
          enabled: true,
        }),
      }, "歌单创建失败");
      const created = await response.json() as AdminPlaylist;
      setCreateOpen(false);
      setName("");
      setDescription("");
      setExternalReference("");
      await load();
      setTrackPayload(null);
      setTrackPage(1);
      setSelectedId(created.id);
      notify(`已创建歌单：${created.name}`, "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "歌单创建失败", "danger");
    } finally {
      setBusy("");
    }
  };

  const renamePlaylist = async (event: FormEvent) => {
    event.preventDefault();
    if (!selected) return;
    setBusy("rename");
    try {
      await request(`/api/v1/admin/playlists/${selected.id}`, {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          name: renameName,
          description: selected.description,
          enabled: selected.enabled,
        }),
      }, "歌单名称保存失败");
      setRenameOpen(false);
      await load();
      notify("歌单名称已更新", "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "歌单名称保存失败", "danger");
    } finally {
      setBusy("");
    }
  };

  const togglePlaylist = async (playlist: AdminPlaylist) => {
    setBusy(`toggle-${playlist.id}`);
    try {
      await request(`/api/v1/admin/playlists/${playlist.id}`, {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name: playlist.name, description: playlist.description, enabled: !playlist.enabled }),
      }, "歌单状态保存失败");
      await load();
      notify(playlist.enabled ? "歌单已停用" : "歌单已启用", "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "歌单状态保存失败", "danger");
    } finally {
      setBusy("");
    }
  };

  const movePlaylist = async (index: number, delta: number) => {
    if (!payload) return;
    const next = index + delta;
    if (next < 0 || next >= payload.items.length) return;
    const order = payload.items.map((item) => item.id);
    [order[index], order[next]] = [order[next], order[index]];
    setBusy("order");
    try {
      await request("/api/v1/admin/playlists/order", {
        method: "PUT",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ order }),
      }, "歌单顺序保存失败");
      await load();
    } catch (error) {
      notify(error instanceof Error ? error.message : "歌单顺序保存失败", "danger");
    } finally {
      setBusy("");
    }
  };

  const deletePlaylist = async (playlist: AdminPlaylist) => {
    if (deleteConfirm !== playlist.id) {
      setDeleteConfirm(playlist.id);
      return;
    }
    setBusy(`delete-${playlist.id}`);
    try {
      await request(`/api/v1/admin/playlists/${playlist.id}`, { method: "DELETE" }, "歌单删除失败");
      setDeleteConfirm(null);
      setRenameOpen(false);
      setTrackPayload(null);
      setTrackPage(1);
      await load();
      notify(`已删除歌单：${playlist.name}`, "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "歌单删除失败", "danger");
    } finally {
      setBusy("");
    }
  };

  const addTrack = async (event: FormEvent) => {
    event.preventDefault();
    if (!selected || !selectedAsset) return;
    setBusy("track-add");
    try {
      await request(`/api/v1/admin/playlists/${selected.id}/tracks`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          asset_id: selectedAsset.id,
          title: trackTitle.trim() || null,
          artist: trackArtist,
          duration_s: 0,
        }),
      }, "歌曲添加失败");
      setSelectedAsset(null);
      setTrackTitle("");
      setTrackArtist("");
      const nextPage = Math.max(1, Math.ceil(((trackPayload?.total ?? selected.track_count ?? 0) + 1) / TRACK_PAGE_SIZE));
      await load();
      if (nextPage === trackPage) {
        await loadTracks(selected.id, nextPage);
      } else {
        setTrackPayload(null);
        setTrackPage(nextPage);
      }
      notify("歌曲已加入歌单", "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "歌曲添加失败", "danger");
    } finally {
      setBusy("");
    }
  };

  const deleteTrack = async (trackId: string) => {
    if (!selected) return;
    setBusy(`track-${trackId}`);
    try {
      await request(`/api/v1/admin/playlists/${selected.id}/tracks/${trackId}`, { method: "DELETE" }, "歌曲移除失败");
      const nextTotal = Math.max(0, (trackPayload?.total ?? selected.track_count ?? 1) - 1);
      const nextPage = Math.min(trackPage, Math.max(1, Math.ceil(nextTotal / TRACK_PAGE_SIZE)));
      await load();
      if (nextPage === trackPage) {
        await loadTracks(selected.id, nextPage);
      } else {
        setTrackPayload(null);
        setTrackPage(nextPage);
      }
      notify("歌曲已从歌单移除", "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "歌曲移除失败", "danger");
    } finally {
      setBusy("");
    }
  };

  const chooseAsset = (asset: AdminAsset) => {
    setSelectedAsset(asset);
    setAssetPickerOpen(false);
  };

  return <>
    <div className="admin-title">
      <div><h1>歌单管理</h1><p>PLAYLISTS</p></div>
      <button className="admin-primary" aria-expanded={createOpen} onClick={() => setCreateOpen((value) => !value)}>{createOpen ? "收起" : "＋ 新建歌单"}</button>
    </div>

    {createOpen && <form className="playlist-create admin-panel" onSubmit={createPlaylist}>
      <header><div><span>CREATE PLAYLIST</span><h2>新建歌单</h2><p>选择曲目来源，然后补充歌单信息。</p></div><button type="button" onClick={() => setCreateOpen(false)} aria-label="关闭新建歌单">×</button></header>
      <div className="playlist-source-tabs" role="tablist" aria-label="歌单来源">
        {(["local", "netease", "qq"] as const).map((source) => <button type="button" role="tab" aria-selected={sourceKind === source} className={sourceKind === source ? "active" : ""} key={source} onClick={() => setSourceKind(source)}>
          <i className={`playlist-source ${source}`}>{source === "local" ? "♫" : source === "netease" ? "云" : "Q"}</i>
          <span><b>{sourceLabels[source]}</b><small>{source === "local" ? "从素材库自由编排" : "同步公开歌单曲目"}</small></span>
          <em>{sourceKind === source ? "✓" : ""}</em>
        </button>)}
      </div>
      <div className="playlist-create-fields">
        <label><span>歌单名称 <small>{sourceKind === "local" ? "必填" : "可选"}</small></span><input required={sourceKind === "local"} maxLength={120} placeholder={sourceKind === "local" ? "例如：深夜写作" : "留空将自动读取平台名称"} value={name} onChange={(event) => setName(event.target.value)} /></label>
        {sourceKind !== "local" && <label className="playlist-reference-field"><span>歌单链接或 ID <small>必填</small></span><input required placeholder={sourceKind === "netease" ? "粘贴网易云歌单链接或 ID" : "粘贴 QQ 音乐歌单链接、短链接或 ID"} value={externalReference} onChange={(event) => setExternalReference(event.target.value)} /></label>}
        <label><span>说明 <small>可选</small></span><input maxLength={500} placeholder="简单描述这个歌单" value={description} onChange={(event) => setDescription(event.target.value)} /></label>
      </div>
      <footer><span>外部歌单仅引用公开曲目；会员、下架或地区受限歌曲仍由原平台决定。</span><div><button type="button" onClick={() => setCreateOpen(false)}>取消</button><button className="admin-primary" disabled={busy === "create"}>{busy === "create" ? "正在验证…" : "创建歌单"}</button></div></footer>
    </form>}

    {!payload && <div className="empty-panel">正在读取歌单…</div>}
    {payload && <div className="playlist-admin-layout">
      <aside className="admin-panel playlist-catalog">
        <header><div><span>LIBRARY</span><h2>全部歌单</h2></div><b>{payload.items.length}</b></header>
        <div className="playlist-catalog-list">
        {payload.items.map((playlist) => <article className={selectedId === playlist.id ? "active" : ""} key={playlist.id}>
          <button type="button" className="playlist-catalog-select" aria-pressed={selectedId === playlist.id} onClick={() => { setSelectedId(playlist.id); setTrackPayload(null); setTrackPage(1); setRenameOpen(false); setDeleteConfirm(null); }}>
            <span className={`playlist-source ${playlist.source_kind}`}>{playlist.source_kind === "local" ? "♫" : playlist.source_kind === "netease" ? "云" : "Q"}</span>
            <span className="playlist-catalog-copy"><b>{playlist.name}</b><small>{sourceLabels[playlist.source_kind]} · {playlist.id === selectedId && trackPayload ? `${trackPayload.total} 首歌曲` : playlist.track_count === null ? "曲目按需加载" : `${playlist.track_count} 首歌曲`}</small></span>
            <span className={`playlist-catalog-status ${playlist.enabled ? "enabled" : ""}`}><i />{playlist.enabled ? "启用" : "停用"}</span>
            <span className="playlist-catalog-arrow">›</span>
          </button>
        </article>)}
        </div>
        {!payload.items.length && <p className="playlist-empty">还没有歌单，先创建一个吧。</p>}
      </aside>

      <div className="playlist-detail">
        {selected ? <>
          <section className="admin-panel playlist-detail-head">
            <div className="playlist-detail-summary">
              <span className={`playlist-source ${selected.source_kind}`}>{selected.source_kind === "local" ? "♫" : selected.source_kind === "netease" ? "云" : "Q"}</span>
              <div className="playlist-detail-copy">
                <span>{sourceLabels[selected.source_kind].toUpperCase()} · {trackPayload ? `${trackPayload.total} TRACKS` : selected.track_count === null ? "TRACKS ON DEMAND" : `${selected.track_count} TRACKS`}</span>
                {renameOpen ? <form className="playlist-rename-form" onSubmit={renamePlaylist}>
                  <input aria-label="歌单名称" autoFocus required maxLength={120} value={renameName} onChange={(event) => setRenameName(event.target.value)} />
                  <button className="admin-primary" disabled={busy === "rename"}>保存</button>
                  <button type="button" onClick={() => setRenameOpen(false)}>取消</button>
                </form> : <h2>{selected.name}</h2>}
                <p>{selected.description || `${sourceLabels[selected.source_kind]}歌单`}</p>
              </div>
            </div>
            <aside className="playlist-detail-state">
              <span><small>展示状态</small><b>{selected.enabled ? "已启用" : "已停用"}</b></span>
              <label className="toggle" title={selected.enabled ? "点击停用" : "点击启用"}><input type="checkbox" checked={selected.enabled} disabled={busy === `toggle-${selected.id}`} onChange={() => void togglePlaylist(selected)} /><i /></label>
            </aside>
            {trackStatusMessage && <p className="playlist-warning">{trackStatusMessage}</p>}
            <footer className="playlist-detail-actions">
              <div>
                <button type="button" disabled={selectedIndex <= 0 || busy === "order"} onClick={() => void movePlaylist(selectedIndex, -1)}>↑ 上移</button>
                <button type="button" disabled={selectedIndex < 0 || selectedIndex >= payload.items.length - 1 || busy === "order"} onClick={() => void movePlaylist(selectedIndex, 1)}>↓ 下移</button>
                {!renameOpen && <button type="button" onClick={() => { setRenameName(selected.name); setRenameOpen(true); }}>重命名</button>}
                {selected.external_url && <a href={selected.external_url} target="_blank" rel="noreferrer">在原平台打开 ↗</a>}
              </div>
              <div>
                <b className={`playlist-health ${trackStatus}`}>{trackLoading ? "◌ 正在读取" : trackStatus === "ready" ? "● 曲目可用" : "● 暂不可用"}</b>
                <button type="button" className={deleteConfirm === selected.id ? "danger confirm" : "danger"} disabled={busy === `delete-${selected.id}`} onClick={() => void deletePlaylist(selected)}>{deleteConfirm === selected.id ? "再次点击确认删除" : "删除歌单"}</button>
              </div>
            </footer>
          </section>

          {selected.source_kind === "local" && <form className="admin-panel playlist-add-track" onSubmit={addTrack}>
            <header><div><span>ADD TRACK</span><h2>添加歌曲</h2></div><small>仅支持素材库中的音频文件</small></header>
            <div className="playlist-track-form-row">
              <label><span>音频素材 <small>必选</small></span><button className="playlist-asset-open" type="button" onClick={() => { setAssetPage(1); setAssetPickerOpen(true); }}>
                  <i>{selectedAsset ? "♫" : "＋"}</i>
                  <span><b>{selectedAsset?.name || "选择音频素材"}</b><small>{selectedAsset?.file.original_filename || "打开素材库并搜索"}</small></span>
                </button></label>
              <label><span>歌曲名称 <small>可选</small></span><input placeholder="留空使用素材名" maxLength={200} value={trackTitle} onChange={(event) => setTrackTitle(event.target.value)} /></label>
              <label><span>艺人 <small>可选</small></span><input placeholder="输入艺人名称" maxLength={200} value={trackArtist} onChange={(event) => setTrackArtist(event.target.value)} /></label>
              <button className="admin-primary" disabled={!selectedAsset || busy === "track-add"}>{busy === "track-add" ? "添加中…" : "加入歌单"}</button>
            </div>
          </form>}

          <section className="admin-panel playlist-tracks">
            <header>
              <div><span>TRACKS</span><h2>歌曲列表</h2></div>
            </header>
            {!trackLoading && !!trackPayload?.items.length && <div className="playlist-track-head"><span>#</span><span>歌曲</span><span>来源</span><span>操作</span></div>}
            {trackLoading && <div className="playlist-track-loading" role="status"><i /><span>正在读取第 {trackPage} 页歌曲…</span></div>}
            {!trackLoading && trackPayload?.items.map((track, index) => <div key={track.id}>
              <span>{String((trackPayload.page - 1) * trackPayload.per_page + index + 1).padStart(2, "0")}</span>
              <div className="playlist-track-copy"><b>{track.title}</b><small>{track.artist || "未知艺人"}</small></div>
              <div className="playlist-track-actions">
                <span>{sourceLabels[track.source_kind]}</span>
              </div>
              <div className="playlist-track-remove">{selected.source_kind === "local" && <button disabled={busy === `track-${track.id}`} onClick={() => void deleteTrack(track.id)}>移除</button>}</div>
            </div>)}
            {!trackLoading && trackPayload && !trackPayload.items.length && <div className="playlist-empty"><i>♫</i><b>{trackStatus === "unavailable" ? "暂时无法读取曲目" : trackPayload.total ? "这一页没有歌曲" : "歌单还是空的"}</b><span>{trackStatus === "unavailable" ? "请检查外部歌单是否公开，或稍后重试。" : trackPayload.total ? "请返回上一页继续浏览。" : selected.source_kind === "local" ? "从上方素材库选择音频，为歌单添加第一首歌曲。" : "原平台歌单当前没有可播放的公开歌曲。"}</span></div>}
            {!trackLoading && trackPayload && trackPayload.total > 0 && <footer className="playlist-track-pagination" aria-label="歌曲列表分页">
              <span>第 {(trackPayload.page - 1) * trackPayload.per_page + 1}–{Math.min(trackPayload.page * trackPayload.per_page, trackPayload.total)} 首，共 {trackPayload.total} 首</span>
              <div>
                <button type="button" aria-label="第一页" disabled={trackPage <= 1} onClick={() => setTrackPage(1)}>«</button>
                <button type="button" aria-label="上一页" disabled={trackPage <= 1} onClick={() => setTrackPage((page) => Math.max(1, page - 1))}>‹</button>
                {visibleTrackPages.map((page) => <button type="button" key={page} aria-current={page === trackPage ? "page" : undefined} className={page === trackPage ? "active" : ""} onClick={() => setTrackPage(page)}>{page}</button>)}
                <button type="button" aria-label="下一页" disabled={trackPage >= trackPageCount} onClick={() => setTrackPage((page) => Math.min(trackPageCount, page + 1))}>›</button>
                <button type="button" aria-label="最后一页" disabled={trackPage >= trackPageCount} onClick={() => setTrackPage(trackPageCount)}>»</button>
              </div>
            </footer>}
          </section>
        </> : <div className="empty-panel">选择左侧歌单查看详情</div>}
      </div>
    </div>}

    {assetPickerOpen && typeof document !== "undefined" && createPortal(
      <div className="playlist-asset-overlay" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && setAssetPickerOpen(false)}>
        <section className="playlist-asset-dialog" role="dialog" aria-modal="true" aria-labelledby="playlist-asset-title">
          <header>
            <div><h2 id="playlist-asset-title">选择音频素材</h2><p>搜索素材名称或原始文件名。</p></div>
            <button type="button" aria-label="关闭素材选择" onClick={() => setAssetPickerOpen(false)}>×</button>
          </header>
          <label className="playlist-asset-search"><span>⌕</span><input autoFocus aria-label="搜索音频素材" placeholder="输入关键词搜索…" value={assetQuery} onChange={(event) => { setAssetQuery(event.target.value); setAssetPage(1); }} /></label>
          <div className="playlist-asset-results" aria-live="polite">
            {assetLoading && <p>正在读取素材…</p>}
            {!assetLoading && audioAssets.map((asset) => <button type="button" key={asset.id} className={selectedAsset?.id === asset.id ? "selected" : ""} onClick={() => chooseAsset(asset)}>
              <span>♫</span>
              <span><b>{asset.name}</b><small>{asset.file.original_filename || asset.file.mime} · {formatAssetSize(asset.file.size_bytes)}</small></span>
              <i>{selectedAsset?.id === asset.id ? "已选择" : "选择"}</i>
            </button>)}
            {!assetLoading && !audioAssets.length && <p>没有找到匹配的音频素材。</p>}
          </div>
          <footer>
            <span>共 {assetTotal} 条音频素材</span>
            <div>
              <button type="button" disabled={assetPage <= 1 || assetLoading} onClick={() => setAssetPage((page) => page - 1)}>上一页</button>
              <span>{assetPage} / {assetPageCount}</span>
              <button type="button" disabled={assetPage >= assetPageCount || assetLoading} onClick={() => setAssetPage((page) => page + 1)}>下一页</button>
            </div>
          </footer>
        </section>
      </div>,
      document.body,
    )}
  </>;
}
