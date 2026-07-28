"use client";

import Image from "next/image";
import { FormEvent, useCallback, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";

import {
  AdminAsset,
  AssetDetailPayload,
  assetLabels,
  cx,
  formatAssetSize,
  Notify,
  responseMessage,
} from "./shared";

type UploadStatus = "queued" | "uploading" | "success" | "error" | "cancelled";
type UploadTask = {
  id: string;
  file: File;
  status: UploadStatus;
  progress: number;
  error: string;
};

type AssetSort = "created_desc" | "created_asc" | "name_asc" | "name_desc" | "size_desc" | "size_asc";

const assetSortOptions: Array<[AssetSort, string]> = [
  ["created_desc", "最近上传"],
  ["created_asc", "最早上传"],
  ["name_asc", "名称 A–Z"],
  ["name_desc", "名称 Z–A"],
  ["size_desc", "文件从大到小"],
  ["size_asc", "文件从小到大"],
];

const MAX_FILE_BYTES = 200 * 1024 * 1024;
const MAX_CONCURRENT_UPLOADS = 3;

function uploadAssetFile(
  file: File,
  signal: AbortSignal,
  onProgress: (progress: number) => void,
) {
  return new Promise<void>((resolve, reject) => {
    const request = new XMLHttpRequest();
    request.open("POST", "/api/v1/admin/assets");
    request.withCredentials = true;
    request.setRequestHeader("accept", "application/json");
    request.upload.addEventListener("progress", (event) => {
      if (event.lengthComputable) onProgress(Math.round(event.loaded / event.total * 100));
    });
    request.addEventListener("load", () => {
      if (request.status >= 200 && request.status < 300) {
        onProgress(100);
        resolve();
        return;
      }
      try {
        const payload = JSON.parse(request.responseText) as { error?: { message?: string }; message?: string };
        reject(new Error(payload.error?.message || payload.message || `${file.name} 上传失败`));
      } catch {
        reject(new Error(`${file.name} 上传失败（HTTP ${request.status}）`));
      }
    });
    request.addEventListener("error", () => reject(new Error(`${file.name} 上传失败，请检查网络连接`)));
    request.addEventListener("abort", () => reject(new DOMException("上传已取消", "AbortError")));
    const abort = () => request.abort();
    signal.addEventListener("abort", abort, { once: true });
    const form = new FormData();
    form.append("file", file);
    request.send(form);
  });
}

function Modal({
  titleId,
  children,
  onDismiss,
}: {
  titleId: string;
  children: React.ReactNode;
  onDismiss: () => void;
}) {
  if (typeof document === "undefined") return null;
  return createPortal(
    <div className="asset-action-overlay" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onDismiss()}>
      <section role="dialog" aria-modal="true" aria-labelledby={titleId}>{children}</section>
    </div>,
    document.body,
  );
}

