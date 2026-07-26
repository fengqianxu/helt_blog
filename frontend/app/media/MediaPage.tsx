"use client";

import dynamic from "next/dynamic";
import { KeyboardEvent, useState } from "react";

import { useBangumiRecords, useGameRecords } from "./hooks";

const AnimePage = dynamic(() => import("./AnimePage"));
const GamesPage = dynamic(() => import("./GamesPage"));

export default function MediaPage() {
  const [tab, setTab] = useState<"anime" | "games">("anime");
  const bangumi = useBangumiRecords();
  const games = useGameRecords();
  const subtitle = `在看 ${bangumi.meta.counts.watching} · 看完 ${bangumi.meta.counts.finished} · 最近在玩 ${games.meta.counts.recent}`;

  const navigateTabs = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    const tabs = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="tab"]'));
    const current = tabs.indexOf(event.target as HTMLButtonElement);
    if (current < 0) return;
    const next = event.key === "Home"
      ? 0
      : event.key === "End"
        ? tabs.length - 1
        : (current + (event.key === "ArrowRight" ? 1 : -1) + tabs.length) % tabs.length;
    event.preventDefault();
    setTab(next === 0 ? "anime" : "games");
    tabs[next]?.focus();
  };

  return (
    <main className="page-wrap page-enter">
      <div className="page-heading"><h1>追番 · 游戏</h1><span>{subtitle}</span></div>
      <div className="view-tabs" role="tablist" aria-label="记录类型" onKeyDown={navigateTabs}>
        <button id="anime-tab" role="tab" aria-controls="media-record-panel" aria-selected={tab === "anime"} tabIndex={tab === "anime" ? 0 : -1} className={tab === "anime" ? "active" : ""} onClick={() => setTab("anime")}>追番记录</button>
        <button id="games-tab" role="tab" aria-controls="media-record-panel" aria-selected={tab === "games"} tabIndex={tab === "games" ? 0 : -1} className={tab === "games" ? "active" : ""} onClick={() => setTab("games")}>游戏进程</button>
      </div>
      <div id="media-record-panel" role="tabpanel" aria-labelledby={`${tab}-tab`} tabIndex={0}>
        {tab === "anime" ? <AnimePage records={bangumi} /> : <GamesPage records={games} />}
      </div>
    </main>
  );
}
