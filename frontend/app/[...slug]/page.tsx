import type { Metadata } from "next";
import { notFound } from "next/navigation";

import { BlogApp } from "../BlogApp";

type CatchAllProps = { params: Promise<{ slug: string[] }> };

const pageTitles = new Map([
  ["archives", "文章归档"],
  ["moments", "时间轴"],
  ["anime", "追番与游戏"],
  ["about", "关于"],
  ["friends", "友链"],
]);

function isKnownPath(parts: string[]): boolean {
  const path = `/${parts.join("/")}`;
  if (parts.length === 1 && pageTitles.has(parts[0])) return true;
  return /^\/admin(?:$|\/(?:login|articles|comments|assets|raiments|appearance|llm|kanban|playlists|media|settings))/.test(path)
    && !path.includes("..")
    && (path === "/admin/articles/new" || /^\/admin\/articles\/\d+\/edit$/.test(path) || parts.length <= 2);
}

export async function generateMetadata({ params }: CatchAllProps): Promise<Metadata> {
  const { slug } = await params;
  if (slug.length === 1 && pageTitles.has(slug[0])) {
    const title = pageTitles.get(slug[0]);
    return { title, alternates: { canonical: `/${slug[0]}` } };
  }
  if (slug[0] === "admin") return { title: "管理后台", robots: { index: false, follow: false } };
  return {};
}

export default async function CatchAllPage({ params }: CatchAllProps) {
  const { slug } = await params;
  if (!isKnownPath(slug)) notFound();
  return <BlogApp />;
}