export function AssetManager({ notify }: { notify: Notify }) {
  const [assets, setAssets] = useState<AdminAsset[]>([]);
  const [filter, setFilter] = useState<AdminAsset["media_type"] | "all">("all");
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<AssetSort>("created_desc");
  const [selecting, setSelecting] = useState(false);
  const [selectedIds, setSelectedIds] = useState<number[]>([]);
  const [detail, setDetail] = useState<AssetDetailPayload | null>(null);
  const [dragging, setDragging] = useState(false);
  const [loading, setLoading] = useState(true);
  const [operationBusy, setOperationBusy] = useState(false);
  const [page, setPage] = useState(1);
  const [total, setTotal] = useState(0);
  const [uploads, setUploads] = useState<UploadTask[]>([]);
  const [renameOpen, setRenameOpen] = useState(false);
  const [renameName, setRenameName] = useState("");
  const [renameError, setRenameError] = useState("");
  const [batchDeleteOpen, setBatchDeleteOpen] = useState(false);
  const [detailDeleteOpen, setDetailDeleteOpen] = useState(false);
  const uploadInput = useRef<HTMLInputElement>(null);
  const replaceInput = useRef<HTMLInputElement>(null);
  const queuedUploads = useRef<UploadTask[]>([]);
  const activeUploads = useRef(0);
  const uploadControllers = useRef(new Map<string, AbortController>());
  const uploadRefreshTimer = useRef<number | null>(null);
  const pumpUploadsRef = useRef<() => void>(() => undefined);
  const perPage = 20;

  const loadAssets = useCallback(async (signal?: AbortSignal) => {
    setLoading(true);
    const params = new URLSearchParams({ page: String(page), per_page: String(perPage) });
    if (filter !== "all") params.set("media_type", filter);
    if (query.trim()) params.set("search", query.trim());
    const [sortField, sortOrder] = sort.split("_");
    params.set("sort", sortField);
    params.set("order", sortOrder);
    try {
      const response = await fetch(`/api/v1/admin/assets?${params}`, {
        credentials: "include",
        signal,
      });
      if (!response.ok) throw new Error(await responseMessage(response, "素材列表加载失败"));
      const payload = await response.json() as { items: AdminAsset[]; total: number };
      setAssets(payload.items);
      setTotal(payload.total);
      setSelectedIds((ids) => ids.filter((id) => payload.items.some((asset) => asset.id === id)));
    } catch (error) {
      if (!(error instanceof DOMException && error.name === "AbortError")) {
        notify(error instanceof Error ? error.message : "素材列表加载失败", "danger");
      }
    } finally {
      setLoading(false);
    }
  }, [filter, notify, page, query, sort]);

  useEffect(() => {
    const controller = new AbortController();
    const timer = window.setTimeout(() => void loadAssets(controller.signal), 250);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [loadAssets]);

  useEffect(() => () => {
    uploadControllers.current.forEach((controller) => controller.abort());
    if (uploadRefreshTimer.current !== null) window.clearTimeout(uploadRefreshTimer.current);
  }, []);

  const updateUpload = useCallback((id: string, patch: Partial<UploadTask>) => {
    setUploads((items) => items.map((item) => item.id === id ? { ...item, ...patch } : item));
  }, []);

  const scheduleAssetRefresh = useCallback(() => {
    if (uploadRefreshTimer.current !== null) window.clearTimeout(uploadRefreshTimer.current);
    uploadRefreshTimer.current = window.setTimeout(() => {
      uploadRefreshTimer.current = null;
      setPage(1);
      void loadAssets();
    }, 350);
  }, [loadAssets]);

  const pumpUploads = useCallback(() => {
    while (activeUploads.current < MAX_CONCURRENT_UPLOADS && queuedUploads.current.length > 0) {
      const task = queuedUploads.current.shift();
      if (!task) break;
      activeUploads.current += 1;
      const controller = new AbortController();
      uploadControllers.current.set(task.id, controller);
      updateUpload(task.id, { status: "uploading", progress: 0, error: "" });

      void uploadAssetFile(
        task.file,
        controller.signal,
        (progress) => updateUpload(task.id, { progress }),
      )
        .then(() => {
          updateUpload(task.id, { status: "success", progress: 100 });
          scheduleAssetRefresh();
        })
        .catch((error) => {
          if (error instanceof DOMException && error.name === "AbortError") {
            updateUpload(task.id, { status: "cancelled", error: "已取消" });
          } else {
            updateUpload(task.id, {
              status: "error",
              error: error instanceof Error ? error.message : "上传失败",
            });
          }
        })
        .finally(() => {
          activeUploads.current -= 1;
          uploadControllers.current.delete(task.id);
          pumpUploadsRef.current();
        });
    }
  }, [scheduleAssetRefresh, updateUpload]);
  useEffect(() => {
    pumpUploadsRef.current = pumpUploads;
  }, [pumpUploads]);

  const addFiles = (files: FileList | File[]) => {
    const incoming = Array.from(files);
    const accepted = incoming.filter((file) => file.size > 0 && file.size <= MAX_FILE_BYTES);
    const rejected = incoming.length - accepted.length;
    if (!accepted.length) {
      notify("没有可上传的文件；单文件不能超过 200 MB", "danger");
      return;
    }
    const tasks = accepted.map((file, index): UploadTask => ({
      id: `${Date.now()}-${index}-${Math.random().toString(36).slice(2)}`,
      file,
      status: "queued",
      progress: 0,
      error: "",
    }));
    queuedUploads.current.push(...tasks);
    setUploads((items) => [...tasks, ...items]);
    pumpUploadsRef.current();
    if (rejected) notify(`${rejected} 个空文件或超出 200 MB 的文件已跳过`, "danger");
    if (uploadInput.current) uploadInput.current.value = "";
  };

  const cancelUpload = (task: UploadTask) => {
    if (task.status === "queued") {
      queuedUploads.current = queuedUploads.current.filter((candidate) => candidate.id !== task.id);
      updateUpload(task.id, { status: "cancelled", error: "已取消" });
      return;
    }
    uploadControllers.current.get(task.id)?.abort();
  };

  const retryUpload = (task: UploadTask) => {
    if (task.status !== "error" && task.status !== "cancelled") return;
    const queued = { ...task, status: "queued" as const, progress: 0, error: "" };
    updateUpload(task.id, queued);
    queuedUploads.current.push(queued);
    pumpUploadsRef.current();
  };

  const openDetail = async (id: number) => {
    setOperationBusy(true);
    try {
      const response = await fetch(`/api/v1/admin/assets/${id}`, { credentials: "include" });
      if (!response.ok) throw new Error(await responseMessage(response, "素材详情加载失败"));
      setDetail(await response.json() as AssetDetailPayload);
    } catch (error) {
      notify(error instanceof Error ? error.message : "素材详情加载失败", "danger");
    } finally {
      setOperationBusy(false);
    }
  };

  const batchRequest = async (path: "batch-delete" | "batch-download") => fetch(`/api/v1/admin/assets/${path}`, {
    method: "POST",
    credentials: "include",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ asset_ids: selectedIds }),
  });

  const removeAssets = async () => {
    setBatchDeleteOpen(false);
    setOperationBusy(true);
    try {
      const response = await batchRequest("batch-delete");
      if (!response.ok) throw new Error(await responseMessage(response, "批量删除失败"));
      const result = await response.json() as { deleted: number[]; blocked: number[] };
      setSelectedIds([]);
      notify(
        result.blocked.length
          ? `已删除 ${result.deleted.length} 项，${result.blocked.length} 项仍被引用`
          : `已删除 ${result.deleted.length} 项`,
        result.blocked.length ? "normal" : "success",
      );
      await loadAssets();
    } catch (error) {
      notify(error instanceof Error ? error.message : "批量删除失败", "danger");
    } finally {
      setOperationBusy(false);
    }
  };

  const downloadAssets = async () => {
    setOperationBusy(true);
    try {
      const response = await batchRequest("batch-download");
      if (!response.ok) throw new Error(await responseMessage(response, "批量下载失败"));
      const url = URL.createObjectURL(await response.blob());
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = "assets.zip";
      anchor.click();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
    } catch (error) {
      notify(error instanceof Error ? error.message : "批量下载失败", "danger");
    } finally {
      setOperationBusy(false);
    }
  };

  const openRename = () => {
    if (!detail) return;
    setRenameName(detail.asset.name);
    setRenameError("");
    setRenameOpen(true);
  };

  const renameDetail = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!detail) return;
    const name = renameName.trim();
    if (!name) {
      setRenameError("素材名称不能为空。");
      return;
    }
    if (name.length > 255) {
      setRenameError("素材名称不能超过 255 个字符。");
      return;
    }
    if (name === detail.asset.name) {
      setRenameOpen(false);
      return;
    }
    setOperationBusy(true);
    setRenameError("");
    try {
      const response = await fetch(`/api/v1/admin/assets/${detail.asset.id}`, {
        method: "PATCH",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "重命名失败"));
      setRenameOpen(false);
      notify("素材已重命名", "success");
      await Promise.all([openDetail(detail.asset.id), loadAssets()]);
    } catch (error) {
      setRenameError(error instanceof Error ? error.message : "重命名失败");
    } finally {
      setOperationBusy(false);
    }
  };

  const replaceAsset = async (file: File) => {
    if (!detail) return;
    setOperationBusy(true);
    try {
      const form = new FormData();
      form.append("file", file);
      const response = await fetch(`/api/v1/admin/assets/${detail.asset.id}/replace`, {
        method: "POST",
        credentials: "include",
        body: form,
      });
      if (!response.ok) throw new Error(await responseMessage(response, "素材替换失败"));
      notify("素材已替换，旧文件已进入清理队列", "success");
      await Promise.all([openDetail(detail.asset.id), loadAssets()]);
    } catch (error) {
      notify(error instanceof Error ? error.message : "素材替换失败", "danger");
    } finally {
      setOperationBusy(false);
      if (replaceInput.current) replaceInput.current.value = "";
    }
  };

  const deleteDetail = async () => {
    if (!detail) return;
    setDetailDeleteOpen(false);
    setOperationBusy(true);
    try {
      const response = await fetch(`/api/v1/admin/assets/${detail.asset.id}`, {
        method: "DELETE",
        credentials: "include",
      });
      if (!response.ok) throw new Error(await responseMessage(response, "删除失败"));
      setDetail(null);
      notify("素材已删除", "success");
      await loadAssets();
    } catch (error) {
      notify(error instanceof Error ? error.message : "删除失败", "danger");
    } finally {
      setOperationBusy(false);
    }
  };

  useEffect(() => {
    if (!renameOpen && !batchDeleteOpen && !detailDeleteOpen) return;
    const close = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || operationBusy) return;
      setRenameOpen(false);
      setBatchDeleteOpen(false);
      setDetailDeleteOpen(false);
    };
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [batchDeleteOpen, detailDeleteOpen, operationBusy, renameOpen]);

  const toggleSelected = (id: number) => setSelectedIds((items) => items.includes(id)
    ? items.filter((item) => item !== id)
    : [...items, id]);
  const totalPages = Math.max(1, Math.ceil(total / perPage));
  const activeOrPendingUploads = uploads.filter((item) => item.status === "queued" || item.status === "uploading").length;
  const selectedReferenced = assets.filter((asset) => selectedIds.includes(asset.id) && asset.reference_count > 0).length;
  const tabs: Array<[AdminAsset["media_type"] | "all", string]> = [
    ["all", "全部"],
    ["image", "图片"],
    ["audio", "音频"],
    ["video", "视频"],
    ["live2d", "Live2D 模型"],
    ["font", "字体"],
    ["other", "其他"],
  ];

  return (
    <div className="asset-page">
      <div className="admin-title">
        <div><h1>素材库</h1><p>ASSETS · {total} 项</p></div>
        <div className="asset-title-actions">
          <input aria-label="搜索素材" value={query} onChange={(event) => { setQuery(event.target.value); setPage(1); }} placeholder="⌕ 搜索文件名…" />
          <button onClick={() => { setSelecting((value) => !value); setSelectedIds([]); }}>{selecting ? "完成" : "☐ 选择"}</button>
          <button className="admin-primary" onClick={() => uploadInput.current?.click()}>↑ 上传素材</button>
          <input ref={uploadInput} type="file" multiple hidden onChange={(event) => event.target.files && addFiles(event.target.files)} />
        </div>
      </div>
      <div className="asset-tabs">
        {tabs.map(([value, label]) => <button key={value} className={filter === value ? "active" : ""} onClick={() => { setFilter(value); setPage(1); }}>{label}{filter === value ? ` ${total}` : ""}</button>)}
        <label className="asset-sort">排列<select aria-label="素材排列方式" value={sort} onChange={(event) => { setSort(event.target.value as AssetSort); setPage(1); }}>{assetSortOptions.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
      </div>
      <div
        className={cx("asset-dropzone", dragging && "dragging")}
        onDragEnter={(event) => { event.preventDefault(); setDragging(true); }}
        onDragOver={(event) => event.preventDefault()}
        onDragLeave={() => setDragging(false)}
        onDrop={(event) => { event.preventDefault(); setDragging(false); addFiles(event.dataTransfer.files); }}
        onClick={() => uploadInput.current?.click()}
        role="button"
        tabIndex={0}
        onKeyDown={(event) => (event.key === "Enter" || event.key === " ") && uploadInput.current?.click()}
      >
        ⇩ 拖拽文件到此处上传
        <span>最多同时上传 {MAX_CONCURRENT_UPLOADS} 个 · 单文件 ≤ 200 MB</span>
      </div>

      {uploads.length > 0 && <section className="asset-upload-queue" aria-label="上传队列" aria-live="polite">
        <header><div><b>上传队列</b><span>{activeOrPendingUploads ? `${activeOrPendingUploads} 项处理中` : "全部处理完成"}</span></div><button type="button" onClick={() => setUploads((items) => items.filter((item) => item.status === "queued" || item.status === "uploading"))}>清除已完成</button></header>
        <div>{uploads.map((task) => <article key={task.id} className={`upload-${task.status}`}>
          <div className="asset-upload-file"><b>{task.file.name}</b><small>{formatAssetSize(task.file.size)} · {task.status === "queued" ? "等待上传" : task.status === "uploading" ? `${task.progress}%` : task.status === "success" ? "上传完成" : task.error}</small></div>
          <progress max={100} value={task.progress} aria-label={`${task.file.name} 上传进度`} />
          {(task.status === "queued" || task.status === "uploading") && <button type="button" onClick={() => cancelUpload(task)}>取消</button>}
          {(task.status === "error" || task.status === "cancelled") && <button type="button" onClick={() => retryUpload(task)}>重试</button>}
        </article>)}</div>
      </section>}

      {selecting && <div className="asset-batchbar">
        <b>已选择 {selectedIds.length} 项</b>
        <span>{selectedReferenced ? `${selectedReferenced} 项仍被引用，将保留` : "业务正在引用的素材不能删除"}</span>
        <button disabled={!selectedIds.length || operationBusy} onClick={() => void downloadAssets()}>⇩ 批量下载</button>
        <button disabled={!selectedIds.length || operationBusy} onClick={() => setBatchDeleteOpen(true)}>删除可删项</button>
      </div>}
      {loading && <div className="empty-panel">正在读取素材库…</div>}
      {!loading && <div className="asset-grid">{assets.map((asset) => {
        const label = assetLabels[asset.media_type];
        const preview = asset.media_type === "image" ? asset.file.url : undefined;
        return <button key={asset.id} className={cx("asset-card", selectedIds.includes(asset.id) && "selected")} onClick={() => selecting ? toggleSelected(asset.id) : void openDetail(asset.id)}>
          <div className={cx("asset-preview", `asset-${label.toLowerCase()}`)} style={preview ? { backgroundImage: `url("${preview}")` } : undefined}>
            <span>{label}</span>{selecting && <i>{selectedIds.includes(asset.id) ? "✓" : ""}</i>}
            {!preview && <b>{asset.media_type === "live2d" ? "L2D" : asset.media_type === "audio" ? "▥▥▥" : asset.media_type === "video" ? "▶" : asset.media_type === "font" ? "Aa" : "FILE"}</b>}
          </div>
          <div><b>{asset.name}</b><small>{label} · {formatAssetSize(asset.file.size_bytes)}</small></div>
        </button>;
      })}</div>}
      {!loading && !assets.length && <div className="empty-panel">没有符合当前筛选的素材。</div>}
      <div className="asset-footer"><div><button disabled={page === 1} onClick={() => setPage((value) => value - 1)}>‹</button><button className="active">{page} / {totalPages}</button><button disabled={page >= totalPages} onClick={() => setPage((value) => value + 1)}>›</button></div></div>

      {detail && typeof document !== "undefined" && createPortal(
        <div className="asset-detail-overlay" role="presentation" onClick={() => !operationBusy && setDetail(null)}>
          <section role="dialog" aria-modal="true" aria-label="素材详情" onClick={(event) => event.stopPropagation()}>
            <header><div><span>ASSET DETAIL</span><h2>{detail.asset.name}</h2></div><button aria-label="关闭素材详情" onClick={() => setDetail(null)}>×</button></header>
            {detail.asset.media_type === "image"
              ? <a className="asset-detail-preview asset-detail-image" href={detail.asset.file.url} target="_blank" rel="noreferrer" title="点击查看原图"><Image src={detail.asset.file.url} width={1200} height={800} sizes="(max-width: 660px) 100vw, 670px" unoptimized alt={`${detail.asset.name} 预览`} /></a>
              : detail.asset.media_type === "audio"
                ? <div className="asset-detail-preview asset-detail-audio"><audio controls preload="metadata" src={detail.asset.file.url} aria-label={`${detail.asset.name} 音频预览`}>当前浏览器不支持音频预览。</audio></div>
                : <div className={cx("asset-detail-preview", `asset-${assetLabels[detail.asset.media_type].toLowerCase()}`)}><b>{detail.asset.media_type === "live2d" ? "L2D" : detail.asset.media_type === "video" ? "▶" : "FILE"}</b></div>}
            <dl><div><dt>素材类型</dt><dd>{assetLabels[detail.asset.media_type]}</dd></div><div><dt>文件大小</dt><dd>{formatAssetSize(detail.asset.file.size_bytes)}</dd></div></dl>
            <div className="asset-detail-list"><b>引用位置</b>{detail.references.length > 0 ? detail.references.map((item) => <span className="asset-reference-tag" key={`${item.source_type}-${item.source_id}`}>{item.source_label}</span>) : <span>当前未被引用</span>}</div>
            <footer><button disabled={operationBusy} onClick={openRename}>重命名</button><button disabled={operationBusy} onClick={() => replaceInput.current?.click()}>替换素材</button><input ref={replaceInput} type="file" hidden onChange={(event) => event.target.files?.[0] && void replaceAsset(event.target.files[0])} /><button disabled={operationBusy || detail.references.length > 0} onClick={() => setDetailDeleteOpen(true)}>删除素材</button></footer>
          </section>
        </div>,
        document.body,
      )}

      {renameOpen && <Modal titleId="rename-asset-title" onDismiss={() => !operationBusy && setRenameOpen(false)}>
        <form onSubmit={renameDetail}>
          <header><div><span>ASSET / RENAME</span><h2 id="rename-asset-title">重命名素材</h2></div><button type="button" aria-label="关闭重命名" onClick={() => setRenameOpen(false)}>×</button></header>
          <label htmlFor="rename-asset-input">素材名称</label>
          <input id="rename-asset-input" autoFocus maxLength={255} value={renameName} onChange={(event) => setRenameName(event.target.value)} aria-describedby={renameError ? "rename-asset-error" : undefined} aria-invalid={Boolean(renameError)} />
          {renameError && <p id="rename-asset-error" className="admin-account-error" role="alert">! {renameError}</p>}
          <footer><button type="button" onClick={() => setRenameOpen(false)}>取消</button><button className="admin-primary" disabled={operationBusy}>{operationBusy ? "正在保存…" : "保存名称"}</button></footer>
        </form>
      </Modal>}

      {batchDeleteOpen && <Modal titleId="batch-delete-title" onDismiss={() => !operationBusy && setBatchDeleteOpen(false)}>
        <header><div><span>ASSET / DELETE</span><h2 id="batch-delete-title">确认批量删除</h2></div><button type="button" aria-label="关闭批量删除确认" onClick={() => setBatchDeleteOpen(false)}>×</button></header>
        <p>将删除所选 {selectedIds.length} 项中的可删除素材。{selectedReferenced ? `其中 ${selectedReferenced} 项仍被引用，会自动保留。` : "此操作不能撤销。"}</p>
        <footer><button type="button" onClick={() => setBatchDeleteOpen(false)}>取消</button><button className="danger" type="button" onClick={() => void removeAssets()}>确认删除</button></footer>
      </Modal>}

      {detailDeleteOpen && detail && <Modal titleId="detail-delete-title" onDismiss={() => !operationBusy && setDetailDeleteOpen(false)}>
        <header><div><span>ASSET / DELETE</span><h2 id="detail-delete-title">删除素材</h2></div><button type="button" aria-label="关闭删除确认" onClick={() => setDetailDeleteOpen(false)}>×</button></header>
        <p>确定删除“{detail.asset.name}”？对象存储文件会由后台清理，此操作不能撤销。</p>
        <footer><button type="button" onClick={() => setDetailDeleteOpen(false)}>取消</button><button className="danger" type="button" onClick={() => void deleteDetail()}>确认删除</button></footer>
      </Modal>}
    </div>
  );
}
