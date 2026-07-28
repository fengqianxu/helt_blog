import type { MetadataRoute } from "next";

import { getAllArticles, publicOrigin } from "./server-api";

export default async function sitemap(): Promise<MetadataRoute.Sitemap> {
  const origin = publicOrigin();
  const staticPaths = ["", "/archives", "/moments", "/anime", "/about", "/friends"];
  const articles = await getAllArticles();
  return [
    ...staticPaths.map((path) => ({
      url: `${origin}${path}`,
      changeFrequency: path === "" ? "daily" as const : "weekly" as const,
      priority: path === "" ? 1 : 0.7,
    })),
    ...articles.map((article) => ({
      url: `${origin}/posts/${encodeURIComponent(article.slug)}`,
      lastModified: article.updated_at,
      changeFrequency: "monthly" as const,
      priority: 0.8,
    })),
  ];
}
