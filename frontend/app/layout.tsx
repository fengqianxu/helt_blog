import type { Metadata } from "next";
import "artalk/Artalk.css";
import "./globals.css";

const themeInitScript = `
  try {
    const saved = localStorage.getItem("helt-color-scheme") || localStorage.getItem("helt-theme");
    const theme = saved === "day" || saved === "night"
      ? saved
      : matchMedia("(prefers-color-scheme: dark)").matches ? "night" : "day";
    document.documentElement.dataset.theme = theme;
  } catch {}
`;

function configuredMetadataBase(): URL {
  try {
    const url = new URL(process.env.PUBLIC_ORIGIN || "http://localhost:3000");
    if (
      (url.protocol === "http:" || url.protocol === "https:")
      && !url.username
      && !url.password
    ) return new URL(url.origin);
  } catch {
    // The backend performs strict startup validation; this keeps isolated
    // frontend development usable even when the environment is malformed.
  }
  return new URL("http://localhost:3000");
}

export function generateMetadata(): Metadata {
  const metadataBase = configuredMetadataBase();
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
