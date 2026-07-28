import { cache } from "react";

import type { ArticleDetailPayload, Post } from "./BlogApp";

type ArticleListPayload = {
  page: number;
  per_page: number;
  total: number;
  items: Post[];
};

function internalApiOrigin(): string {
  const candidate = process.env.INTERNAL_API_ORIGIN || "http://backend:3000";
  const url = new URL(candidate);
  if (!/^https?:$/.test(url.protocol) || url.username || url.password || url.pathname !== "/") {
    throw new Error("INTERNAL_API_ORIGIN must be an HTTP(S) origin without credentials or a path");
  }
  return url.origin;
}

function publicOrigin(): string {
  const url = new URL(process.env.PUBLIC_ORIGIN || "http://localhost:3000");
  return url.origin;
}

async function apiFetch(path: string): Promise<Response> {
  return fetch(`${internalApiOrigin()}${path}`, {
    cache: "no-store",
    headers: { accept: "application/json" },
  });
}

export const getArticle = cache(async (slug: string): Promise<ArticleDetailPayload | null> => {
  const response = await apiFetch(`/api/v1/articles/${encodeURIComponent(slug)}`);
  if (response.status === 404) return null;
  if (!response.ok) throw new Error(`article API returned ${response.status}`);
  return response.json() as Promise<ArticleDetailPayload>;
});

export async function getAllArticles(): Promise<Post[]> {
  const items: Post[] = [];
  let page = 1;
  while (true) {
    const response = await apiFetch(`/api/v1/articles?page=${page}&per_page=50`);
    if (!response.ok) throw new Error(`article list API returned ${response.status}`);
    const payload = await response.json() as ArticleListPayload;
    items.push(...payload.items);
    if (items.length >= payload.total || payload.items.length === 0) return items;
    page += 1;
  }
}

export { publicOrigin };
