"use client";

import Image from "next/image";
import Link from "next/link";
import { FormEvent, useCallback, useEffect, useState } from "react";

import { AdminAsset, Notify, responseMessage } from "./shared";

type ReviewSection = "comments" | "friends";
type CommentFilter = "all" | "pending" | "approved";
type FriendStatus = "pending" | "approved" | "rejected";
type FriendFilter = "all" | FriendStatus;

type ModeratedComment = {
  id: number;
  rid: number;
  content: string;
  date: string;
  nick: string;
  page_key: string;
  page_url: string;
  ua: string;
  ip_region: string;
  is_pending: boolean;
  is_collapsed: boolean;
  is_pinned: boolean;
  visible: boolean;
  vote_up: number;
  vote_down: number;
};

type CommentListPayload = {
  page: number;
  per_page: number;
  total: number;
  counts: Record<CommentFilter, number>;
  items: ModeratedComment[];
};

type FriendApplication = {
  id: number;
  name: string;
  url: string;
  avatar_url: string;
  avatar_asset_id: number | null;
  avatar_asset_url: string | null;
  contact_email: string;
  description: string;
  status: FriendStatus;
  sort_order: number;
  created_at: string;
  updated_at: string;
  reviewed_at: string | null;
};

type FriendListPayload = {
  page: number;
  per_page: number;
  total: number;
  counts: Record<FriendStatus, number>;
  items: FriendApplication[];
};

type AssetListPayload = {
  items: AdminAsset[];
};

const statusLabels: Record<FriendStatus, string> = {
  pending: "待审核",
  approved: "已通过",
  rejected: "已拒绝",
};

const filterLabels: Record<FriendFilter, string> = {
  all: "全部",
  pending: "待审核",
  approved: "已通过",
  rejected: "已拒绝",
};

const commentFilterLabels: Record<CommentFilter, string> = {
  all: "全部",
  pending: "待审核",
  approved: "已通过",
};

function localDate(value: string) {
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(new Date(value));
}

