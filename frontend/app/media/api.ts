export type SyncStatus = "ok" | "queued" | "disabled" | "error";
export type LoadState = "loading" | "ready" | "error";
export type BangumiStatus = "watching" | "finished";

export type BangumiItem = {
  id: number;
  bilibili_media_id: number;
  season_id: number | null;
  title: string;
  cover_url: string | null;
  status: BangumiStatus;
  ep_current: number;
  ep_total: number;
  synced_at: string;
  season_type: string;
  summary: string;
  score: number | null;
  url: string;
  latest_episode: string;
};

export type BangumiListPayload = {
  page: number;
  per_page: number;
  total: number;
  items: BangumiItem[];
  meta: {
    counts: Record<BangumiStatus, number>;
    synced_at: string | null;
    configured: boolean;
    sync_status: SyncStatus;
  };
};

export type SteamGameItem = {
  id: number;
  steam_app_id: number;
  title: string;
  status: "playing" | "finished" | "shelved";
  cover_url: string;
  icon_url: string | null;
  playtime_2weeks_minutes: number;
  playtime_forever_minutes: number;
  playtime_windows_minutes: number;
  playtime_mac_minutes: number;
  playtime_linux_minutes: number;
  last_played_at: string | null;
  synced_at: string;
  steam_url: string;
};

export type GameListPayload = {
  page: number;
  per_page: number;
  total: number;
  items: SteamGameItem[];
  meta: {
    counts: { total: number; recent: number };
    synced_at: string | null;
    configured: boolean;
    sync_status: SyncStatus;
  };
};

export const BANGUMI_PAGE_SIZE = 8;
export const GAME_PAGE_SIZE = 8;

export async function fetchBangumiPage(page: number, signal: AbortSignal) {
  const query = new URLSearchParams({
    page: String(page),
    per_page: String(BANGUMI_PAGE_SIZE),
  });
  const response = await fetch(`/api/v1/bangumi?${query}`, { signal });
  if (!response.ok) throw new Error("追番数据加载失败");
  return response.json() as Promise<BangumiListPayload>;
}

export async function fetchGamePage(
  page: number,
  sort: "recent" | "playtime",
  range: "all" | "recent",
  signal: AbortSignal,
) {
  const query = new URLSearchParams({
    page: String(page),
    per_page: String(GAME_PAGE_SIZE),
    sort,
  });
  if (range === "recent") query.set("recent", "true");
  const response = await fetch(`/api/v1/games?${query}`, { signal });
  if (!response.ok) throw new Error("Steam 游戏数据加载失败");
  return response.json() as Promise<GameListPayload>;
}
