import type { Metadata } from "next";
import { headers } from "next/headers";
import "artalk/Artalk.css";
import "./globals.css";

const themeInitScript = `
  try {
    const saved = localStorage.getItem("helt-theme");
    const theme = saved === "day" || saved === "night"
      ? saved
      : matchMedia("(prefers-color-scheme: dark)").matches ? "night" : "day";
    document.documentElement.dataset.theme = theme;
  } catch {}
`;

export async function generateMetadata(): Promise<Metadata> {
  const requestHeaders = await headers();
  const host = requestHeaders.get("x-forwarded-host") || requestHeaders.get("host") || "localhost:3000";
  const protocol = requestHeaders.get("x-forwarded-proto") || (host.startsWith("localhost") ? "http" : "https");
  const metadataBase = new URL(`${protocol}://${host}`);
  const title = "helt. | 写代码、追番、折腾博客";
  const description = "一个记录技术、生活、动画与游戏的个人博客。";

  return {
    metadataBase,
    title,
    description,
    icons: { icon: "/saber-day.png" },
    openGraph: { title, description, type: "website", images: [{ url: "/og.png", width: 1200, height: 630, alt: "helt. 日夜双主题博客" }] },
    twitter: { card: "summary_large_image", title, description, images: ["/og.png"] },
  };
}

export default function RootLayout({ children }: Readonly<{ children: React.ReactNode }>) {
  return (
    <html lang="zh-CN" suppressHydrationWarning>
      <head><script dangerouslySetInnerHTML={{ __html: themeInitScript }} /></head>
      <body>{children}</body>
    </html>
  );
}
