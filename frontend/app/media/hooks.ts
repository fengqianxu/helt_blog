"use client";

import { useEffect, useState } from "react";

import {
  BANGUMI_PAGE_SIZE,
  BangumiItem,
  BangumiListPayload,
  fetchBangumiPage,
  fetchGamePage,
  GAME_PAGE_SIZE,
  GameListPayload,
  LoadState,
  SteamGameItem,
} from "./api";

const initialBangumiMeta: BangumiListPayload["meta"] = {
  counts: { watching: 0, finished: 0 },
  synced_at: null,
  configured: false,
  sync_status: "disabled",
};

const initialGameMeta: GameListPayload["meta"] = {
  counts: { total: 0, recent: 0 },
  synced_at: null,
  configured: false,
  sync_status: "disabled",
};

export function useBangumiRecords() {
  const [page, setPage] = useState(1);
  const [items, setItems] = useState<BangumiItem[]>([]);
  const [total, setTotal] = useState(0);
  const [meta, setMeta] = useState(initialBangumiMeta);
  const [state, setState] = useState<LoadState>("loading");
  const changePage = (nextPage: number) => {
    setState("loading");
    setPage(nextPage);
  };

  useEffect(() => {
    const controller = new AbortController();
    void fetchBangumiPage(page, controller.signal)
      .then((payload) => {
        if (controller.signal.aborted) return;
        const pageCount = Math.max(1, Math.ceil(payload.total / payload.per_page));
        setMeta(payload.meta);
        setTotal(payload.total);
        if (page > pageCount) {
          setPage(pageCount);
          return;
        }
        setItems(payload.items);
        setState("ready");
      })
      .catch((error: unknown) => {
        if (!controller.signal.aborted && error instanceof Error && error.name !== "AbortError") {
          setState("error");
        }
      });
    return () => controller.abort();
  }, [page]);

  return {
    page,
    setPage: changePage,
    items,
    total,
    meta,
    state,
    pageCount: Math.max(1, Math.ceil(total / BANGUMI_PAGE_SIZE)),
  };
}

export function useGameRecords() {
  const [page, setPage] = useState(1);
  const [sort, setSort] = useState<"recent" | "playtime">("recent");
  const [range, setRange] = useState<"all" | "recent">("all");
  const [items, setItems] = useState<SteamGameItem[]>([]);
  const [total, setTotal] = useState(0);
  const [meta, setMeta] = useState(initialGameMeta);
  const [state, setState] = useState<LoadState>("loading");
  const changePage = (nextPage: number) => {
    setState("loading");
    setPage(nextPage);
  };
  const changeSort = (nextSort: "recent" | "playtime") => {
    setState("loading");
    setSort(nextSort);
  };
  const changeRange = (nextRange: "all" | "recent") => {
    setState("loading");
    setRange(nextRange);
  };

  useEffect(() => {
    const controller = new AbortController();
    void fetchGamePage(page, sort, range, controller.signal)
      .then((payload) => {
        if (controller.signal.aborted) return;
        const pageCount = Math.max(1, Math.ceil(payload.total / payload.per_page));
        setMeta(payload.meta);
        setTotal(payload.total);
        if (page > pageCount) {
          setPage(pageCount);
          return;
        }
        setItems(payload.items);
        setState("ready");
      })
      .catch((error: unknown) => {
        if (!controller.signal.aborted && error instanceof Error && error.name !== "AbortError") {
          setState("error");
        }
      });
    return () => controller.abort();
  }, [page, range, sort]);

  return {
    page,
    setPage: changePage,
    sort,
    setSort: changeSort,
    range,
    setRange: changeRange,
    items,
    total,
    meta,
    state,
    pageCount: Math.max(1, Math.ceil(total / GAME_PAGE_SIZE)),
  };
}

export type BangumiRecords = ReturnType<typeof useBangumiRecords>;
export type GameRecords = ReturnType<typeof useGameRecords>;
