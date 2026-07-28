import type { Metadata } from "next";
import { notFound } from "next/navigation";

import { BlogApp } from "../../BlogApp";
import { getArticle, publicOrigin } from "../../server-api";

type ArticlePageProps = { params: Promise<{ slug: string }> };

export async function generateMetadata({ params }: ArticlePageProps): Promise<Metadata> {
  const { slug } = await params;
  const payload = await getArticle(slug);
  if (!payload) return {};
  const article = payload.article;
  const canonical = `/posts/${article.slug}`;
  return {
    title: article.title,
    description: article.summary,
    alternates: { canonical },
    openGraph: {
      type: "article",
      url: new URL(canonical, publicOrigin()),
      title: article.title,
      description: article.summary,
      publishedTime: article.published_at || article.created_at,
      modifiedTime: article.updated_at,
      tags: article.tags.map((tag) => tag.name),
      images: article.cover_url ? [{ url: article.cover_url, alt: `${article.title} 封面` }] : undefined,
    },
    twitter: {
      card: article.cover_url ? "summary_large_image" : "summary",
      title: article.title,
      description: article.summary,
      images: article.cover_url ? [article.cover_url] : undefined,
    },
  };
}

export default async function ArticleRoute({ params }: ArticlePageProps) {
  const { slug } = await params;
  const payload = await getArticle(slug);
  if (!payload) notFound();
  return <BlogApp initialArticle={payload} />;
}
