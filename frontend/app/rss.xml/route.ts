import { getAllArticles, publicOrigin } from "../server-api";

function xml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
}

export async function GET() {
  const origin = publicOrigin();
  const articles = await getAllArticles();
  const items = articles.map((article) => {
    const link = `${origin}/posts/${encodeURIComponent(article.slug)}`;
    const published = new Date(article.published_at || article.created_at).toUTCString();
    return `<item><title>${xml(article.title)}</title><link>${xml(link)}</link><guid isPermaLink="true">${xml(link)}</guid><description>${xml(article.summary)}</description><pubDate>${published}</pubDate></item>`;
  }).join("");
  const body = `<?xml version="1.0" encoding="UTF-8"?><rss version="2.0"><channel><title>helt.</title><link>${xml(origin)}</link><description>记录技术、生活、动画与游戏</description><language>zh-CN</language>${items}</channel></rss>`;
  return new Response(body, {
    headers: {
      "content-type": "application/rss+xml; charset=utf-8",
      "cache-control": "public, max-age=300, stale-while-revalidate=3600",
    },
  });
}