export function ReviewManager({ notify }: { notify: Notify }) {
  const [section, setSection] = useState<ReviewSection>("comments");
  const [commentFilter, setCommentFilter] = useState<CommentFilter>("pending");
  const [commentSearchDraft, setCommentSearchDraft] = useState("");
  const [commentSearch, setCommentSearch] = useState("");
  const [commentPage, setCommentPage] = useState(1);
  const [commentPayload, setCommentPayload] = useState<CommentListPayload>({
    page: 1,
    per_page: 20,
    total: 0,
    counts: { all: 0, pending: 0, approved: 0 },
    items: [],
  });
  const [commentLoading, setCommentLoading] = useState(true);
  const [commentError, setCommentError] = useState("");
  const [commentBusyId, setCommentBusyId] = useState<number | null>(null);
  const [commentDeleteConfirmation, setCommentDeleteConfirmation] = useState<number | null>(null);
  const [filter, setFilter] = useState<FriendFilter>("pending");
  const [searchDraft, setSearchDraft] = useState("");
  const [search, setSearch] = useState("");
  const [page, setPage] = useState(1);
  const [payload, setPayload] = useState<FriendListPayload>({
    page: 1,
    per_page: 20,
    total: 0,
    counts: { pending: 0, approved: 0, rejected: 0 },
    items: [],
  });
  const [assets, setAssets] = useState<AdminAsset[]>([]);
  const [assetSelections, setAssetSelections] = useState<Record<number, number>>({});
  const [loading, setLoading] = useState(true);
  const [assetsLoading, setAssetsLoading] = useState(true);
  const [error, setError] = useState("");
  const [busyId, setBusyId] = useState<number | null>(null);
  const [deleteConfirmation, setDeleteConfirmation] = useState<number | null>(null);

  const loadComments = useCallback(async () => {
    setCommentLoading(true);
    setCommentError("");
    const params = new URLSearchParams({
      page: String(commentPage),
      per_page: "20",
      status: commentFilter,
    });
    if (commentSearch) params.set("search", commentSearch);
    try {
      const response = await fetch(`/api/v1/admin/comments?${params}`, {
        credentials: "include",
        headers: { accept: "application/json" },
      });
      if (!response.ok) throw new Error(await responseMessage(response, "读取评论审核队列失败"));
      const next = await response.json() as CommentListPayload;
      setCommentPayload(next);
    } catch (loadError) {
      setCommentError(loadError instanceof Error ? loadError.message : "读取评论审核队列失败");
    } finally {
      setCommentLoading(false);
    }
  }, [commentFilter, commentPage, commentSearch]);

  const loadFriends = useCallback(async () => {
    setLoading(true);
    setError("");
    const params = new URLSearchParams({
      page: String(page),
      per_page: "20",
    });
    if (filter !== "all") params.set("status", filter);
    if (search) params.set("search", search);
    try {
      const response = await fetch(`/api/v1/admin/friends?${params}`, {
        credentials: "include",
        headers: { accept: "application/json" },
      });
      if (!response.ok) throw new Error(await responseMessage(response, "读取友链审核队列失败"));
      const next = await response.json() as FriendListPayload;
      setPayload(next);
      setAssetSelections((current) => {
        const updated = { ...current };
        for (const item of next.items) {
          if (item.avatar_asset_id && !updated[item.id]) {
            updated[item.id] = item.avatar_asset_id;
          }
        }
        return updated;
      });
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "读取友链审核队列失败");
    } finally {
      setLoading(false);
    }
  }, [filter, page, search]);

  const loadAssets = useCallback(async () => {
    setAssetsLoading(true);
    try {
      const response = await fetch("/api/v1/admin/assets?usable_for=friend_avatar&per_page=100", {
        credentials: "include",
        headers: { accept: "application/json" },
      });
      if (!response.ok) throw new Error(await responseMessage(response, "读取头像素材失败"));
      const next = await response.json() as AssetListPayload;
      setAssets(next.items);
    } catch (loadError) {
      setError(loadError instanceof Error ? loadError.message : "读取头像素材失败");
    } finally {
      setAssetsLoading(false);
    }
  }, []);

  useEffect(() => {
    if (section !== "comments") return;
    const timer = window.setTimeout(() => void loadComments(), 0);
    return () => window.clearTimeout(timer);
  }, [loadComments, section]);

  useEffect(() => {
    const timer = window.setTimeout(() => void loadFriends(), 0);
    return () => window.clearTimeout(timer);
  }, [loadFriends]);

  useEffect(() => {
    const timer = window.setTimeout(() => void loadAssets(), 0);
    return () => window.clearTimeout(timer);
  }, [loadAssets]);

  const chooseFilter = (next: FriendFilter) => {
    setFilter(next);
    setPage(1);
    setDeleteConfirmation(null);
  };

  const chooseCommentFilter = (next: CommentFilter) => {
    setCommentFilter(next);
    setCommentPage(1);
    setCommentDeleteConfirmation(null);
  };

  const submitCommentSearch = (event: FormEvent) => {
    event.preventDefault();
    setCommentSearch(commentSearchDraft.trim());
    setCommentPage(1);
  };

  const updateCommentStatus = async (item: ModeratedComment, status: Exclude<CommentFilter, "all">) => {
    setCommentBusyId(item.id);
    setCommentError("");
    try {
      const response = await fetch(`/api/v1/admin/comments/${item.id}`, {
        method: "PATCH",
        credentials: "include",
        headers: { "content-type": "application/json", accept: "application/json" },
        body: JSON.stringify({ status }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "更新评论审核状态失败"));
      notify(`“${item.nick || "匿名访客"}”的评论已${status === "approved" ? "通过" : "移回待审核"}`, "success");
      await loadComments();
    } catch (updateError) {
      setCommentError(updateError instanceof Error ? updateError.message : "更新评论审核状态失败");
    } finally {
      setCommentBusyId(null);
    }
  };

  const deleteComment = async (item: ModeratedComment) => {
    setCommentBusyId(item.id);
    setCommentError("");
    try {
      const response = await fetch(`/api/v1/admin/comments/${item.id}`, {
        method: "DELETE",
        credentials: "include",
        headers: { accept: "application/json" },
      });
      if (!response.ok) throw new Error(await responseMessage(response, "删除评论失败"));
      notify(`已删除“${item.nick || "匿名访客"}”的评论`, "success");
      setCommentDeleteConfirmation(null);
      await loadComments();
    } catch (deleteError) {
      setCommentError(deleteError instanceof Error ? deleteError.message : "删除评论失败");
    } finally {
      setCommentBusyId(null);
    }
  };

  const submitSearch = (event: FormEvent) => {
    event.preventDefault();
    setSearch(searchDraft.trim());
    setPage(1);
  };

  const updateStatus = async (item: FriendApplication, status: FriendStatus) => {
    const avatarAssetId = assetSelections[item.id] || item.avatar_asset_id;
    if (status === "approved" && !avatarAssetId) {
      setError("通过申请前，请先选择一张已上传到素材库的头像。");
      return;
    }
    setBusyId(item.id);
    setError("");
    try {
      const response = await fetch(`/api/v1/admin/friends/${item.id}`, {
        method: "PATCH",
        credentials: "include",
        headers: { "content-type": "application/json", accept: "application/json" },
        body: JSON.stringify({
          status,
          ...(status === "approved" ? { avatar_asset_id: avatarAssetId } : {}),
        }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "更新审核状态失败"));
      notify(`“${item.name}”已${status === "approved" ? "通过" : status === "rejected" ? "拒绝" : "移回待审核"}`, "success");
      await loadFriends();
    } catch (updateError) {
      setError(updateError instanceof Error ? updateError.message : "更新审核状态失败");
    } finally {
      setBusyId(null);
    }
  };

  const deleteFriend = async (item: FriendApplication) => {
    setBusyId(item.id);
    setError("");
    try {
      const response = await fetch(`/api/v1/admin/friends/${item.id}`, {
        method: "DELETE",
        credentials: "include",
        headers: { accept: "application/json" },
      });
      if (!response.ok) throw new Error(await responseMessage(response, "删除友链记录失败"));
      notify(`已删除“${item.name}”`, "success");
      setDeleteConfirmation(null);
      await loadFriends();
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "删除友链记录失败");
    } finally {
      setBusyId(null);
    }
  };

  const pageCount = Math.max(1, Math.ceil(payload.total / payload.per_page));
  const commentPageCount = Math.max(1, Math.ceil(commentPayload.total / commentPayload.per_page));

  return <>
    <div className="admin-title review-title">
      <div><h1>审核</h1><p>REVIEW · COMMENTS & FRIEND LINKS</p></div>
      <button className="admin-primary" type="button" onClick={() => section === "comments" ? void loadComments() : void loadFriends()}>
        {section === "comments" ? "刷新评论" : "刷新申请"}
      </button>
    </div>

    <nav className="review-section-switch" aria-label="审核内容">
      <button type="button" className={section === "comments" ? "active" : ""} aria-pressed={section === "comments"} onClick={() => setSection("comments")}>
        <span>◫</span><b>评论</b><small>{commentPayload.counts.pending} 条待审核</small>
        {commentPayload.counts.pending > 0 && <i>{commentPayload.counts.pending}</i>}
      </button>
      <button type="button" className={section === "friends" ? "active" : ""} aria-pressed={section === "friends"} onClick={() => setSection("friends")}>
        <span>◇</span><b>友链申请</b><small>{payload.counts.pending} 条待审核</small>
        {payload.counts.pending > 0 && <i>{payload.counts.pending}</i>}
      </button>
    </nav>

    {section === "comments" ? (
      <section className="comment-review" aria-label="评论审核">
        <div className="friend-review-toolbar comment-review-toolbar">
          <div role="group" aria-label="评论审核状态">
            {(Object.keys(commentFilterLabels) as CommentFilter[]).map((value) => (
              <button type="button" key={value} className={commentFilter === value ? "active" : ""} onClick={() => chooseCommentFilter(value)}>
                {commentFilterLabels[value]} <span>{commentPayload.counts[value]}</span>
              </button>
            ))}
          </div>
          <form onSubmit={submitCommentSearch}>
            <input aria-label="搜索评论" value={commentSearchDraft} onChange={(event) => setCommentSearchDraft(event.target.value)} placeholder="作者、内容或文章路径" />
            <button type="submit">搜索</button>
          </form>
        </div>

        {commentError && <div className="friend-review-error" role="alert">{commentError}</div>}
        {commentLoading ? (
          <div className="friend-review-empty" role="status">正在读取评论审核队列…</div>
        ) : commentPayload.items.length === 0 ? (
          <div className="friend-review-empty"><b>队列为空</b><p>{commentSearch ? "没有符合搜索条件的评论。" : `${commentFilterLabels[commentFilter]}列表中暂时没有评论。`}</p></div>
        ) : (
          <div className="comment-review-list">
            {commentPayload.items.map((item) => <article className="comment-review-card" key={item.id}>
              <header>
                <div className="comment-review-avatar" aria-hidden="true">{(item.nick || "?").slice(0, 1).toUpperCase()}</div>
                <div>
                  <span className={`friend-review-status ${item.is_pending ? "pending" : "approved"}`}>{item.is_pending ? "待审核" : "已通过"}</span>
                  <h2>{item.nick || "匿名访客"}</h2>
                  <small>{item.page_key || "未知页面"}{item.rid > 0 ? ` · 回复 #${item.rid}` : ""}</small>
                </div>
                <time dateTime={item.date}>{localDate(item.date)}</time>
              </header>
              <p className="comment-review-content">{item.content}</p>
              <dl>
                {item.ip_region && <div><dt>地区</dt><dd>{item.ip_region}</dd></div>}
                {item.ua && <div><dt>设备</dt><dd>{item.ua}</dd></div>}
                <div><dt>赞同</dt><dd>{item.vote_up}</dd></div>
                {item.is_pinned && <div><dt>标记</dt><dd>已置顶</dd></div>}
                {item.is_collapsed && <div><dt>显示</dt><dd>已折叠</dd></div>}
              </dl>
              <footer>
                <div>
                  {item.is_pending
                    ? <button type="button" className="approve" disabled={commentBusyId === item.id} onClick={() => void updateCommentStatus(item, "approved")}>✓ 通过</button>
                    : <button type="button" disabled={commentBusyId === item.id} onClick={() => void updateCommentStatus(item, "pending")}>↶ 移回待审核</button>}
                </div>
                {commentDeleteConfirmation === item.id ? (
                  <div className="friend-review-delete-confirm">
                    <span>确定永久删除？</span>
                    <button type="button" disabled={commentBusyId === item.id} onClick={() => void deleteComment(item)}>确认</button>
                    <button type="button" onClick={() => setCommentDeleteConfirmation(null)}>取消</button>
                  </div>
                ) : (
                  <button type="button" className="comment-review-delete" disabled={commentBusyId === item.id} onClick={() => setCommentDeleteConfirmation(item.id)}>删除评论</button>
                )}
              </footer>
            </article>)}
          </div>
        )}

        {commentPageCount > 1 && <nav className="admin-article-pagination" aria-label="评论分页">
          <button type="button" disabled={commentPage <= 1 || commentLoading} onClick={() => setCommentPage((current) => Math.max(1, current - 1))}>上一页</button>
          <span>{commentPage} / {commentPageCount}</span>
          <button type="button" disabled={commentPage >= commentPageCount || commentLoading} onClick={() => setCommentPage((current) => Math.min(commentPageCount, current + 1))}>下一页</button>
        </nav>}
      </section>
    ) : (
      <section className="friend-review">
        <div className="friend-review-toolbar">
          <div role="group" aria-label="友链审核状态">
            {(Object.keys(filterLabels) as FriendFilter[]).map((value) => {
              const count = value === "all"
                ? payload.counts.pending + payload.counts.approved + payload.counts.rejected
                : payload.counts[value];
              return <button type="button" key={value} className={filter === value ? "active" : ""} onClick={() => chooseFilter(value)}>
                {filterLabels[value]} <span>{count}</span>
              </button>;
            })}
          </div>
          <form onSubmit={submitSearch}>
            <input aria-label="搜索友链申请" value={searchDraft} onChange={(event) => setSearchDraft(event.target.value)} placeholder="站点、网址或邮箱" />
            <button type="submit">搜索</button>
          </form>
        </div>

        {error && <div className="friend-review-error" role="alert">{error}</div>}
        {loading ? (
          <div className="friend-review-empty" role="status">正在读取审核队列…</div>
        ) : payload.items.length === 0 ? (
          <div className="friend-review-empty"><b>队列为空</b><p>{search ? "没有符合搜索条件的申请。" : `${filterLabels[filter]}列表中暂时没有记录。`}</p></div>
        ) : (
          <div className="friend-review-list">
            {payload.items.map((item) => {
              const selectedAsset = assets.find((asset) => asset.id === (assetSelections[item.id] || item.avatar_asset_id));
              return <article className="friend-review-card" key={item.id}>
                <header>
                  <div className="friend-review-avatar">
                    {item.avatar_asset_url
                      ? <Image src={item.avatar_asset_url} width={54} height={54} unoptimized alt="" />
                      : <span>{item.name.slice(0, 1).toUpperCase()}</span>}
                  </div>
                  <div>
                    <span className={`friend-review-status ${item.status}`}>{statusLabels[item.status]}</span>
                    <h2>{item.name}</h2>
                    <a href={item.url} target="_blank" rel="noreferrer">{item.url} ↗</a>
                  </div>
                  <time dateTime={item.created_at}>{localDate(item.created_at)} 提交</time>
                </header>
                <p className="friend-review-description">{item.description || "申请人未填写站点介绍。"}</p>
                <dl>
                  <div><dt>联系邮箱</dt><dd><a href={`mailto:${item.contact_email}`}>{item.contact_email}</a></dd></div>
                  <div><dt>头像来源</dt><dd>{item.avatar_url ? <a href={item.avatar_url} target="_blank" rel="noreferrer">查看申请头像 ↗</a> : "未提供"}</dd></div>
                  {item.reviewed_at && <div><dt>最近审核</dt><dd>{localDate(item.reviewed_at)}</dd></div>}
                </dl>
                <div className="friend-review-asset">
                  <label>
                    <span>发布头像素材</span>
                    <select
                      value={assetSelections[item.id] || item.avatar_asset_id || ""}
                      disabled={assetsLoading || busyId === item.id}
                      onChange={(event) => setAssetSelections((current) => ({ ...current, [item.id]: Number(event.target.value) }))}
                    >
                      <option value="">{assetsLoading ? "正在读取图片素材…" : "选择素材库中的图片"}</option>
                      {assets.map((asset) => <option key={asset.id} value={asset.id}>{asset.name}</option>)}
                    </select>
                  </label>
                  {selectedAsset && <span><Image src={selectedAsset.file.url} width={34} height={34} unoptimized alt="" />{selectedAsset.name}</span>}
                  <Link href="/admin/assets">去素材库上传 →</Link>
                </div>
                <footer>
                  <div>
                    {item.status !== "approved" && <button type="button" className="approve" disabled={busyId === item.id} onClick={() => void updateStatus(item, "approved")}>✓ 通过</button>}
                    {item.status !== "rejected" && <button type="button" className="reject" disabled={busyId === item.id} onClick={() => void updateStatus(item, "rejected")}>× 拒绝</button>}
                    {item.status !== "pending" && <button type="button" disabled={busyId === item.id} onClick={() => void updateStatus(item, "pending")}>↶ 移回待审核</button>}
                  </div>
                  {deleteConfirmation === item.id ? (
                    <div className="friend-review-delete-confirm">
                      <span>确定永久删除？</span>
                      <button type="button" disabled={busyId === item.id} onClick={() => void deleteFriend(item)}>确认</button>
                      <button type="button" onClick={() => setDeleteConfirmation(null)}>取消</button>
                    </div>
                  ) : (
                    <button type="button" className="friend-review-delete" disabled={busyId === item.id} onClick={() => setDeleteConfirmation(item.id)}>删除记录</button>
                  )}
                </footer>
              </article>;
            })}
          </div>
        )}

        {pageCount > 1 && <nav className="admin-article-pagination" aria-label="友链申请分页">
          <button type="button" disabled={page <= 1 || loading} onClick={() => setPage((current) => Math.max(1, current - 1))}>上一页</button>
          <span>{page} / {pageCount}</span>
          <button type="button" disabled={page >= pageCount || loading} onClick={() => setPage((current) => Math.min(pageCount, current + 1))}>下一页</button>
        </nav>}
      </section>
    )}
  </>;
}
