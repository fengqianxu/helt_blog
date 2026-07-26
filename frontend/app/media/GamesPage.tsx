"use client";

import Image from "next/image";

import { cx } from "../admin/shared";
import { GameRecords } from "./hooks";
import { Pagination } from "./Pagination";

function playtime(minutes: number) {
  if (minutes <= 0) return "尚未游玩";
  if (minutes < 60) return `${minutes} 分钟`;
  const hours = minutes / 60;
  return `${hours < 100 ? hours.toFixed(1) : Math.round(hours)} 小时`;
}

function lastPlayed(value: string | null) {
  return value ? new Date(value).toLocaleDateString("zh-CN") : "暂无记录";
}

export default function GamesPage({ records }: { records: GameRecords }) {
  const {
    items,
    meta,
    page,
    pageCount,
    range,
    setPage,
    setRange,
    setSort,
    sort,
    state,
    total,
  } = records;
  const changePage = (nextPage: number) => {
    setPage(Math.min(pageCount, Math.max(1, nextPage)));
    document.querySelector(".view-tabs")?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  return (
    <>
      <div className="bangumi-filters" aria-label="Steam 游戏筛选、排序与同步状态">
        <button className={cx("game-count", range === "all" && "active")} aria-pressed={range === "all"} onClick={() => { setRange("all"); setPage(1); }}>游戏库 {meta.counts.total}</button>
        <button className={cx("game-count recent", range === "recent" && "active")} aria-pressed={range === "recent"} onClick={() => { setRange("recent"); setPage(1); }}>最近两周 {meta.counts.recent}</button>
        <label className="game-sort-control">排序<select aria-label="游戏排序" value={sort} onChange={(event) => { setSort(event.target.value as "recent" | "playtime"); setPage(1); }}><option value="recent">最近游玩</option><option value="playtime">累计时长</option></select></label>
        {meta.sync_status === "error" && total > 0 && <span className="sync-warning" role="status">同步异常，正在展示上次数据</span>}
        {meta.synced_at && <time dateTime={meta.synced_at}>同步于 {new Date(meta.synced_at).toLocaleDateString("zh-CN")}</time>}
      </div>
      {state === "loading" ? <div className="media-empty"><b>STEAM</b><p>正在读取第 {page} 页游戏进程…</p></div>
        : state === "error" ? <div className="media-empty error"><b>OFFLINE</b><p>Steam 游戏数据暂时无法读取，请稍后再试。</p></div>
          : !meta.configured ? <div className="media-empty"><b>STEAM</b><p>站主尚未配置公开游戏记录。</p></div>
            : items.length === 0 && meta.sync_status === "queued" ? <div className="media-empty"><b>SYNC</b><p>Steam 游戏库正在同步，稍后再来看看。</p></div>
              : items.length === 0 && meta.sync_status === "error" ? <div className="media-empty error"><b>OFFLINE</b><p>Steam 同步暂时失败，站主修复后会恢复展示。</p></div>
                : items.length === 0 ? <div className="media-empty"><b>EMPTY</b><p>Steam 暂未返回可公开展示的游戏记录。</p></div>
                  : <>
                    <div className="media-grid">
                      {items.map((item) => <a className="media-card game-card" key={item.steam_app_id} href={item.steam_url} target="_blank" rel="noreferrer"><span className="media-cover has-image steam-cover"><Image src={item.cover_url} fill sizes="(max-width: 720px) 112px, 150px" unoptimized alt="" onError={(event) => { if (item.icon_url && event.currentTarget.src !== item.icon_url) event.currentTarget.src = item.icon_url; }} /><span>STEAM</span></span><span className="media-copy"><span className={cx("status", item.playtime_2weeks_minutes === 0 && "finished")}>{item.playtime_2weeks_minutes > 0 ? "● 最近在玩" : "◆ 游戏库"}</span><h2>{item.title}</h2><p>累计 {playtime(item.playtime_forever_minutes)}{item.playtime_2weeks_minutes > 0 ? ` · 最近两周 ${playtime(item.playtime_2weeks_minutes)}` : ""}</p><span className="game-hours">最后游玩 {lastPlayed(item.last_played_at)}</span><small>前往 Steam 官网 ↗</small></span></a>)}
                    </div>
                    <Pagination current={page} count={pageCount} total={total} unit="款" label="游戏分页" onChange={changePage} />
                  </>}
    </>
  );
}
