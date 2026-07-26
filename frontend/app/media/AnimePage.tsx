"use client";

import Image from "next/image";

import { cx } from "../admin/shared";
import { BangumiItem } from "./api";
import { BangumiRecords } from "./hooks";
import { Pagination } from "./Pagination";

function animeProgress(item: BangumiItem) {
  if (item.status === "finished") return item.ep_total > 0 ? `全 ${item.ep_total} 话` : "已标记看完";
  if (item.ep_current > 0) return `看到 ${item.ep_current}${item.ep_total > 0 ? ` / ${item.ep_total}` : ""} 话`;
  if (item.latest_episode) return `作品${item.latest_episode}`;
  return "观看进度未公开";
}

export default function AnimePage({ records }: { records: BangumiRecords }) {
  const { items, meta, page, pageCount, setPage, state, total } = records;
  const changePage = (nextPage: number) => {
    setPage(Math.min(pageCount, Math.max(1, nextPage)));
    document.querySelector(".view-tabs")?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  return (
    <>
      {meta.sync_status === "error" && items.length > 0 && <div className="sync-warning media-sync-warning" role="status">追番同步异常，正在展示上次数据</div>}
      {state === "loading" ? <div className="media-empty"><b>SYNC</b><p>正在读取第 {page} 页追番记录…</p></div>
        : state === "error" ? <div className="media-empty error"><b>OFFLINE</b><p>追番数据暂时无法读取，请稍后再试。</p></div>
          : !meta.configured ? <div className="media-empty"><b>UID</b><p>站主尚未配置公开追番记录。</p></div>
            : items.length === 0 && meta.sync_status === "queued" ? <div className="media-empty"><b>SYNC</b><p>追番记录正在同步，稍后再来看看。</p></div>
              : items.length === 0 && meta.sync_status === "error" ? <div className="media-empty error"><b>OFFLINE</b><p>追番同步暂时失败，站主修复后会恢复展示。</p></div>
                : items.length === 0 ? <div className="media-empty"><b>EMPTY</b><p>暂时没有公开的追番记录。</p></div>
                  : <>
                    <div className="media-grid">
                      {items.map((item) => <a className="media-card" key={item.bilibili_media_id} href={item.url} target="_blank" rel="noreferrer"><span className={cx("media-cover", item.cover_url && "has-image")}>{item.cover_url && <Image src={item.cover_url} fill sizes="(max-width: 720px) 112px, 150px" unoptimized alt="" />}<span>{item.season_type || "ANIME"}</span></span><span className="media-copy"><span className={cx("status", item.status === "finished" && "finished")}>{item.status === "watching" ? "● 在看" : "◆ 看完"} · {animeProgress(item)}</span><h2>{item.title}</h2><p>{item.summary || "数据来自 Bilibili"}</p>{item.score !== null && <span className="score">★ {item.score.toFixed(1)}</span>}<small>前往 Bilibili 官网 ↗</small></span></a>)}
                    </div>
                    <Pagination current={page} count={pageCount} total={total} unit="部" label="追番分页" onChange={changePage} />
                  </>}
    </>
  );
}
