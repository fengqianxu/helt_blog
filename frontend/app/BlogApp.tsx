"use client";

import Link from "next/link";
import Image from "next/image";
import dynamic from "next/dynamic";
import { createContext, type CSSProperties, FormEvent, type PointerEvent as ReactPointerEvent, type RefObject, useCallback, useContext, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { usePathname } from "next/navigation";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import "@uiw/react-md-editor/markdown-editor.css";
import "@uiw/react-markdown-preview/markdown.css";

import { buildArticleToc, getActiveTocId } from "./article-toc.mjs";
import {
  AdminIdentity,
  AdminAsset,
  AdminPlaylist,
  assetLabels,
  cx,
  DEFAULT_PROFILE_AVATAR_URL,
  isJsonResponse,
  Notify,
  PlaylistPayload,
  PublicRaimentPayload,
  PublicProfile,
  RaimentSchedule,
  responseMessage,
  scheduledPeriod,
  SitePayload,
  Theme,
  ThemeTokens,
} from "./admin/shared";

type Raiment = {
  id: string;
  mode: Theme;
  name: string;
  shortName: string;
  cover: string;
  colors: ThemeTokens;
  coverTitle: string;
  coverSubtitle: string;
  coverCharacterName: string;
  coverDialogue: string;
  coverVoiceLabel: string;
  coverVoiceUrl: string | null;
  kanban: {
    displayName: string;
    greeting: string;
  };
};

type RaimentCatalog = {
  items: Record<string, Raiment>;
  order: string[];
  defaultId: string;
  activeId: string;
  schedule: RaimentSchedule;
};

// 看板娘展示文案仍是静态回退；目录已经为每套灵衣保留 kanban_asset_id 接口。
const DEFAULT_RAIMENTS: Record<string, Raiment> = {
  saber: {
    id: "saber",
    mode: "day",
    name: "日间模式",
    shortName: "SABER",
    cover: "/saber-day.png",
    colors: {
      primary: "#2B5FB8", secondary: "#B99A3E", background: "#F5F7FB",
      surface: "#FFFFFF", surface_alt: "#F0EFE9", text: "#1F2534",
      text_secondary: "#3A4155", muted: "#6B7284", faint: "#9AA1B3",
      border: "#D9DCE3", danger: "#D84358", success: "#3D8455",
    },
    coverTitle: "「問おう。\n貴方が私のマスターか？」",
    coverSubtitle: "—— 我问你，你就是我的 Master 吗？",
    coverCharacterName: "Saber",
    coverDialogue: "今日もいい天気ですね。",
    coverVoiceLabel: "音声を再生 · 川澄綾子",
    coverVoiceUrl: null,
    kanban: {
      displayName: "Saber",
      greeting: "Master，今日也请从容阅读。",
    },
  },
  "alter-saber": {
    id: "alter-saber",
    mode: "night",
    name: "夜间模式",
    shortName: "ALTER",
    cover: "/saber-night.png",
    colors: {
      primary: "#D84358", secondary: "#7B4B8E", background: "#0E0B16",
      surface: "#171320", surface_alt: "#211B2B", text: "#EAE7F2",
      text_secondary: "#C5BFD1", muted: "#9A94AD", faint: "#6F6A80",
      border: "#3A3447", danger: "#F0718A", success: "#77B989",
    },
    coverTitle: "「問おう。\n貴方が私のマスターか？」",
    coverSubtitle: "—— 我问你，你就是我的 Master 吗？",
    coverCharacterName: "Alter",
    coverDialogue: "夜已深，Master。仍要继续前行吗？",
    coverVoiceLabel: "音声を再生 · Alter",
    coverVoiceUrl: null,
    kanban: {
      displayName: "Alter",
      greeting: "夜深了，Master。继续前进吧。",
    },
  },
};

const DEFAULT_RAIMENT_CATALOG: RaimentCatalog = {
  items: DEFAULT_RAIMENTS,
  order: ["saber", "alter-saber"],
  defaultId: "saber",
  activeId: "saber",
  schedule: {
    revision: 1,
    periods: [
      { id: "period-saber", start_at: "07:00", end_at: "19:00", raiment_id: "saber", playlist_id: null },
      { id: "period-alter-saber", start_at: "19:00", end_at: "07:00", raiment_id: "alter-saber", playlist_id: null },
    ],
  },
};

const RaimentContext = createContext<RaimentCatalog>(DEFAULT_RAIMENT_CATALOG);
const DEFAULT_SITE: SitePayload = {
  basic: {
    name: "helt.",
    tagline: "记录技术、生活与热爱",
    footer_text: "记录技术、生活与热爱",
    footer_copyright: "© 2020—{year} {site_name} · POWERED BY REACT",
    hero_eyebrow: "SINCE 2020 · HELT'S BLOG",
    domain: "",
    icp: "",
    founded_at: "2026-07-23",
    logo_asset_id: null,
    logo_url: null,
    favicon_asset_id: null,
    favicon_url: null,
  },
  features: {
    splash: true,
    comments: true,
    kanban: true,
    music: true,
    stats: true,
    easter_egg: true,
  },
  stats: { article_count: 0, total_words: 0, total_visits: 0, uptime_days: 1 },
  updated_at: "",
};
const SiteContext = createContext<SitePayload>(DEFAULT_SITE);
const RAIMENT_STORAGE_KEY = "helt-raiment";
const COLOR_SCHEME_STORAGE_KEY = "helt-color-scheme";
const LEGACY_THEME_STORAGE_KEY = "helt-theme";
const VISITOR_STORAGE_KEY = "helt-visitor-id";

function readStoredRaiment() {
  try {
    const raimentId = localStorage.getItem(RAIMENT_STORAGE_KEY);
    const legacyTheme = localStorage.getItem(LEGACY_THEME_STORAGE_KEY) as Theme | null;
    const savedColorScheme = localStorage.getItem(COLOR_SCHEME_STORAGE_KEY) as Theme | null;
    return {
      raimentId,
      legacyTheme: legacyTheme === "day" || legacyTheme === "night" ? legacyTheme : null,
      colorScheme: savedColorScheme === "day" || savedColorScheme === "night" ? savedColorScheme : null,
    };
  } catch {
    return { raimentId: null, legacyTheme: null, colorScheme: null };
  }
}

function persistColorScheme(theme: Theme) {
  try {
    localStorage.setItem(COLOR_SCHEME_STORAGE_KEY, theme);
  } catch {
    // Storage can be unavailable in hardened/private browsing contexts.
  }
}

function persistRaimentPreference(id: string, theme: Theme) {
  try {
    localStorage.setItem(RAIMENT_STORAGE_KEY, id);
    localStorage.setItem(COLOR_SCHEME_STORAGE_KEY, theme);
    localStorage.removeItem(LEGACY_THEME_STORAGE_KEY);
  } catch {
    // Theme switching must still work when persistence is unavailable.
  }
}

function clearStoredRaimentPreference() {
  try {
    localStorage.removeItem(RAIMENT_STORAGE_KEY);
    localStorage.removeItem(LEGACY_THEME_STORAGE_KEY);
  } catch {
    // The in-memory schedule remains authoritative.
  }
}

function visitorId() {
  try {
    const saved = localStorage.getItem(VISITOR_STORAGE_KEY);
    if (saved) return saved;
    const created = typeof crypto !== "undefined" && "randomUUID" in crypto
      ? crypto.randomUUID()
      : `visitor-${Date.now()}-${Math.random().toString(36).slice(2)}`;
    localStorage.setItem(VISITOR_STORAGE_KEY, created);
    return created;
  } catch {
    return null;
  }
}

const resolveRaiment = (catalog: RaimentCatalog): Raiment =>
  catalog.items[catalog.activeId]
  || catalog.items[catalog.defaultId]
  || catalog.items[catalog.order[0]]
  || DEFAULT_RAIMENTS.saber;

const useRaiment = () => resolveRaiment(useContext(RaimentContext));
const useSite = () => useContext(SiteContext);

const catalogFromPayload = (payload: PublicRaimentPayload): RaimentCatalog => ({
  items: Object.fromEntries(payload.items.map((item) => {
    const fallback = DEFAULT_RAIMENTS[item.id];
    return [item.id, {
      id: item.id,
      mode: item.color_scheme,
      name: item.name,
      shortName: item.name.toUpperCase(),
      cover: item.cover_url,
      colors: item.theme,
      coverTitle: item.cover_title,
      coverSubtitle: item.cover_subtitle,
      coverCharacterName: item.cover_character_name,
      coverDialogue: item.cover_dialogue,
      coverVoiceLabel: item.cover_voice_label,
      coverVoiceUrl: item.cover_voice_url,
      kanban: fallback?.kanban || {
        displayName: item.name,
        greeting: "欢迎来到 helt.",
      },
    } satisfies Raiment];
  })),
  order: payload.items.map((item) => item.id),
  defaultId: payload.default_raiment_id,
  activeId: payload.default_raiment_id,
  schedule: payload.schedule,
});

const raimentFromSchedule = (catalog: RaimentCatalog): string => {
  if (!catalog.order.length) return "saber";
  const active = scheduledPeriod(catalog.schedule);
  return active && catalog.items[active.raiment_id]
    ? active.raiment_id
    : catalog.items[catalog.defaultId]
      ? catalog.defaultId
      : catalog.order[0];
};

type ArticleCategory = { id: number; name: string; slug: string; color: string; article_count?: number | null };
type ArticleTag = { id: number; name: string; article_count?: number | null };
export type Post = {
  id: number;
  slug: string;
  title: string;
  summary: string;
  content_md?: string | null;
  status: "draft" | "published" | "hidden";
  is_pinned: boolean;
  allow_comment: boolean;
  kanban_ref: boolean;
  word_count: number;
  read_minutes: number;
  view_count: number;
  published_at: string | null;
  created_at: string;
  updated_at: string;
  category: ArticleCategory | null;
  cover_url: string | null;
  tags: ArticleTag[];
};

type ArticleListPayload = { page: number; per_page: number; total: number; items: Post[] };
type RelatedPost = Pick<Post, "id" | "slug" | "title">;
type TocItem = { id: string; text: string; level: number };
type MomentImage = {
  asset_id: number;
  url: string;
  alt_text: string;
};
type Moment = {
  id: number;
  content: string;
  images: MomentImage[];
  tags: ArticleTag[];
  like_count: number;
  created_at: string;
  updated_at?: string;
  liked_by_me: boolean;
};
type MomentListPayload = { page: number; per_page: number; total: number; items: Moment[] };
export type ArticleDetailPayload = {
  article: Post;
  previous: Post | null;
  next: Post | null;
  related: RelatedPost[];
  allow_comment: boolean;
};

const ARTALK_SERVER = "/artalk";
const ARTALK_SITE = "helt.";
const articleCommentKey = (slug: string) => `/posts/${slug}`;
type ArtalkInstance = { destroy: () => void; setDarkMode: (darkMode: boolean) => void };

function useArtalkCommentCounts(pageKeys: string) {
  useEffect(() => {
    if (!pageKeys) return;
    let cancelled = false;
    void import("artalk").then(({ default: Artalk }) => {
      if (cancelled) return;
      Artalk.loadCountWidget({
        server: ARTALK_SERVER,
        site: ARTALK_SITE,
        pvEl: ".artalk-pv-count-disabled",
        countEl: ".artalk-comment-count",
      });
    }).catch(() => undefined);
    return () => { cancelled = true; };
  }, [pageKeys]);
}

const categoryName = (post: Post) => post.category?.name || "未分类";
const articleDate = (post: Post) => (post.published_at || post.updated_at || post.created_at).slice(0, 10);
const articleWords = (post: Post) => `${post.word_count.toLocaleString()} 字`;
const articleTime = (post: Post) => `${Math.max(1, post.read_minutes)} min`;

const navItems = [
  ["/", "首页"], ["/archives", "归档"], ["/moments", "时间轴"], ["/anime", "追番"], ["/about", "关于"], ["/friends", "友链"],
];
const MediaPage = dynamic(() => import("./media/MediaPage"), {
  loading: () => <main className="page-wrap page-enter"><div className="media-empty"><b>SYNC</b><p>正在加载追番与游戏页面…</p></div></main>,
});
const AdminAccountCenter = dynamic(() => import("./admin/AdminAccountCenter").then((module) => module.AdminAccountCenter));
const AdminProfileAvatar = dynamic(() => import("./admin/AdminAccountCenter").then((module) => module.AdminProfileAvatar));
const AdminLogin = dynamic(() => import("./admin/AdminLogin").then((module) => module.AdminLogin));
const AssetManager = dynamic(() => import("./admin/AssetManager").then((module) => module.AssetManager));
const LlmSettings = dynamic(() => import("./admin/LlmSettings").then((module) => module.LlmSettings));
const RaimentSettings = dynamic(() => import("./admin/RaimentSettings").then((module) => module.RaimentSettings));
const SiteSettings = dynamic(() => import("./admin/SiteSettings").then((module) => module.SiteSettings));
const PlaylistSettings = dynamic(() => import("./admin/PlaylistSettings").then((module) => module.PlaylistSettings));
const ReviewManager = dynamic(() => import("./admin/ReviewManager").then((module) => module.ReviewManager));
const MDEditor = dynamic(() => import("@uiw/react-md-editor/nohighlight"), { ssr: false });

export function BlogApp({ initialArticle = null }: { initialArticle?: ArticleDetailPayload | null }) {
  const pathname = usePathname() || "/";
  const [site, setSite] = useState<SitePayload>(DEFAULT_SITE);
  const [siteFeaturesReady, setSiteFeaturesReady] = useState(false);
  const [raimentCatalog, setRaimentCatalog] = useState<RaimentCatalog>(DEFAULT_RAIMENT_CATALOG);
  const [activeRaimentId, setActiveRaimentId] = useState("saber");
  const theme = raimentCatalog.items[activeRaimentId]?.mode || "day";
  const [menuOpen, setMenuOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [themeTransition, setThemeTransition] = useState(false);
  const [toast, setToast] = useState<{ message: string; tone: "normal" | "success" | "danger" } | null>(null);
  const [easterEggPath, setEasterEggPath] = useState<string | null>(null);
  const toastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const storedRaimentPreference = useRef(false);
  const storedRaimentId = useRef<string | null>(null);

  useEffect(() => {
    let controller: AbortController | null = null;
    const loadSite = () => {
      controller?.abort();
      controller = new AbortController();
      void fetch("/api/v1/site", { signal: controller.signal, cache: "no-store" })
        .then(async (response) => {
          if (!response.ok) throw new Error("站点资料加载失败");
          return response.json() as Promise<SitePayload>;
        })
        .then((payload) => {
          setSite(payload);
          if (!payload.features.easter_egg) setEasterEggPath(null);
          setSiteFeaturesReady(true);
        })
        .catch((error) => {
          if (!(error instanceof DOMException && error.name === "AbortError")) {
            console.warn("Falling back to bundled site configuration", error);
            setSiteFeaturesReady(true);
          }
        });
    };
    loadSite();
    window.addEventListener("helt:site-settings-updated", loadSite);
    return () => {
      controller?.abort();
      window.removeEventListener("helt:site-settings-updated", loadSite);
    };
  }, []);

  useEffect(() => {
    document.title = site.basic.tagline
      ? `${site.basic.name} | ${site.basic.tagline}`
      : site.basic.name;
    const faviconUrl = site.basic.favicon_url || "/saber-day.png";
    const icons = Array.from(document.querySelectorAll<HTMLLinkElement>('link[rel~="icon"]'));
    const targets = icons.length ? icons : [document.head.appendChild(document.createElement("link"))];
    targets.forEach((link) => {
      link.rel = "icon";
      link.href = faviconUrl;
    });
  }, [site.basic.favicon_url, site.basic.name, site.basic.tagline]);

  useEffect(() => {
    if (!siteFeaturesReady || pathname.startsWith("/admin") || !site.features.stats) return;
    const id = visitorId();
    if (!id) return;
    const visitKey = `helt-visited:${pathname}`;
    try {
      if (sessionStorage.getItem(visitKey)) return;
      sessionStorage.setItem(visitKey, "1");
    } catch {
      // A visit can still be recorded if session storage is unavailable.
    }
    void fetch("/api/v1/stats/visit", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ visitor_id: id, path: pathname }),
      keepalive: true,
    }).catch(() => undefined);
  }, [pathname, site.features.stats, siteFeaturesReady]);

  useEffect(() => {
    const { raimentId: saved, legacyTheme, colorScheme } = readStoredRaiment();
    const fallbackId = legacyTheme === "night" ? "alter-saber" : "saber";
    const nextId = saved || fallbackId;
    storedRaimentPreference.current = Boolean(saved || legacyTheme);
    storedRaimentId.current = nextId;
    const nextTheme = colorScheme || DEFAULT_RAIMENTS[nextId]?.mode || "day";
    document.documentElement.dataset.theme = nextTheme;
    window.requestAnimationFrame(() => {
      setActiveRaimentId(nextId);
    });
  }, []);

  useEffect(() => {
    let controller: AbortController | null = null;
    const loadRaiments = () => {
      controller?.abort();
      controller = new AbortController();
      void fetch("/api/v1/raiments", { signal: controller.signal, cache: "no-store" })
        .then(async (response) => {
          if (!response.ok) throw new Error("灵衣目录加载失败");
          return response.json() as Promise<PublicRaimentPayload>;
        })
        .then((payload) => {
          const catalog = catalogFromPayload(payload);
          setRaimentCatalog(catalog);
          const saved = storedRaimentId.current;
          const savedIsAvailable = Boolean(saved && catalog.items[saved]);
          if (saved && !savedIsAvailable) {
            storedRaimentPreference.current = false;
            storedRaimentId.current = null;
            clearStoredRaimentPreference();
          }
          const nextId = saved && savedIsAvailable ? saved : raimentFromSchedule(catalog);
          setActiveRaimentId(nextId);
        })
        .catch((error) => {
          if (!(error instanceof DOMException && error.name === "AbortError")) {
            console.warn("Falling back to bundled raiment configuration", error);
          }
        });
    };
    loadRaiments();
    window.addEventListener("helt:raiments-updated", loadRaiments);
    return () => {
      controller?.abort();
      window.removeEventListener("helt:raiments-updated", loadRaiments);
    };
  }, []);

  useEffect(() => {
    if (storedRaimentPreference.current) return;
    const syncSchedule = () => setActiveRaimentId(raimentFromSchedule(raimentCatalog));
    syncSchedule();
    const timer = window.setInterval(syncSchedule, 60_000);
    return () => window.clearInterval(timer);
  }, [raimentCatalog]);

  const activeCatalog = { ...raimentCatalog, activeId: activeRaimentId };

  useEffect(() => {
    const activeRaiment = resolveRaiment({ ...raimentCatalog, activeId: activeRaimentId });
    document.documentElement.dataset.theme = activeRaiment.mode;
    document.documentElement.style.setProperty("--accent", activeRaiment.colors.primary);
    document.documentElement.style.setProperty("--gold", activeRaiment.colors.secondary);
    document.documentElement.style.setProperty("--bg", activeRaiment.colors.background);
    document.documentElement.style.setProperty("--surface", activeRaiment.colors.surface);
    document.documentElement.style.setProperty("--surface-2", activeRaiment.colors.surface_alt);
    document.documentElement.style.setProperty("--text", activeRaiment.colors.text);
    document.documentElement.style.setProperty("--text-2", activeRaiment.colors.text_secondary);
    document.documentElement.style.setProperty("--muted", activeRaiment.colors.muted);
    document.documentElement.style.setProperty("--faint", activeRaiment.colors.faint);
    document.documentElement.style.setProperty("--line", activeRaiment.colors.border);
    document.documentElement.style.setProperty("--accent-soft", `color-mix(in srgb, ${activeRaiment.colors.primary} 15%, ${activeRaiment.colors.background})`);
    document.documentElement.style.setProperty("--danger", activeRaiment.colors.danger);
    document.documentElement.style.setProperty("--green", activeRaiment.colors.success);
    document.documentElement.style.setProperty("--shadow", `0 14px 38px color-mix(in srgb, ${activeRaiment.colors.text} 14%, transparent)`);
    persistColorScheme(activeRaiment.mode);
  }, [raimentCatalog, activeRaimentId]);

  useEffect(() => {
    const reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduceMotion) return;
    const observer = new IntersectionObserver((entries) => entries.forEach((entry) => {
      if (entry.isIntersecting) entry.target.classList.add("is-visible");
    }), { threshold: .12, rootMargin: "0px 0px -40px" });
    document.querySelectorAll(".reveal, .post-card, .media-grid article, .friend-card, .moment-card, .admin-panel").forEach((node) => observer.observe(node));
    return () => observer.disconnect();
  }, [pathname]);

  useEffect(() => {
    if (!siteFeaturesReady || pathname.startsWith("/admin") || !site.features.easter_egg) return;
    const sequence = ["ArrowUp", "ArrowUp", "ArrowDown", "ArrowDown", "ArrowLeft", "ArrowRight", "ArrowLeft", "ArrowRight", "b", "a"];
    let cursor = 0;
    const onKey = (event: KeyboardEvent) => {
      if ((event.target as HTMLElement | null)?.closest("input, textarea, select, [contenteditable='true']")) return;
      cursor = event.key.toLowerCase() === sequence[cursor].toLowerCase() ? cursor + 1 : 0;
      if (cursor === sequence.length) { setEasterEggPath(pathname); cursor = 0; }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pathname, site.features.easter_egg, siteFeaturesReady]);

  const notify = useCallback<Notify>((message, tone = "normal") => {
    setToast({ message, tone });
    if (toastTimer.current) clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(null), 2600);
  }, []);

  useEffect(() => () => {
    if (toastTimer.current) clearTimeout(toastTimer.current);
  }, []);

  const toggleTheme = () => {
    if (themeTransition) return;
    if (raimentCatalog.order.length < 2) {
      notify("当前只有一套可用灵衣");
      return;
    }
    setThemeTransition(true);
    window.setTimeout(() => {
      const currentIndex = raimentCatalog.order.indexOf(activeRaimentId);
      const nextId = raimentCatalog.order[(currentIndex + 1) % raimentCatalog.order.length]
        || raimentCatalog.order[0]
        || "saber";
      storedRaimentPreference.current = true;
      storedRaimentId.current = nextId;
      persistRaimentPreference(nextId, raimentCatalog.items[nextId]?.mode || "day");
      setActiveRaimentId(nextId);
    }, 310);
    window.setTimeout(() => setThemeTransition(false), 760);
  };
  const common = { toggleTheme, pathname, menuOpen, setMenuOpen, searchOpen, setSearchOpen };

  if (pathname.startsWith("/admin")) return <SiteContext.Provider value={site}><RaimentContext.Provider value={activeCatalog}><AdminRouter pathname={pathname} theme={theme} toggleTheme={toggleTheme} notify={notify} />{themeTransition && <ThemeBlade />}{toast && <Toast {...toast} />}</RaimentContext.Provider></SiteContext.Provider>;

  let page: React.ReactNode;
  if (pathname === "/") page = <HomePage toggleTheme={toggleTheme} notify={notify} onSearch={() => setSearchOpen(true)} siteFeaturesReady={siteFeaturesReady} />;
  else if (pathname.startsWith("/posts/")) {
    const slug = pathname.split("/").filter(Boolean)[1];
    page = <ArticlePage slug={slug} theme={theme} notify={notify} siteFeaturesReady={siteFeaturesReady} initialPayload={initialArticle} />;
  }
  else if (pathname === "/archives") page = <ArchivesPage />;
  else if (pathname === "/moments") page = <MomentsPage notify={notify} />;
  else if (pathname === "/anime") page = <MediaPage />;
  else if (pathname === "/about") page = <AboutPage notify={notify} />;
  else if (pathname === "/friends") page = <FriendsPage notify={notify} />;
  else page = <NotFound />;

  return (
    <SiteContext.Provider value={site}><RaimentContext.Provider value={activeCatalog}><div className="site-shell">
      {pathname !== "/" && <TopNav {...common} />}
      {page}
      <Footer />
      {siteFeaturesReady && site.features.music && <BackgroundMusic schedule={raimentCatalog.schedule} />}
      {searchOpen && <SearchOverlay onClose={() => setSearchOpen(false)} />}
      {themeTransition && <ThemeBlade />}
      {toast && <Toast {...toast} />}
      {site.features.easter_egg && easterEggPath === pathname && <EasterEgg onClose={() => setEasterEggPath(null)} />}
    </div></RaimentContext.Provider></SiteContext.Provider>
  );
}

function ThemeBlade() { return <div className="theme-blade" aria-hidden="true"><i /><i /><i /></div>; }

function Toast({ message, tone }: { message: string; tone: "normal" | "success" | "danger" }) {
  return <div className={cx("toast", `toast-${tone}`)} role="status"><span>{tone === "success" ? "✓" : tone === "danger" ? "!" : "◆"}</span>{message}</div>;
}

function formatAudioTime(seconds: number) {
  const safeSeconds = Number.isFinite(seconds) && seconds > 0 ? Math.floor(seconds) : 0;
  return `${Math.floor(safeSeconds / 60)}:${String(safeSeconds % 60).padStart(2, "0")}`;
}

type MusicPlayerPosition = { x: number; y: number };
type MusicPlayerDock = "left" | "right";

const MUSIC_PLAYER_LAYOUT_KEY = "helt-bgm-player-layout";

function fitMusicPlayerPosition(position: MusicPlayerPosition, width: number, height: number, edge = 8) {
  const viewportWidth = document.documentElement.clientWidth;
  const viewportHeight = window.innerHeight;
  return {
    x: Math.min(Math.max(edge, position.x), Math.max(edge, viewportWidth - width - edge)),
    y: Math.min(Math.max(edge, position.y), Math.max(edge, viewportHeight - height - edge)),
  };
}

function BackgroundMusic({ schedule }: { schedule: RaimentSchedule }) {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const playerRef = useRef<HTMLElement | null>(null);
  const wantsPlayback = useRef(true);
  const lastAudibleVolume = useRef(.45);
  const dragState = useRef<{ pointerId: number; offsetX: number; offsetY: number } | null>(null);
  const dragCleanup = useRef<(() => void) | null>(null);
  const positionRef = useRef<MusicPlayerPosition | null>(null);
  const expandedPosition = useRef<MusicPlayerPosition | null>(null);
  const dockRef = useRef<MusicPlayerDock>("left");
  const [playlistId, setPlaylistId] = useState<number | null>(null);
  const [playlist, setPlaylist] = useState<AdminPlaylist | null>(null);
  const [trackIndex, setTrackIndex] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(.45);
  const [playerPosition, setPlayerPosition] = useState<MusicPlayerPosition | null>(null);
  const [minimized, setMinimized] = useState(true);
  const [dockedSide, setDockedSide] = useState<MusicPlayerDock>("left");
  const [dragging, setDragging] = useState(false);

  useEffect(() => {
    const sync = () => setPlaylistId(scheduledPeriod(schedule)?.playlist_id ?? null);
    sync();
    const timer = window.setInterval(sync, 30_000);
    return () => window.clearInterval(timer);
  }, [schedule]);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      try {
        const raw = localStorage.getItem("helt-bgm-volume");
        const stored = raw === null ? Number.NaN : Number(raw);
        if (Number.isFinite(stored) && stored >= 0 && stored <= 1) {
          setVolume(stored);
          if (stored > 0) lastAudibleVolume.current = stored;
        }
      } catch {
        // Background music remains usable when storage is unavailable.
      }
    }, 0);
    return () => window.clearTimeout(timer);
  }, []);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      try {
        const raw = localStorage.getItem(MUSIC_PLAYER_LAYOUT_KEY);
        if (!raw) return;
        const stored = JSON.parse(raw) as {
          position?: MusicPlayerPosition;
          expandedPosition?: MusicPlayerPosition;
          minimized?: boolean;
          dockedSide?: MusicPlayerDock;
        };
        const validPosition = (value: MusicPlayerPosition | undefined) => value
          && Number.isFinite(value.x)
          && Number.isFinite(value.y);
        if (validPosition(stored.position)) {
          positionRef.current = stored.position!;
          setPlayerPosition(stored.position!);
        }
        if (validPosition(stored.expandedPosition)) expandedPosition.current = stored.expandedPosition!;
        if (stored.dockedSide === "left" || stored.dockedSide === "right") {
          dockRef.current = stored.dockedSide;
          setDockedSide(stored.dockedSide);
        }
        setMinimized(true);
      } catch {
        // Ignore stale or malformed layout preferences.
      }
    }, 0);
    return () => window.clearTimeout(timer);
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    if (playlistId === null) return () => controller.abort();
    void fetch("/api/v1/playlists", { signal: controller.signal, cache: "no-store" })
      .then(async (response) => {
        if (!response.ok) throw new Error("歌单加载失败");
        return response.json() as Promise<PlaylistPayload>;
      })
      .then((payload) => {
        setTrackIndex(0);
        setPlaying(false);
        setCurrentTime(0);
        setDuration(0);
        setPlaylist(payload.items.find((item) => item.id === playlistId) || null);
      })
      .catch((error) => {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          console.warn("Scheduled background playlist is unavailable", error);
        }
      });
    return () => controller.abort();
  }, [playlistId]);

  const activePlaylist = playlist?.id === playlistId ? playlist : null;
  const tracks = activePlaylist?.tracks || [];
  const track = tracks[trackIndex] || null;
  const hasPlayerPosition = playerPosition !== null;

  useEffect(() => {
    const keepPlayerInView = () => {
      const player = playerRef.current;
      if (!player || (!positionRef.current && !minimized)) return;
      const rect = player.getBoundingClientRect();
      const current = positionRef.current || { x: rect.left, y: rect.top };
      const next = minimized
        ? {
            x: dockedSide === "left" ? 0 : Math.max(0, document.documentElement.clientWidth - rect.width),
            y: Math.min(Math.max(8, current.y), Math.max(8, window.innerHeight - rect.height - 8)),
          }
        : fitMusicPlayerPosition(current, rect.width, rect.height);
      positionRef.current = next;
      setPlayerPosition((previous) => previous && previous.x === next.x && previous.y === next.y ? previous : next);
    };
    keepPlayerInView();
    window.addEventListener("resize", keepPlayerInView);
    return () => window.removeEventListener("resize", keepPlayerInView);
  }, [dockedSide, hasPlayerPosition, minimized, track]);

  useEffect(() => {
    const audio = audioRef.current;
    if (!audio || !track) return;
    audio.load();
    setCurrentTime(0);
    setDuration(track.duration_s);
    if (wantsPlayback.current) {
      void audio.play().then(() => setPlaying(true)).catch(() => setPlaying(false));
    }
  }, [track]);

  useEffect(() => {
    if (audioRef.current) audioRef.current.volume = volume;
  }, [volume, track]);

  useEffect(() => () => {
    audioRef.current?.pause();
    dragCleanup.current?.();
  }, []);

  const advance = (delta: number) => {
    if (!tracks.length) return;
    const next = (trackIndex + delta + tracks.length) % tracks.length;
    if (next === trackIndex) {
      const audio = audioRef.current;
      if (!audio) return;
      audio.currentTime = 0;
      if (wantsPlayback.current) {
        void audio.play().then(() => setPlaying(true)).catch(() => setPlaying(false));
      }
      return;
    }
    setTrackIndex(next);
  };

  const togglePlayback = () => {
    const audio = audioRef.current;
    if (!audio) return;
    if (playing) {
      wantsPlayback.current = false;
      audio.pause();
      setPlaying(false);
      return;
    }
    wantsPlayback.current = true;
    void audio.play().then(() => setPlaying(true)).catch(() => setPlaying(false));
  };

  const changeVolume = (next: number) => {
    setVolume(next);
    if (next > 0) lastAudibleVolume.current = next;
    if (audioRef.current) audioRef.current.volume = next;
    try {
      localStorage.setItem("helt-bgm-volume", String(next));
    } catch {
      // Keep the in-memory value when storage is unavailable.
    }
  };

  const toggleMute = () => changeVolume(volume > 0 ? 0 : lastAudibleVolume.current);

  const placePlayer = (next: MusicPlayerPosition) => {
    positionRef.current = next;
    setPlayerPosition(next);
  };

  const persistPlayerLayout = (
    nextPosition: MusicPlayerPosition | null,
    nextMinimized: boolean,
    nextDock: MusicPlayerDock,
  ) => {
    try {
      localStorage.setItem(MUSIC_PLAYER_LAYOUT_KEY, JSON.stringify({
        position: nextPosition,
        expandedPosition: expandedPosition.current,
        minimized: nextMinimized,
        dockedSide: nextDock,
      }));
    } catch {
      // Keep the layout in memory when storage is unavailable.
    }
  };

  const beginPlayerDrag = (event: ReactPointerEvent<HTMLElement>) => {
    if (event.button !== 0 || (event.target as Element).closest("button, input, label, a")) return;
    const player = playerRef.current;
    if (!player) return;
    const rect = player.getBoundingClientRect();
    dragState.current = {
      pointerId: event.pointerId,
      offsetX: event.clientX - rect.left,
      offsetY: event.clientY - rect.top,
    };
    event.preventDefault();
    setDragging(true);
    dragCleanup.current?.();

    const cleanup = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
      dragCleanup.current = null;
    };
    const move = (pointerEvent: PointerEvent) => {
      const drag = dragState.current;
      const activePlayer = playerRef.current;
      if (!drag || drag.pointerId !== pointerEvent.pointerId || !activePlayer) return;
      pointerEvent.preventDefault();
      const activeRect = activePlayer.getBoundingClientRect();
      if (minimized) {
        const nextDock: MusicPlayerDock = pointerEvent.clientX <= document.documentElement.clientWidth / 2 ? "left" : "right";
        const next = {
          x: nextDock === "left" ? 0 : Math.max(0, document.documentElement.clientWidth - activeRect.width),
          y: Math.min(Math.max(8, pointerEvent.clientY - drag.offsetY), Math.max(8, window.innerHeight - activeRect.height - 8)),
        };
        if (dockRef.current !== nextDock) {
          dockRef.current = nextDock;
          setDockedSide(nextDock);
        }
        placePlayer(next);
        return;
      }
      placePlayer(fitMusicPlayerPosition({
        x: pointerEvent.clientX - drag.offsetX,
        y: pointerEvent.clientY - drag.offsetY,
      }, activeRect.width, activeRect.height));
    };
    const finish = (pointerEvent: PointerEvent) => {
      const drag = dragState.current;
      if (!drag || drag.pointerId !== pointerEvent.pointerId) return;
      cleanup();
      dragState.current = null;
      setDragging(false);
      if (!minimized && positionRef.current) expandedPosition.current = positionRef.current;
      persistPlayerLayout(positionRef.current, minimized, dockRef.current);
    };
    window.addEventListener("pointermove", move, { passive: false });
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", finish);
    dragCleanup.current = cleanup;
  };

  const togglePlayerMinimized = () => {
    const player = playerRef.current;
    if (!player) return;
    const rect = player.getBoundingClientRect();
    const viewportWidth = document.documentElement.clientWidth;
    if (!minimized) {
      const nextDock: MusicPlayerDock = rect.left + rect.width / 2 <= viewportWidth / 2 ? "left" : "right";
      const currentExpanded = { x: rect.left, y: rect.top };
      const next = {
        x: nextDock === "left" ? 0 : Math.max(0, viewportWidth - 64),
        y: Math.min(Math.max(8, rect.top), Math.max(8, window.innerHeight - 72)),
      };
      expandedPosition.current = currentExpanded;
      dockRef.current = nextDock;
      setDockedSide(nextDock);
      placePlayer(next);
      setMinimized(true);
      persistPlayerLayout(next, true, nextDock);
      return;
    }
    const compactReserve = viewportWidth <= 430 ? 64 : viewportWidth <= 768 ? 72 : 116;
    const expectedWidth = Math.min(560, Math.max(240, viewportWidth - compactReserve));
    const expectedHeight = viewportWidth <= 768 ? 82 : 92;
    const fallback = {
      x: dockedSide === "left" ? 10 : viewportWidth - expectedWidth - 10,
      y: Math.min(Math.max(8, rect.top), Math.max(8, window.innerHeight - expectedHeight - 8)),
    };
    const next = fitMusicPlayerPosition(expandedPosition.current || fallback, expectedWidth, expectedHeight);
    placePlayer(next);
    setMinimized(false);
    persistPlayerLayout(next, false, dockedSide);
  };

  if (!activePlaylist || !track) return null;
  const effectiveDuration = Number.isFinite(duration) && duration > 0 ? duration : 0;
  const progressMax = effectiveDuration || Math.max(1, currentTime);
  const progressPercent = Math.min(100, Math.max(0, (currentTime / progressMax) * 100));
  const progressStyle = { "--range-fill": `${progressPercent}%` } as CSSProperties;
  const volumeStyle = { "--range-fill": `${volume * 100}%` } as CSSProperties;
  const playerStyle = playerPosition ? {
    left: playerPosition.x,
    top: playerPosition.y,
    right: "auto",
    bottom: "auto",
  } as CSSProperties : undefined;

  return <aside
    ref={playerRef}
    className={cx(
      "background-music-player",
      playing && "is-playing",
      dragging && "is-dragging",
      minimized && "is-minimized",
      playerPosition && "has-custom-position",
      minimized && `dock-${dockedSide}`,
    )}
    style={playerStyle}
    aria-label={`背景音乐：${activePlaylist.name}`}
    onPointerDown={beginPlayerDrag}
  >
    <audio
      ref={audioRef}
      src={track.url}
      preload="metadata"
      onPlay={() => setPlaying(true)}
      onPause={() => setPlaying(false)}
      onEnded={() => advance(1)}
      onTimeUpdate={(event) => setCurrentTime(event.currentTarget.currentTime)}
      onLoadedMetadata={(event) => setDuration(Number.isFinite(event.currentTarget.duration) ? event.currentTarget.duration : track.duration_s)}
      onError={() => setPlaying(false)}
    />
    <span className="background-music-drag-cue" aria-hidden="true"><i /><i /><i /><i /><i /><i /></span>
    <button type="button" className="background-music-collapse" onClick={togglePlayerMinimized} aria-label={minimized ? "展开音乐播放器" : "收起音乐播放器到边缘"} aria-expanded={!minimized} title={minimized ? "展开播放器" : "收起到边缘"}>
      {minimized
        ? <svg viewBox="0 0 24 24"><path d="M9 4H4v5m11-5h5v5M4 15v5h5m11-5v5h-5" /></svg>
        : <svg viewBox="0 0 24 24"><path d="M6 7h12v10H6zM9 12h6" /></svg>
      }
    </button>
    <div className="background-music-art" aria-hidden="true">
      <span className="background-music-disc">
        <svg viewBox="0 0 24 24"><path d="M9.5 17.5V7.2l8-1.7v9.2M9.5 17.5c0 1.4-1.3 2.5-3 2.5s-3-1.1-3-2.5 1.3-2.5 3-2.5 3 1.1 3 2.5Zm8-2.8c0 1.4-1.3 2.5-3 2.5s-3-1.1-3-2.5 1.3-2.5 3-2.5 3 1.1 3 2.5Z" /></svg>
      </span>
      <span className="background-music-equalizer"><i /><i /><i /><i /></span>
    </div>
    <div className="background-music-copy" aria-live="polite">
      <small><i />{activePlaylist.name}<em>{String(trackIndex + 1).padStart(2, "0")} / {String(tracks.length).padStart(2, "0")}</em></small>
      <b title={track.title}>{track.title}</b>
      <span>{track.artist || "未知艺人"}</span>
    </div>
    <div className="background-music-controls">
      <button type="button" onClick={() => advance(-1)} aria-label="上一首" title="上一首"><svg viewBox="0 0 24 24"><path d="M6 5v14M18 6l-8 6 8 6V6Z" /></svg></button>
      <button type="button" className="background-music-toggle" onClick={togglePlayback} aria-label={playing ? "暂停背景音乐" : "播放背景音乐"} aria-pressed={playing}>{playing
        ? <svg viewBox="0 0 24 24"><path d="M8 6v12M16 6v12" /></svg>
        : <svg viewBox="0 0 24 24"><path className="play-shape" d="m9 6 9 6-9 6V6Z" /></svg>
      }</button>
      <button type="button" onClick={() => advance(1)} aria-label="下一首" title="下一首"><svg viewBox="0 0 24 24"><path d="m6 6 8 6-8 6V6Zm12-1v14" /></svg></button>
    </div>
    <div className="background-music-timeline">
      <span>{formatAudioTime(currentTime)}</span>
      <label className="background-music-progress">
        <span className="sr-only">播放进度</span>
        <input style={progressStyle} type="range" min={0} max={progressMax} step={.1} value={Math.min(currentTime, progressMax)} onChange={(event) => {
          const next = Number(event.target.value);
          if (audioRef.current) audioRef.current.currentTime = next;
          setCurrentTime(next);
        }} />
      </label>
      <span>{formatAudioTime(effectiveDuration)}</span>
    </div>
    <div className="background-music-volume">
      <button type="button" onClick={toggleMute} aria-label={volume === 0 ? "恢复背景音乐音量" : "静音背景音乐"} aria-pressed={volume === 0} title={volume === 0 ? "恢复音量" : "静音"}>
        <svg viewBox="0 0 24 24"><path d="M5 10v4h3l4 3V7l-4 3H5Z" /><path className="volume-wave" d={volume === 0 ? "m16 10 4 4m0-4-4 4" : "M16 9.5c1.4 1.4 1.4 3.6 0 5M18.5 7c2.8 2.8 2.8 7.2 0 10"} /></svg>
      </button>
      <label>
        <span className="sr-only">背景音乐音量</span>
        <input style={volumeStyle} type="range" min={0} max={1} step={.05} value={volume} onChange={(event) => changeVolume(Number(event.target.value))} />
      </label>
    </div>
  </aside>;
}

function EasterEgg({ onClose }: { onClose: () => void }) {
  const raiment = useRaiment();
  useEffect(() => {
    const close = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [onClose]);
  return <div className="easter-egg" role="dialog" aria-modal="true" aria-label="隐藏彩蛋" onClick={onClose}><div onClick={(e) => e.stopPropagation()}><Image src={raiment.cover} width={5120} height={2160} sizes="(max-width: 760px) 100vw, 760px" unoptimized alt="" /><div className="dialog-box"><b>{raiment.kanban.displayName}</b><p>能抵达这里，说明你的意志相当坚定，Master。今晚的晚餐，就由胜者决定吧。</p></div><button onClick={onClose}>收起令咒</button></div></div>;
}

function ThemeSwitch({ onClick, compact = false }: { onClick: () => void; compact?: boolean }) {
  const catalog = useContext(RaimentContext);
  const current = resolveRaiment(catalog);
  const currentIndex = catalog.order.indexOf(current.id);
  const nextId = catalog.order[(currentIndex + 1) % catalog.order.length] || catalog.order[0];
  const next = catalog.items[nextId] || current;
  return (
    <button className={cx("theme-switch", compact && "compact")} onClick={onClick} aria-label={`切换到${next.name}`}>
      <span className="theme-label active">{current.name}</span>
      <span className="switch-track"><i /></span>
      {!compact && <span className="theme-label muted">{next.name}</span>}
    </button>
  );
}

function SiteBrand({ href = "/", suffix }: { href?: string; suffix?: React.ReactNode }) {
  const site = useSite();
  const text = site.basic.name || "helt.";
  const stem = text.endsWith(".") ? text.slice(0, -1) : text;
  return <Link href={href} className={cx("brand", site.basic.logo_url && "image-brand")} aria-label={`${text} 首页`}>
    {site.basic.logo_url
      ? <Image src={site.basic.logo_url} width={360} height={96} unoptimized alt={text} />
      : <>{stem}{text.endsWith(".") && <span>.</span>}</>}
    {suffix}
  </Link>;
}

function TopNav({ pathname, toggleTheme, menuOpen, setMenuOpen, setSearchOpen, floating = false, elevated = false }: { pathname: string; toggleTheme: () => void; menuOpen: boolean; setMenuOpen: (v: boolean) => void; searchOpen: boolean; setSearchOpen: (v: boolean) => void; floating?: boolean; elevated?: boolean }) {
  return (
    <header className={cx("top-nav", floating && "home-touchbar", elevated && "is-elevated", menuOpen && "menu-open")}>
      <SiteBrand />
      <nav id="primary-navigation" aria-label="主导航" className={cx("main-nav", menuOpen && "open")}>
        {navItems.map(([href, label]) => <Link key={href} href={href} aria-current={pathname === href ? "page" : undefined} className={pathname === href ? "active" : ""} onClick={() => setMenuOpen(false)}>{label}</Link>)}
        <div className="mobile-nav-actions">
          <button onClick={() => { setMenuOpen(false); setSearchOpen(true); }} aria-label="搜索文章">⌕ <span>搜索文章</span></button>
          <ThemeSwitch onClick={toggleTheme} />
        </div>
      </nav>
      <div className="nav-actions">
        <button className="search-button" onClick={() => setSearchOpen(true)} aria-label="搜索文章">⌕ <span>搜索文章…</span></button>
        <ThemeSwitch onClick={toggleTheme} compact />
        <button className="menu-button" onClick={() => setMenuOpen(!menuOpen)} aria-label={menuOpen ? "关闭菜单" : "打开菜单"} aria-expanded={menuOpen} aria-controls="primary-navigation">{menuOpen ? "×" : "☰"}</button>
      </div>
    </header>
  );
}

function HomePage({ toggleTheme, notify, onSearch, siteFeaturesReady }: { toggleTheme: () => void; notify: Notify; onSearch: () => void; siteFeaturesReady: boolean }) {
  const raiment = useRaiment();
  const site = useSite();
  const [page, setPage] = useState(1);
  const [posts, setPosts] = useState<Post[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [menuOpen, setMenuOpen] = useState(false);
  const [navElevated, setNavElevated] = useState(false);
  const [voicePlaying, setVoicePlaying] = useState(false);
  const [voiceSource, setVoiceSource] = useState("");
  const [voiceSeconds, setVoiceSeconds] = useState(0);
  const [voiceDuration, setVoiceDuration] = useState(0);
  const [dialoguePosition, setDialoguePosition] = useState({ raimentId: "", index: 0 });
  const voiceRef = useRef<HTMLAudioElement | null>(null);
  const pageCount = Math.max(1, Math.ceil(total / 4));
  useEffect(() => {
    const controller = new AbortController();
    fetch(`/api/v1/articles?page=${page}&per_page=4`, { signal: controller.signal })
      .then(async (response) => {
        if (!response.ok) throw new Error(await responseMessage(response, "文章列表加载失败"));
        return response.json() as Promise<ArticleListPayload>;
      })
      .then((payload) => {
        setPosts(payload.items);
        setTotal(payload.total);
        setError("");
      })
      .catch((reason) => {
        if (!(reason instanceof DOMException && reason.name === "AbortError")) {
          setError(reason instanceof Error ? reason.message : "文章列表加载失败");
        }
      })
      .finally(() => setLoading(false));
    return () => controller.abort();
  }, [page]);
  const visiblePosts = posts;
  const commentCountKey = visiblePosts.map((post) => post.slug).join("|");
  useArtalkCommentCounts(siteFeaturesReady && site.features.comments && !loading && !error ? commentCountKey : "");
  const enter = () => document.getElementById("articles")?.scrollIntoView({ behavior: "smooth" });
  useEffect(() => {
    const updateNav = () => setNavElevated(window.scrollY > 56);
    updateNav();
    window.addEventListener("scroll", updateNav, { passive: true });
    return () => window.removeEventListener("scroll", updateNav);
  }, []);
  const coverDialogues = raiment.coverDialogue
    .split(/\r?\n/)
    .map((item) => item.trim())
    .filter(Boolean);
  const dialogueIndex = dialoguePosition.raimentId === raiment.id ? dialoguePosition.index : 0;
  const activeDialogue = coverDialogues[dialogueIndex % Math.max(1, coverDialogues.length)] || raiment.coverSubtitle;
  const nextDialogue = useCallback(() => {
    setDialoguePosition((current) => {
      const currentIndex = current.raimentId === raiment.id ? current.index : 0;
      return {
        raimentId: raiment.id,
        index: coverDialogues.length > 1 ? (currentIndex + 1) % coverDialogues.length : 0,
      };
    });
  }, [raiment.id, coverDialogues.length]);
  useEffect(() => {
    if (coverDialogues.length < 2) return;
    const timer = window.setInterval(nextDialogue, 6000);
    return () => window.clearInterval(timer);
  }, [raiment.id, coverDialogues.length, nextDialogue]);
  useEffect(() => {
    const source = raiment.coverVoiceUrl;
    if (!source) {
      voiceRef.current?.pause();
      voiceRef.current = null;
      return;
    }
    const audio = new Audio(source);
    audio.preload = "auto";
    voiceRef.current?.pause();
    voiceRef.current = audio;
    let active = true;
    const retryAfterInteraction = () => {
      if (active && audio.paused) void audio.play().catch(() => undefined);
    };
    const removeUnlockListeners = () => {
      window.removeEventListener("pointerdown", retryAfterInteraction);
      window.removeEventListener("click", retryAfterInteraction);
      window.removeEventListener("keydown", retryAfterInteraction);
    };
    audio.ontimeupdate = () => setVoiceSeconds(Math.floor(audio.currentTime || 0));
    audio.onloadedmetadata = () => setVoiceDuration(Math.floor(audio.duration || 0));
    audio.onplay = () => {
      removeUnlockListeners();
      setVoiceSource(source);
      setVoicePlaying(true);
      setVoiceSeconds(Math.floor(audio.currentTime || 0));
    };
    audio.onpause = () => setVoicePlaying(false);
    audio.onended = () => {
      setVoicePlaying(false);
      setVoiceSeconds(0);
    };
    window.addEventListener("pointerdown", retryAfterInteraction, { once: true });
    window.addEventListener("click", retryAfterInteraction, { once: true });
    window.addEventListener("keydown", retryAfterInteraction, { once: true });
    void audio.play().catch(() => undefined);
    return () => {
      active = false;
      removeUnlockListeners();
      audio.pause();
      audio.src = "";
      if (voiceRef.current === audio) voiceRef.current = null;
    };
  }, [raiment.id, raiment.coverVoiceUrl]);
  const voiceActive = voicePlaying && voiceSource === raiment.coverVoiceUrl;
  const toggleCoverVoice = () => {
    if (!raiment.coverVoiceUrl) {
      notify("当前灵衣还没有配置封面语音", "danger");
      return;
    }
    const sourceHref = new URL(raiment.coverVoiceUrl, window.location.href).href;
    let audio = voiceRef.current;
    const shouldPause = voiceActive && audio?.src === sourceHref;
    if (!audio || audio.src !== sourceHref) {
      audio?.pause();
      audio = new Audio(raiment.coverVoiceUrl);
      audio.preload = "metadata";
      setVoiceSource(raiment.coverVoiceUrl);
      setVoicePlaying(false);
      setVoiceSeconds(0);
      setVoiceDuration(0);
      audio.ontimeupdate = () => setVoiceSeconds(Math.floor(audio?.currentTime || 0));
      audio.onloadedmetadata = () => setVoiceDuration(Math.floor(audio?.duration || 0));
      audio.onpause = () => setVoicePlaying(false);
      audio.onended = () => {
        setVoicePlaying(false);
        setVoiceSeconds(0);
        notify("语音播放完成", "success");
      };
      voiceRef.current = audio;
    }
    if (shouldPause) {
      audio.pause();
      setVoicePlaying(false);
      notify("语音已暂停");
    } else {
      void audio.play().then(() => {
        if (voiceRef.current !== audio) return;
        setVoicePlaying(true);
        notify("开始播放开屏语音");
      }).catch(() => notify("语音加载或播放失败，请稍后再试", "danger"));
    }
  };
  const changePage = (next: number) => {
    const target = Math.min(pageCount, Math.max(1, next));
    if (target === page) return;
    setPage(target);
    document.querySelector(".section-intro")?.scrollIntoView({ behavior: "smooth", block: "start" });
  };
  return (
    <>
      <TopNav pathname="/" toggleTheme={toggleTheme} menuOpen={menuOpen} setMenuOpen={setMenuOpen} searchOpen={false} setSearchOpen={() => onSearch()} floating={site.features.splash} elevated={navElevated} />
      {site.features.splash && <section className="hero">
        <div className="hero-stripe stripe-one" /><div className="hero-stripe stripe-two" />
        <Image className="hero-art" src={raiment.cover} width={5120} height={2160} sizes="(max-width: 768px) 100vw, 64vw" priority unoptimized alt={`${raiment.name} 灵衣封面`} />
        <div className="hero-copy">
          <div className="eyebrow"><i /> {site.basic.hero_eyebrow}</div>
          <h1>{raiment.coverTitle}</h1>
          <p>{raiment.coverSubtitle}</p>
          <div className="hero-actions">
            {raiment.coverVoiceUrl && <button className={cx("voice-button", voiceActive && "is-playing")} aria-pressed={voiceActive} onClick={toggleCoverVoice}><b>{voiceActive ? "Ⅱ" : "▶"}</b><span className="wave"><i /><i /><i /><i /><i /><i /></span><span>{voiceActive ? `播放中 ${Math.floor(voiceSeconds / 60)}:${String(voiceSeconds % 60).padStart(2, "0")} / ${Math.floor(voiceDuration / 60)}:${String(voiceDuration % 60).padStart(2, "0")}` : raiment.coverVoiceLabel}</span></button>}
            <button className="primary-button" onClick={enter}>ENTER · 进入博客 ▾</button>
          </div>
        </div>
        <button
          type="button"
          className="dialog-box hero-dialog"
          onPointerDown={(event) => event.stopPropagation()}
          onKeyDown={(event) => event.stopPropagation()}
          onClick={(event) => { event.stopPropagation(); nextDialogue(); }}
          aria-label={coverDialogues.length > 1 ? "显示下一条封面对话" : undefined}
        >
          <b>{raiment.coverCharacterName}</b><p key={`${raiment.id}-${dialogueIndex}`}>{activeDialogue}</p><span>{coverDialogues.length > 1 ? `▼ ${dialogueIndex % coverDialogues.length + 1}/${coverDialogues.length}` : "▼"}</span>
        </button>
        <button className="scroll-cue" onClick={enter}>SCROLL ▼</button>
      </section>}
      <section id="articles" className={cx("home-content", !site.features.splash && "without-splash")}>
        {site.features.stats && <Stats />}
        <div className="section-intro reveal">
          <div><span>RECENT WRITING</span><h2>最近写下的东西</h2></div>
          <div className="section-intro-note">
            <p>{site.basic.tagline || "技术札记、生活切片与持续折腾的现场。"}</p>
            <span><b>{total.toLocaleString()}</b> PUBLIC NOTES</span>
          </div>
        </div>
        <div className="post-list">
          <div className="post-page" key={page}>
            {loading && <div className="empty-panel">正在读取文章…</div>}
            {!loading && error && <div className="empty-panel">{error}</div>}
            {!loading && !error && !visiblePosts.length && <div className="empty-panel">暂时还没有已发布文章。</div>}
            {!loading && !error && visiblePosts.map((post, index) => <PostCard key={post.id} post={post} index={index} />)}
          </div>
          <div className="pagination" aria-label="文章分页"><button onClick={() => changePage(page - 1)} disabled={page === 1} aria-label="上一页">◀</button>{Array.from({ length: pageCount }, (_, i) => i + 1).map((item) => <button key={item} className={page === item ? "current" : ""} onClick={() => changePage(item)} aria-current={page === item ? "page" : undefined}>{item}</button>)}<button onClick={() => changePage(page + 1)} disabled={page === pageCount} aria-label="下一页">▶</button></div>
        </div>
      </section>
    </>
  );
}

function Stats() {
  const { stats } = useSite();
  return <div className="stats">{[
    [stats.article_count.toLocaleString(), "文章"],
    [stats.total_words.toLocaleString(), "总字数"],
    [stats.total_visits.toLocaleString(), "访问"],
    [stats.uptime_days.toLocaleString(), "运行天数"],
  ].map(([n, l]) => <div key={l}><b>{n}</b><span>{l}</span></div>)}</div>;
}

function PostCard({ post, index = 0 }: { post: Post; index?: number }) {
  const site = useSite();
  const categoryColor = post.category?.color || "var(--accent)";
  return (
    <Link href={`/posts/${post.slug}`} className={cx("post-card", index === 0 && "featured", post.is_pinned && "pinned")} style={{ "--post-category": categoryColor } as CSSProperties}>
      <span className="post-index" aria-hidden="true">{String(index + 1).padStart(2, "0")}</span>
      {post.is_pinned && <span className="pin">置顶 PINNED</span>}
      <div className="post-main"><div className="post-meta"><span className="tag">{categoryName(post)}</span><span>{articleDate(post)}</span></div><h2>{post.title}</h2><p>{post.summary}</p><div className="post-stats"><span>{articleTime(post)}</span><span>{articleWords(post)}</span>{site.features.comments && <span>评论 <span className="artalk-comment-count" data-page-key={articleCommentKey(post.slug)}>0</span></span>}</div></div>
      {post.cover_url && <Image src={post.cover_url} width={512} height={288} sizes="240px" alt={`${post.title} 封面`} unoptimized />}
    </Link>
  );
}

function PageHeading({ title, subtitle }: { title: string; subtitle: string }) { return <div className="page-heading"><h1>{title}</h1><span>{subtitle}</span></div>; }

function ArticlePage({ slug, theme, notify, siteFeaturesReady, initialPayload }: { slug: string; theme: Theme; notify: Notify; siteFeaturesReady: boolean; initialPayload: ArticleDetailPayload | null }) {
  const site = useSite();
  const [progress, setProgress] = useState(0);
  const [liked, setLiked] = useState(false);
  const initialForSlug = initialPayload?.article.slug === slug ? initialPayload : null;
  const [requestState, setRequestState] = useState<{
    slug: string;
    payload: ArticleDetailPayload | null;
    error: string;
  }>({ slug, payload: null, error: "" });
  const payload = initialForSlug ?? (requestState.slug === slug ? requestState.payload : null);
  const error = initialForSlug ? "" : (requestState.slug === slug ? requestState.error : "");
  const [tocItems, setTocItems] = useState<TocItem[]>([]);
  const [activeTocId, setActiveTocId] = useState("article-content");
  const articleContentRef = useRef<HTMLDivElement | null>(null);
  const tocListRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    if (initialPayload?.article.slug === slug) return;
    const controller = new AbortController();
    fetch(`/api/v1/articles/${encodeURIComponent(slug)}`, { signal: controller.signal })
      .then(async (response) => {
        if (!response.ok) throw new Error(await responseMessage(response, "文章不存在"));
        return response.json() as Promise<ArticleDetailPayload>;
      })
      .then((value) => {
        setRequestState({ slug, payload: value, error: "" });
      })
      .catch((reason) => {
        if (!(reason instanceof DOMException && reason.name === "AbortError")) {
          setRequestState({
            slug,
            payload: null,
            error: reason instanceof Error ? reason.message : "文章加载失败",
          });
        }
      });
    return () => controller.abort();
  }, [initialPayload, slug]);
  useEffect(() => {
    const update = () => {
      const max = document.documentElement.scrollHeight - window.innerHeight;
      setProgress(max > 0 ? Math.min(100, (window.scrollY / max) * 100) : 0);
    };
    update(); window.addEventListener("scroll", update, { passive: true });
    return () => window.removeEventListener("scroll", update);
  }, []);
  useEffect(() => {
    const content = articleContentRef.current;
    if (!payload || !content) {
      setTocItems([]);
      setActiveTocId("article-content");
      return;
    }
    const items = buildArticleToc(content.querySelectorAll<HTMLHeadingElement>("h2, h3, h4"));
    setTocItems(items);
    setActiveTocId(items[0]?.id || "article-content");
  }, [payload]);
  useEffect(() => {
    if (!tocItems.length) return;
    let frame = 0;
    const updateActiveHeading = () => {
      window.cancelAnimationFrame(frame);
      frame = window.requestAnimationFrame(() => {
        const currentId = getActiveTocId(
          tocItems,
          (id) => document.getElementById(id)?.getBoundingClientRect().top,
        );
        setActiveTocId((current) => current === currentId ? current : currentId);
      });
    };
    updateActiveHeading();
    window.addEventListener("scroll", updateActiveHeading, { passive: true });
    window.addEventListener("resize", updateActiveHeading);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener("scroll", updateActiveHeading);
      window.removeEventListener("resize", updateActiveHeading);
    };
  }, [tocItems]);
  useEffect(() => {
    const list = tocListRef.current;
    if (!list) return;
    const activeLink = Array.from(list.querySelectorAll<HTMLElement>("[data-toc-id]"))
      .find((link) => link.dataset.tocId === activeTocId);
    if (!activeLink) return;
    const target = activeLink.offsetTop - (list.clientHeight - activeLink.offsetHeight) / 2;
    list.scrollTo({ top: Math.max(0, target), behavior: "smooth" });
  }, [activeTocId]);
  const shareArticle = async () => {
    try {
      if (!navigator.clipboard) throw new Error("Clipboard API unavailable");
      await navigator.clipboard.writeText(location.href);
      notify("文章链接已复制", "success");
    } catch {
      notify("暂时无法复制，请从地址栏复制链接", "danger");
    }
  };
  const closeArticle = () => {
    if (window.history.length > 1) window.history.back();
    else window.location.assign("/");
  };
  if (error) return <NotFound message={error} />;
  if (!payload) return <main className="empty-state"><b>◆</b><h1>正在读取文章</h1><p>请稍候，Master。</p></main>;
  const post = payload.article;
  const previousPost = payload.previous;
  const nextPost = payload.next;
  const related = payload.related;
  const displayedTocItems = tocItems.length ? tocItems : [{ id: "article-content", text: "正文", level: 2 }];
  return (
    <><div className="article-reading-bar"><i style={{ width: `${progress}%` }} /></div><main className="page-wrap article-layout page-enter" onClick={(event) => event.target === event.currentTarget && closeArticle()}>
      <article className="article-card">
        <button type="button" className="article-close" aria-label="关闭文章详情" title="关闭文章" onClick={closeArticle}>×</button>
        <header className="article-head">
          <div className="breadcrumbs"><Link href="/">首页</Link><span aria-hidden="true">/</span><Link href="/archives">文章归档</Link></div>
          <div className="article-kicker"><span>{categoryName(post)}</span><small>ARTICLE / READING</small></div>
          <h1>{post.title}</h1>
          <p className="article-summary">{post.summary}</p>
          <dl className="article-meta" aria-label="文章信息">
            <div><dt>发布于</dt><dd>{articleDate(post)}</dd></div>
            <div><dt>阅读时长</dt><dd>{articleTime(post)}</dd></div>
            <div><dt>文章字数</dt><dd>{articleWords(post)}</dd></div>
            <div><dt>阅读次数</dt><dd>{post.view_count.toLocaleString()}</dd></div>
          </dl>
          {post.tags.length > 0 && <div className="article-tags" aria-label="文章标签">{post.tags.map((tag) => <span key={tag.id}># {tag.name}</span>)}</div>}
        </header>
        {post.cover_url && <figure className="article-cover">
          <Image className="article-image" src={post.cover_url} width={1200} height={675} sizes="(max-width: 768px) 100vw, 780px" alt={`${post.title} 封面`} unoptimized />
          <figcaption><span>FEATURED IMAGE</span><b>{categoryName(post)}</b></figcaption>
        </figure>}
        <div className="article-body-divider" aria-hidden="true"><span>正文</span><i /></div>
        <MarkdownBody source={post.content_md || "这篇文章还没有正文。"} containerRef={articleContentRef} />
        <div id="article-actions" className="article-actions"><button className={liked ? "liked" : ""} onClick={() => { setLiked(!liked); notify(liked ? "已取消喜欢" : "感谢你的喜欢", "success"); }}>{liked ? "♥ 已喜欢" : "♡ 喜欢"}</button><button onClick={shareArticle}>⌁ 分享文章</button></div>
        <nav className="article-nav" aria-label="上一篇和下一篇">{previousPost ? <Link href={`/posts/${previousPost.slug}`}><small>← PREVIOUS · 上一篇</small><b>{previousPost.title}</b></Link> : <span />}{nextPost ? <Link href={`/posts/${nextPost.slug}`}><small>NEXT · 下一篇 →</small><b>{nextPost.title}</b></Link> : <span />}</nav>
        {siteFeaturesReady && (site.features.comments && payload.allow_comment
          ? <Comments slug={post.slug} title={post.title} theme={theme} />
          : <section className="comments comments-disabled"><h2>评论</h2><p>{site.features.comments ? "这篇文章已关闭评论。" : "站点当前已关闭全站评论。"}</p></section>)}
      </article>
      <aside className="article-aside">
        <nav className="toc" aria-label="文章目录">
          <b><span>目录 <small>CONTENTS</small></span><em>{Math.round(progress)}%</em></b>
          <div className="toc-links" ref={tocListRef}>
            {displayedTocItems.map((item) => <Link
              key={item.id}
              href={`#${item.id}`}
              data-toc-id={item.id}
              className={cx("toc-link", `toc-level-${item.level}`, activeTocId === item.id && "active")}
              aria-current={activeTocId === item.id ? "location" : undefined}
            >{item.text}</Link>)}
          </div>
        </nav>
        <div className="recommend"><b><span>相关文章</span><small>RELATED</small></b>{related.length ? related.map((item, index) => <Link key={item.id} href={`/posts/${item.slug}`}><small>{String(index + 1).padStart(2, "0")}</small><span>{item.title}</span><i aria-hidden="true">↗</i></Link>) : <Link href="/archives"><small>ALL</small><span>浏览全部文章</span><i aria-hidden="true">↗</i></Link>}</div>
      </aside>
    </main></>
  );
}

function MarkdownBody({ source, containerRef }: { source: string; containerRef?: RefObject<HTMLDivElement | null> }) {
  return <div ref={containerRef} id="article-content" className="article-content markdown-renderer"><ReactMarkdown remarkPlugins={[remarkGfm]}>{source}</ReactMarkdown></div>;
}

function SectionTitle({ index, title, id }: { index: string; title: string; id?: string }) {
  return <h2 id={id} className="section-title"><i />{index}、{title}</h2>;
}

function Comments({ slug, title, theme }: { slug: string; title: string; theme: Theme }) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const artalkRef = useRef<ArtalkInstance | null>(null);
  const [loadError, setLoadError] = useState("");

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;
    let cancelled = false;
    const markFieldRequirements = () => {
      const fields = [
        { selector: 'input[name="name"]', placeholder: "昵称（必填）", required: true },
        { selector: 'input[name="email"]', placeholder: "邮箱（必填）", required: true },
        { selector: 'input[name="link"]', placeholder: "网站（选填）", required: false },
      ];
      for (const field of fields) {
        const input = container.querySelector<HTMLInputElement>(field.selector);
        if (!input) continue;
        if (input.placeholder !== field.placeholder) input.placeholder = field.placeholder;
        if (input.required !== field.required) input.required = field.required;
        input.setAttribute("aria-required", String(field.required));
      }
    };
    const fieldObserver = new MutationObserver(markFieldRequirements);
    fieldObserver.observe(container, {
      subtree: true,
      childList: true,
      attributes: true,
      attributeFilter: ["placeholder", "required"],
    });
    setLoadError("");
    void import("artalk").then(({ default: Artalk }) => {
      if (cancelled) return;
      artalkRef.current = Artalk.init({
        el: container,
        pageKey: articleCommentKey(slug),
        pageTitle: title,
        server: ARTALK_SERVER,
        site: ARTALK_SITE,
        locale: "zh-CN",
        darkMode: document.documentElement.dataset.theme === "night",
        placeholder: "写下想说的话……",
        noComment: "还没有评论，来留下第一条回应吧。",
        sendBtn: "发送评论",
        pageVote: false,
        pvAdd: false,
      });
      markFieldRequirements();
    }).catch(() => {
      if (!cancelled) setLoadError("评论组件加载失败，请刷新页面后重试。");
    });
    return () => {
      cancelled = true;
      fieldObserver.disconnect();
      artalkRef.current?.destroy();
      artalkRef.current = null;
    };
  }, [slug, title]);

  useEffect(() => {
    artalkRef.current?.setDarkMode(theme === "night");
  }, [theme]);

  return <section className="comments" aria-labelledby="article-comments-title">
    <header className="comment-section-header">
      <h2 id="article-comments-title">评论 · <span className="artalk-comment-count" data-page-key={articleCommentKey(slug)}>0</span></h2>
    </header>
    <div id="article-comments-body">
      {loadError && <p className="comment-load-error" role="alert">{loadError}</p>}
      <div ref={containerRef} className="artalk-host" />
    </div>
  </section>;
}

function ArchivesPage() {
  const [selected, setSelected] = useState<{ kind: "category" | "tag"; id: number } | null>(null);
  const [posts, setPosts] = useState<Post[]>([]);
  const [categories, setCategories] = useState<ArticleCategory[]>([]);
  const [tags, setTags] = useState<ArticleTag[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState("");
  useEffect(() => {
    const controller = new AbortController();
    const load = async () => {
      const [articleResponse, categoryResponse, tagResponse] = await Promise.all([
        fetch("/api/v1/articles?page=1&per_page=50", { signal: controller.signal }),
        fetch("/api/v1/categories", { signal: controller.signal }),
        fetch("/api/v1/tags", { signal: controller.signal }),
      ]);
      if (!articleResponse.ok || !categoryResponse.ok || !tagResponse.ok) throw new Error("归档数据加载失败");
      const articles = await articleResponse.json() as ArticleListPayload;
      const categoryPayload = await categoryResponse.json() as { items: ArticleCategory[] };
      const tagPayload = await tagResponse.json() as { items: ArticleTag[] };
      const allPosts = [...articles.items];
      const pageCount = Math.ceil(articles.total / articles.per_page);
      for (let page = 2; page <= pageCount; page += 1) {
        const response = await fetch(`/api/v1/articles?page=${page}&per_page=${articles.per_page}`, { signal: controller.signal });
        if (!response.ok) throw new Error("归档文章加载失败");
        allPosts.push(...((await response.json() as ArticleListPayload).items));
      }
      if (controller.signal.aborted) return;
      setPosts(allPosts);
      setTotal(articles.total);
      setCategories(categoryPayload.items);
      setTags(tagPayload.items);
    };
    void load()
      .catch((error) => {
        if (!(error instanceof DOMException && error.name === "AbortError")) setLoadError("归档数据暂时无法加载，请稍后重试。");
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, []);

  const categoryChoices = categories.map((item) => ({ ...item, count: posts.filter((post) => post.category?.id === item.id).length }));
  const tagChoices = tags.map((item) => ({ ...item, count: posts.filter((post) => post.tags.some((tag) => tag.id === item.id)).length }));
  const selectedChoice = selected?.kind === "category"
    ? categoryChoices.find((item) => item.id === selected.id) ?? null
    : selected?.kind === "tag"
      ? tagChoices.find((item) => item.id === selected.id) ?? null
      : null;
  const matchedPosts = selectedChoice
    ? posts.filter((post) => selected?.kind === "category"
      ? post.category?.id === selectedChoice.id
      : post.tags.some((tag) => tag.id === selectedChoice.id))
    : posts;

  const toggleSelection = (kind: "category" | "tag", id: number) => {
    setSelected((current) => current?.kind === kind && current.id === id ? null : { kind, id });
  };

  return <main className="page-wrap page-enter">
    <PageHeading title="归档" subtitle={`ARCHIVE · ${total} 篇`} />
    {loading ? <div className="archive-status" aria-live="polite"><b>LOADING</b><p>正在整理文章归档…</p></div>
      : loadError ? <div className="archive-status error" role="alert"><b>OFFLINE</b><p>{loadError}</p></div>
        : <div className="archive-grid">
          <div className="timeline-list">
            <div className="archive-timeline-heading"><h2>文章</h2><small>{matchedPosts.length} 篇</small></div>
            {selectedChoice && <div className="archive-selection" role="status"><span>{selected?.kind === "category" ? "分类筛选" : "标签筛选"}</span><b>{selected?.kind === "tag" && <i aria-hidden="true">#</i>}{selectedChoice.name}</b><p>时间线中显示 {matchedPosts.length} 篇相关内容</p><button onClick={() => setSelected(null)} aria-label={`清除${selectedChoice.name}筛选`}>清除筛选 ×</button></div>}
            {matchedPosts.length ? matchedPosts.map((post) => <Link key={post.id} href={`/posts/${post.slug}`}><time>{articleDate(post).slice(5)}</time><b>{post.title}</b><span className="tag">{categoryName(post)}</span></Link>) : <div className="archive-list-empty">{selectedChoice ? "没有匹配的已发布文章。" : "暂时还没有已发布文章。"}</div>}
          </div>
          <aside className="archive-side" aria-label="归档索引">
            <section className="archive-side-section" aria-labelledby="archive-category-index-title">
              <div className="archive-side-heading"><span><small>CATEGORIES</small><b id="archive-category-index-title">分类索引</b></span><strong>{categoryChoices.length}</strong></div>
              {categoryChoices.length ? categoryChoices.map((item) => <button key={item.id} className={selected?.kind === "category" && selected.id === item.id ? "active" : ""} aria-pressed={selected?.kind === "category" && selected.id === item.id} style={{ "--taxonomy-color": item.color || "var(--accent)" } as CSSProperties} onClick={() => toggleSelection("category", item.id)}><i aria-hidden="true" /><span>{item.name}</span><strong>{item.count}</strong><em aria-hidden="true">↗</em></button>) : <div className="archive-side-empty"><span aria-hidden="true">◇</span><p>暂无分类</p></div>}
            </section>
            <section className="archive-side-section" aria-labelledby="archive-tag-index-title">
              <div className="archive-side-heading"><span><small>TAGS</small><b id="archive-tag-index-title">标签索引</b></span><strong>{tagChoices.length}</strong></div>
              {tagChoices.length ? tagChoices.map((item) => <button key={item.id} className={selected?.kind === "tag" && selected.id === item.id ? "active archive-side-tag" : "archive-side-tag"} aria-pressed={selected?.kind === "tag" && selected.id === item.id} onClick={() => toggleSelection("tag", item.id)}><i aria-hidden="true">#</i><span>{item.name}</span><strong>{item.count}</strong><em aria-hidden="true">↗</em></button>) : <div className="archive-side-empty"><span aria-hidden="true">#</span><p>暂无标签</p></div>}
            </section>
          </aside>
        </div>}
  </main>;
}

function MomentsPage({ notify }: { notify: Notify }) {
  const [moments, setMoments] = useState<Moment[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [loadError, setLoadError] = useState("");
  const [visitor] = useState<string | null>(() => typeof window === "undefined" ? null : visitorId());
  const [liking, setLiking] = useState<number[]>([]);
  const perPage = 20;

  const loadPage = useCallback(async (targetPage: number, visitorIdValue: string | null, append: boolean, signal?: AbortSignal) => {
    const params = new URLSearchParams({ page: String(targetPage), per_page: String(perPage) });
    if (visitorIdValue) params.set("visitor_id", visitorIdValue);
    const response = await fetch(`/api/v1/moments?${params}`, { signal, cache: "no-store" });
    if (!response.ok) throw new Error(await responseMessage(response, "时间轴加载失败"));
    const payload = await response.json() as MomentListPayload;
    setMoments((items) => append ? [...items, ...payload.items] : payload.items);
    setPage(payload.page);
    setTotal(payload.total);
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    const params = new URLSearchParams({ page: "1", per_page: String(perPage) });
    if (visitor) params.set("visitor_id", visitor);
    void fetch(`/api/v1/moments?${params}`, { signal: controller.signal, cache: "no-store" })
      .then(async (response) => {
        if (!response.ok) throw new Error(await responseMessage(response, "时间轴加载失败"));
        return response.json() as Promise<MomentListPayload>;
      })
      .then((payload) => {
        setMoments(payload.items);
        setPage(payload.page);
        setTotal(payload.total);
      })
      .catch((error) => {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          setLoadError(error instanceof Error ? error.message : "时间轴加载失败");
        }
      })
      .finally(() => {
        if (!controller.signal.aborted) setLoading(false);
      });
    return () => controller.abort();
  }, [visitor]);

  const loadMore = async () => {
    if (loadingMore || moments.length >= total) return;
    setLoadingMore(true);
    try {
      await loadPage(page + 1, visitor, true);
    } catch (error) {
      notify(error instanceof Error ? error.message : "更多说说加载失败", "danger");
    } finally {
      setLoadingMore(false);
    }
  };

  const toggleLike = async (moment: Moment) => {
    if (!visitor || liking.includes(moment.id)) return;
    setLiking((items) => [...items, moment.id]);
    try {
      const response = await fetch(`/api/v1/moments/${moment.id}/like`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ visitor_id: visitor }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "点赞失败"));
      const payload = await response.json() as { like_count: number; liked: boolean };
      setMoments((items) => items.map((item) => item.id === moment.id
        ? { ...item, like_count: payload.like_count, liked_by_me: payload.liked }
        : item));
    } catch (error) {
      notify(error instanceof Error ? error.message : "点赞失败", "danger");
    } finally {
      setLiking((items) => items.filter((id) => id !== moment.id));
    }
  };

  const now = new Date();
  const thisMonth = `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
  const monthCount = moments.filter((moment) => {
    const date = new Date(moment.created_at);
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}` === thisMonth;
  }).length;
  const imageCount = moments.reduce((count, moment) => count + moment.images.length, 0);
  const latestDate = moments.length ? new Date(moments[0].created_at) : null;
  const latestCode = latestDate
    ? latestDate.toLocaleDateString("en-US", { month: "short", year: "numeric" }).toUpperCase()
    : "NO SIGNAL";
  const streamCode = latestDate
    ? latestDate.toLocaleDateString("en-US", { month: "long", year: "numeric" }).replace(" ", " / ").toUpperCase()
    : "MOMENTS";

  return <main className="page-wrap narrow page-enter moments-page">
    <PageHeading title="时间轴" subtitle={`MOMENTS · ${total} 条碎碎念`} />
    <section className="moment-overview" aria-label="时间轴概览">
      <div className="moment-overview-copy">
        <span className="moment-signal"><i aria-hidden="true" /> RECENT SIGNAL</span>
        <h2>把日常收进时间的缝隙</h2>
      </div>
      <dl>
        <div><dt>{String(monthCount).padStart(2, "0")}</dt><dd>本月动态</dd></div>
        <div><dt>{String(imageCount).padStart(2, "0")}</dt><dd>影像记录</dd></div>
        <div><dt>{latestDate ? String(latestDate.getDate()).padStart(2, "0") : "--"}</dt><dd>最近更新</dd></div>
      </dl>
      <span className="moment-overview-code" aria-hidden="true">{latestCode}</span>
    </section>

    <div className="moment-stream-heading">
      <span><i aria-hidden="true" /> {streamCode}</span>
    </div>
    {loading ? <div className="moment-status" aria-live="polite"><b>LOADING</b><p>正在接收时间轴信号…</p></div>
      : loadError ? <div className="moment-status error" role="alert"><b>OFFLINE</b><p>{loadError}</p></div>
        : !moments.length ? <div className="moment-status"><b>EMPTY</b><p>还没有说说，第一条日常正在路上。</p></div>
          : <div className="moments">
            {moments.map((moment, index) => {
              const date = new Date(moment.created_at);
              const day = `${String(date.getMonth() + 1).padStart(2, "0")}.${String(date.getDate()).padStart(2, "0")}`;
              const time = `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
              return <article key={moment.id} style={{ "--moment-order": index } as CSSProperties}>
                <time className="moment-date" dateTime={moment.created_at}>
                  <span>{date.getFullYear()}</span>
                  <b>{day}</b>
                  <small>{time}</small>
                </time>
                <i className="moment-node" aria-hidden="true"><span /></i>
                <div className="moment-card">
                  <header>
                    <span className="moment-mood"><i aria-hidden="true">✦</i>碎碎念</span>
                    <small>NO. {String(total - index).padStart(2, "0")}</small>
                  </header>
                  {moment.content && <p className="moment-content">{moment.content}</p>}
                  {moment.tags.length > 0 && <div className="moment-tags" aria-label="说说标签">{moment.tags.map((tag) => <span key={tag.id}># {tag.name}</span>)}</div>}
                  {moment.images.length > 0 && <div className={cx("moment-gallery", `count-${Math.min(moment.images.length, 4)}`)}>
                    {moment.images.map((image, imageIndex) => <Image key={image.asset_id} src={image.url} width={960} height={720} sizes="(max-width: 700px) 90vw, 620px" alt={image.alt_text || `说说配图 ${imageIndex + 1}`} unoptimized />)}
                  </div>}
                  <footer className="moment-actions">
                    <span><i aria-hidden="true" /> LIFE LOG</span>
                    <button disabled={!visitor || liking.includes(moment.id)} className={moment.liked_by_me ? "liked" : ""} aria-label={moment.liked_by_me ? "取消点赞" : "点赞"} aria-pressed={moment.liked_by_me} onClick={() => void toggleLike(moment)}>
                      <i aria-hidden="true">{moment.liked_by_me ? "♥" : "♡"}</i>
                      <span>{moment.like_count}</span>
                    </button>
                  </footer>
                </div>
              </article>;
            })}
            {moments.length < total && <button className="moment-load-more" disabled={loadingMore} onClick={() => void loadMore()}>{loadingMore ? "正在接收…" : `继续读取 · 还有 ${total - moments.length} 条`}</button>}
            <div className="moment-tail" aria-hidden="true"><i /> TO BE CONTINUED</div>
          </div>}
  </main>;
}

function AboutPage({ notify }: { notify: Notify }) {
  const site = useSite();
  const [profile, setProfile] = useState<PublicProfile | null>(null);
  const [loadError, setLoadError] = useState("");
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    const controller = new AbortController();
    const load = () => {
      setLoadError("");
      void fetch("/api/v1/profile", {
        headers: { accept: "application/json" },
        cache: "no-store",
        signal: controller.signal,
      })
        .then(async (response) => {
          if (!response.ok) throw new Error("个人资料读取失败");
          return response.json() as Promise<PublicProfile>;
        })
        .then(setProfile)
        .catch((error) => {
          if (!(error instanceof DOMException && error.name === "AbortError")) {
            setLoadError("暂时无法读取个人资料，请稍后重试。");
            notify("暂时无法读取个人资料");
          }
        });
    };
    load();
    window.addEventListener("helt:profile-updated", load);
    return () => {
      controller.abort();
      window.removeEventListener("helt:profile-updated", load);
    };
  }, [notify, reloadKey]);

  if (!profile) {
    return <main className="page-wrap about-loading page-enter">
      <PageHeading title="关于我" subtitle="ABOUT · MASTER PROFILE" />
      {loadError
        ? <div className="about-load-state error" role="alert"><b>资料暂时失联</b><p>{loadError}</p><button type="button" onClick={() => setReloadKey((value) => value + 1)}>重新读取</button></div>
        : <div className="about-load-state" role="status"><i aria-hidden="true">◆</i><b>正在读取真实资料</b><p>头像、介绍与站点数据正在同步。</p></div>}
    </main>;
  }

  const copyEmail = async () => {
    if (!profile.email) {
      notify("尚未设置联系邮箱");
      return;
    }
    try {
      if (!navigator.clipboard) throw new Error("Clipboard API unavailable");
      await navigator.clipboard.writeText(profile.email);
      notify("邮箱已复制", "success");
    } catch {
      notify(`邮箱：${profile.email}`, "normal");
    }
  };
  const displayName = profile.about.display_name || profile.username;
  const avatarUrl = profile.avatar_url || DEFAULT_PROFILE_AVATAR_URL;
  const socialMark = (label: string) => label.trim().slice(0, 2).toUpperCase();
  return <main className="page-wrap about-layout page-enter">
    <aside className="profile-card">
      <div className="profile-card-signal" aria-hidden="true"><span>PROFILE</span><i /></div>
      <div className="avatar"><Image src={avatarUrl} width={256} height={256} sizes="128px" unoptimized alt={`${displayName} 的头像`} style={{ objectPosition: `${50 + profile.avatar_crop_x * 50}% ${50 + profile.avatar_crop_y * 50}%`, transform: `scale(${profile.avatar_crop_zoom || 1})` }} /></div>
      <h1>{displayName}</h1>
      {profile.about.bio && <p>{profile.about.bio}</p>}
      {(profile.about.status || profile.about.location) && <div className="profile-presence">
        {profile.about.status && <span><i aria-hidden="true" />{profile.about.status}</span>}
        {profile.about.location && <small>⌖ {profile.about.location}</small>}
      </div>}
      <div className="profile-stats">
        <span><b>{profile.stats.article_count.toLocaleString()}</b>篇文章</span>
        <span><b>{profile.stats.uptime_days.toLocaleString()}</b>天相伴</span>
      </div>
      {(profile.about.socials.length > 0 || profile.email) && <div className="profile-links">
        {profile.about.socials.map((social) => <a key={`${social.label}-${social.url}`} href={social.url} target="_blank" rel="me noreferrer" title={social.label}><i className={social.icon_url ? "has-image" : ""} aria-hidden="true">{social.icon_url ? <Image src={social.icon_url} width={28} height={28} unoptimized alt="" /> : socialMark(social.label)}</i><span>{social.label}</span><b aria-hidden="true">↗</b></a>)}
        {profile.email && <button type="button" onClick={copyEmail}><i aria-hidden="true">✉</i><span>复制联系邮箱</span><b aria-hidden="true">＋</b></button>}
      </div>}
    </aside>
    <div className="about-content">
      <PageHeading title="关于我" subtitle="ABOUT · MASTER PROFILE" />
      {profile.about.intro_md && <section className="about-intro-card">
        <div className="about-intro-label"><span>HELLO / 你好</span><b>{displayName}</b></div>
        <div className="about-markdown markdown-renderer"><ReactMarkdown remarkPlugins={[remarkGfm]}>{profile.about.intro_md}</ReactMarkdown></div>
      </section>}
      {profile.about.skills.length > 0 && <section className="about-section">
        <SectionTitle index="01" title="技能与兴趣" />
        <div className="skill-grid">{profile.about.skills.map((skill, index) => <span key={skill}><i>{String(index + 1).padStart(2, "0")}</i>{skill}</span>)}</div>
      </section>}
      {(profile.about.site_note || site.basic.tagline) && <section className="about-section about-site-note">
        <SectionTitle index={profile.about.skills.length ? "02" : "01"} title="关于本站" />
        <div><span aria-hidden="true">◆</span><p>{profile.about.site_note || site.basic.tagline}</p><small>{site.basic.name} · {profile.stats.article_count.toLocaleString()} ARTICLES · {profile.stats.uptime_days.toLocaleString()} DAYS</small></div>
      </section>}
    </div>
  </main>;
}

type PublicFriend = {
  name: string;
  url: string;
  avatar_url: string;
  description: string;
};

type PublicFriendPayload = {
  page: number;
  per_page: number;
  total: number;
  items: PublicFriend[];
};

function FriendsPage({ notify }: { notify: Notify }) {
  const [selected, setSelected] = useState<PublicFriend | null>(null);
  const [friends, setFriends] = useState<PublicFriend[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState("");
  const [submitError, setSubmitError] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [sent, setSent] = useState(false);

  useEffect(() => {
    const controller = new AbortController();
    const load = async () => {
      try {
        const response = await fetch("/api/v1/friends?per_page=50", {
          headers: { accept: "application/json" },
          signal: controller.signal,
        });
        if (!response.ok) throw new Error(await responseMessage(response, "读取友链失败"));
        const payload = await response.json() as PublicFriendPayload;
        setFriends(payload.items);
        setTotal(payload.total);
      } catch (error) {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          setLoadError(error instanceof Error ? error.message : "读取友链失败");
        }
      } finally {
        if (!controller.signal.aborted) setLoading(false);
      }
    };
    void load();
    return () => controller.abort();
  }, []);

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const form = event.currentTarget;
    const data = new FormData(form);
    setSubmitting(true);
    setSent(false);
    setSubmitError("");
    try {
      const response = await fetch("/api/v1/friends", {
        method: "POST",
        headers: { "content-type": "application/json", accept: "application/json" },
        body: JSON.stringify({
          name: String(data.get("name") || ""),
          url: String(data.get("url") || ""),
          avatar_url: String(data.get("avatar_url") || ""),
          contact_email: String(data.get("contact_email") || ""),
          description: String(data.get("description") || ""),
        }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "提交友链申请失败"));
      form.reset();
      setSent(true);
      notify("友链申请已提交，审核通过后会公开展示", "success");
    } catch (error) {
      const message = error instanceof Error ? error.message : "提交友链申请失败";
      setSubmitError(message);
      notify(message, "danger");
    } finally {
      setSubmitting(false);
    }
  };

  useEffect(() => {
    if (!selected) return;
    const close = (event: KeyboardEvent) => event.key === "Escape" && setSelected(null);
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [selected]);

  return <main className="page-wrap page-enter">
    <PageHeading title="友情链接" subtitle={`FRIENDS · ${total} 位`} />
    {loading ? <div className="friends-empty" role="status">正在读取友链…</div>
      : loadError ? <div className="friends-empty error" role="alert">{loadError}</div>
        : friends.length ? <div className="friends-grid">{friends.map((friend) => <button key={friend.url} className="friend-card" onClick={() => setSelected(friend)}>
          <span className="friend-avatar">{friend.avatar_url ? <Image src={friend.avatar_url} width={52} height={52} unoptimized alt="" /> : friend.name.slice(0, 1).toUpperCase()}</span>
          <span><b>{friend.name}</b><p>{friend.description || "这位朋友还没有留下简介。"}</p></span><i>→</i>
        </button>)}</div>
          : <div className="friends-empty"><b>友链正在生长</b><p>还没有已通过的友链，欢迎提交第一份申请。</p></div>}
    <form className="friend-form dialog-box" onSubmit={submit}>
      <b>申请友链</b>
      <p>交换的不只是链接，也是彼此在网络世界留下的一盏灯。申请会先进入后台审核，不会立即公开。</p>
      <div className="form-grid">
        <input aria-label="站点名称" name="name" maxLength={100} placeholder="站点名称" required />
        <input aria-label="站点地址" name="url" maxLength={2048} placeholder="站点地址" type="url" required />
        <input aria-label="头像地址" name="avatar_url" maxLength={2048} placeholder="头像地址（可选）" type="url" />
        <input aria-label="联系邮箱" name="contact_email" maxLength={254} placeholder="联系邮箱（仅管理员可见）" type="email" required />
      </div>
      <textarea aria-label="站点介绍" name="description" maxLength={500} placeholder="一句话介绍你的小站（最多 500 字）" required />
      <button className="primary-button" disabled={submitting}>{submitting ? "正在提交…" : "提交申请"}</button>
      {sent && <span className="success" role="status">申请已提交，等待审核。</span>}
      {submitError && <span className="friend-submit-error" role="alert">{submitError}</span>}
    </form>
    {selected && <div className="friend-drawer" role="dialog" aria-modal="true" aria-labelledby="friend-drawer-title">
      <button aria-label="关闭友链详情" onClick={() => setSelected(null)}>×</button>
      <span className="friend-avatar">{selected.avatar_url ? <Image src={selected.avatar_url} width={52} height={52} unoptimized alt="" /> : selected.name.slice(0, 1).toUpperCase()}</span>
      <small>FRIEND PROFILE</small><h2 id="friend-drawer-title">{selected.name}</h2><p>{selected.description || "这位朋友还没有留下简介。"}</p>
      <a className="primary-button" href={selected.url} target="_blank" rel="noreferrer">访问站点 ↗</a>
    </div>}
  </main>;
}

function NotFound({ message }: { message?: string } = {}) { return <main className="empty-state"><b>404</b><h1>前方并非约定之地</h1><p>{message || "Master，这条路径似乎不存在。"}</p><Link href="/" className="primary-button">返回首页</Link></main>; }

function SearchOverlay({ onClose }: { onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<Post[]>([]);
  useEffect(() => {
    if (!query.trim()) {
      return;
    }
    const controller = new AbortController();
    const timer = window.setTimeout(() => {
      fetch(`/api/v1/articles?search=${encodeURIComponent(query.trim())}&page=1&per_page=6`, { signal: controller.signal })
        .then((response) => response.ok ? response.json() as Promise<ArticleListPayload> : null)
        .then((payload) => setResult(payload?.items || []))
        .catch(() => undefined);
    }, 180);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [query]);
  useEffect(() => { const close = (e: KeyboardEvent) => e.key === "Escape" && onClose(); window.addEventListener("keydown", close); return () => window.removeEventListener("keydown", close); }, [onClose]);
  return <div className="search-overlay" role="dialog" aria-modal="true" aria-label="搜索文章" onClick={onClose}><div className="search-panel" onClick={(e) => e.stopPropagation()}><div><span aria-hidden="true">⌕</span><input aria-label="搜索关键词" autoFocus value={query} onChange={(e) => { setQuery(e.target.value); if (!e.target.value.trim()) setResult([]); }} placeholder="搜索标题、分类或关键词…" /><button onClick={onClose} aria-label="关闭搜索">×</button></div>{query ? <section aria-live="polite">{result.length ? result.map((p) => <Link key={p.id} href={`/posts/${p.slug}`}><span className="tag">{categoryName(p)}</span><b>{p.title}</b><small>{articleDate(p)}</small></Link>) : <p>没有找到相关内容，换个关键词试试。</p>}</section> : <div className="search-suggestions"><small>热门关键词</small><div>{["React", "Fate", "Live2D", "博客重构"].map((item) => <button key={item} onClick={() => setQuery(item)}>{item}</button>)}</div></div>}<small>ESC 关闭 · 输入关键词实时检索文章库</small></div></div>;
}

function Footer() {
  const site = useSite();
  const year = new Date().getFullYear();
  const copyright = (site.basic.footer_copyright ?? "© 2020—{year} {site_name} · POWERED BY REACT")
    .replaceAll("{year}", String(year))
    .replaceAll("{site_name}", site.basic.name)
    .trim();
  const footerMeta = [copyright, site.basic.icp.trim()].filter(Boolean).join(" · ");
  return <footer><SiteBrand />{site.basic.footer_text && <p className="site-footer-text">{site.basic.footer_text}</p>}{footerMeta && <span>{footerMeta}</span>}</footer>;
}

function AdminRouter({ pathname, theme, toggleTheme, notify }: { pathname: string; theme: Theme; toggleTheme: () => void; notify: Notify }) {
  if (pathname === "/admin/login") return <AdminLogin />;
  return <AdminSessionGate>{(admin) => <AdminLayout pathname={pathname} theme={theme} toggleTheme={toggleTheme} notify={notify} admin={admin} />}</AdminSessionGate>;
}

function AdminSessionGate({ children }: { children: (admin: AdminIdentity) => React.ReactNode }) {
  const [admin, setAdmin] = useState<AdminIdentity | null>(null);

  useEffect(() => {
    const controller = new AbortController();
    const verify = async () => {
      try {
        const session = await fetch("/api/v1/admin/auth/me", {
          credentials: "include",
          headers: { accept: "application/json" },
          signal: controller.signal,
        });
        if (session.ok && isJsonResponse(session)) {
          setAdmin(await session.json() as AdminIdentity);
          return;
        }
        const refreshed = await fetch("/api/v1/admin/auth/refresh", {
          method: "POST",
          credentials: "include",
          headers: { accept: "application/json" },
          signal: controller.signal,
        });
        if (refreshed.ok && isJsonResponse(refreshed)) {
          const payload = await refreshed.json() as { admin: AdminIdentity };
          setAdmin(payload.admin);
          return;
        }
        window.location.replace("/admin/login");
      } catch (error) {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          window.location.replace("/admin/login");
        }
      }
    };
    void verify();
    return () => controller.abort();
  }, []);

  if (!admin) {
    return <main className="admin-session-loading" role="status"><span aria-hidden="true">◆</span><p>正在验证令咒…</p></main>;
  }
  return children(admin);
}

const adminNav = [["/admin", "▦", "仪表盘"], ["/admin/articles", "▤", "文章管理"], ["/admin/comments", "◫", "审核"], ["/admin/assets", "▧", "素材库"], ["/admin/raiments", "♙", "灵衣"], ["/admin/llm", "✦", "LLM"], ["/admin/playlists", "♫", "歌单"], ["/admin/settings", "⚙", "站点设置"]];

function AdminLayout({ pathname, theme, toggleTheme, notify, admin }: { pathname: string; theme: Theme; toggleTheme: () => void; notify: Notify; admin: AdminIdentity }) {
  const [commandOpen, setCommandOpen] = useState(false);
  const [accountOpen, setAccountOpen] = useState(false);
  const [currentAdmin, setCurrentAdmin] = useState(admin);
  const current = adminNav.find(([href]) => pathname === href)?.[2] || (pathname.includes("articles") ? "文章编辑器" : "仪表盘");
  let content: React.ReactNode;
  if (pathname === "/admin") content = <Dashboard notify={notify} />;
  else if (pathname === "/admin/articles") content = <ContentManager notify={notify} />;
  else if (pathname.includes("/admin/articles/")) content = <ArticleEditor pathname={pathname} theme={theme} notify={notify} />;
  else if (pathname === "/admin/comments") content = <ReviewManager notify={notify} />;
  else if (pathname === "/admin/assets") content = <AssetManager notify={notify} />;
  else if (pathname === "/admin/raiments" || pathname === "/admin/appearance") content = <RaimentSettings notify={notify} />;
  else if (pathname === "/admin/llm" || pathname === "/admin/kanban") content = <LlmSettings notify={notify} />;
  else if (pathname === "/admin/playlists" || pathname === "/admin/media") content = <PlaylistSettings notify={notify} />;
  else content = <SiteSettings notify={notify} />;
  useEffect(() => {
    const successVoice = sessionStorage.getItem("helt-login-success-voice");
    if (!successVoice) return;
    sessionStorage.removeItem("helt-login-success-voice");
    const audio = new Audio(successVoice);
    audio.preload = "auto";
    void audio.play().catch(() => undefined);
    return () => {
      audio.pause();
      audio.src = "";
    };
  }, []);
  useEffect(() => {
    if (!commandOpen) return;
    const close = (event: KeyboardEvent) => event.key === "Escape" && setCommandOpen(false);
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [commandOpen]);
  return (
    <div className="admin-shell">
      <aside className="admin-sidebar">
        <SiteBrand href="/admin" suffix={<small>ADMIN</small>} />
        <nav aria-label="后台主导航">
          {adminNav.map(([href, icon, label]) => (
            <Link key={href} href={href} aria-current={pathname === href ? "page" : undefined} className={pathname === href ? "active" : ""}>
              <i>{icon}</i>{label}
            </Link>
          ))}
        </nav>
        <div className="admin-user">
          <AdminProfileAvatar admin={currentAdmin} />
          <div>
            <b>{currentAdmin.username}</b>
            <small>{currentAdmin.email || "唯一管理员"}</small>
          </div>
        </div>
      </aside>
      <main className="admin-main">
        <header>
          <div><span>CONTROL ROOM /</span><b>{current}</b></div>
          <div>
            <ThemeSwitch onClick={toggleTheme} compact />
            <button onClick={() => setCommandOpen(true)} aria-label="打开快捷导航" aria-expanded={commandOpen}>⌕</button>
            <button
              className="admin-profile-trigger"
              type="button"
              aria-label="打开账户菜单"
              aria-expanded={accountOpen}
              onClick={() => setAccountOpen((value) => !value)}
            >
              <AdminProfileAvatar admin={currentAdmin} />
              <span><b>{currentAdmin.username}</b><small>唯一管理员</small></span>
              <i aria-hidden="true">⌄</i>
            </button>
          </div>
        </header>
        <nav className="admin-mobile-nav" aria-label="后台移动导航">
          {adminNav.map(([href, icon, label]) => (
            <Link key={href} href={href} aria-current={pathname === href ? "page" : undefined} className={pathname === href ? "active" : ""}>
              <i>{icon}</i><span>{label}</span>
            </Link>
          ))}
        </nav>
        <div className="admin-content page-enter">{content}</div>
      </main>
      <AdminAccountCenter
        open={accountOpen}
        admin={currentAdmin}
        onClose={() => setAccountOpen(false)}
        onAdminChange={setCurrentAdmin}
        notify={notify}
      />
      {commandOpen && (
        <div className="admin-command" role="dialog" aria-modal="true" aria-label="快速导航" onClick={() => setCommandOpen(false)}>
          <div onClick={(event) => event.stopPropagation()}>
            <header><b>快速前往</b><button aria-label="关闭快捷导航" onClick={() => setCommandOpen(false)}>×</button></header>
            {adminNav.map(([href, icon, label]) => <Link key={href} href={href}><i>{icon}</i><span>{label}</span><small>→</small></Link>)}
          </div>
        </div>
      )}
    </div>
  );
}

function AdminTitle({ title, sub, action }: { title: string; sub: string; action?: React.ReactNode }) { return <div className="admin-title"><div><h1>{title}</h1><p>{sub}</p></div>{action}</div>; }

type DashboardOverview = {
  today_pv: number;
  today_uv: number;
  yesterday_pv: number;
  yesterday_uv: number;
  article_count: number;
  published_count: number;
  draft_count: number;
  total_visits: number;
  uptime_days: number;
};

type DashboardDailyStat = { date: string; pv: number; uv: number };

type DashboardModeratedComment = {
  id: number;
  content: string;
  date: string;
  nick: string;
  page_key: string;
};

type DashboardCommentQueue = {
  counts: { all: number; pending: number; approved: number };
  items: DashboardModeratedComment[];
};

type DashboardFriendApplication = {
  id: number;
  name: string;
  url: string;
  description: string;
  created_at: string;
};

type DashboardFriendQueue = {
  counts: { pending: number; approved: number; rejected: number };
  items: DashboardFriendApplication[];
};

const dashboardReviewDate = (value: string) => new Intl.DateTimeFormat("zh-CN", {
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  hour12: false,
}).format(new Date(value));

function Dashboard({ notify }: { notify: Notify }) {
  const [overview, setOverview] = useState<DashboardOverview | null>(null);
  const [trend, setTrend] = useState<DashboardDailyStat[]>([]);
  const [error, setError] = useState("");
  const [commentQueue, setCommentQueue] = useState<DashboardCommentQueue>({
    counts: { all: 0, pending: 0, approved: 0 },
    items: [],
  });
  const [friendQueue, setFriendQueue] = useState<DashboardFriendQueue>({
    counts: { pending: 0, approved: 0, rejected: 0 },
    items: [],
  });
  const [reviewLoading, setReviewLoading] = useState(true);
  const [reviewError, setReviewError] = useState("");
  const [reviewBusyId, setReviewBusyId] = useState<number | null>(null);

  const loadReviewQueue = useCallback(async (signal?: AbortSignal) => {
    setReviewLoading(true);
    setReviewError("");
    try {
      const [commentResponse, friendResponse] = await Promise.all([
        fetch("/api/v1/admin/comments?status=pending&page=1&per_page=2", {
          credentials: "include",
          headers: { accept: "application/json" },
          signal,
        }),
        fetch("/api/v1/admin/friends?status=pending&page=1&per_page=2", {
          credentials: "include",
          headers: { accept: "application/json" },
          signal,
        }),
      ]);
      if (!commentResponse.ok) throw new Error(await responseMessage(commentResponse, "待审评论队列加载失败"));
      if (!friendResponse.ok) throw new Error(await responseMessage(friendResponse, "友链审核队列加载失败"));
      const [comments, friends] = await Promise.all([
        commentResponse.json() as Promise<DashboardCommentQueue>,
        friendResponse.json() as Promise<DashboardFriendQueue>,
      ]);
      setCommentQueue(comments);
      setFriendQueue(friends);
    } catch (reason) {
      if (!(reason instanceof DOMException && reason.name === "AbortError")) {
        setReviewError(reason instanceof Error ? reason.message : "审核队列加载失败");
      }
    } finally {
      if (!signal?.aborted) setReviewLoading(false);
    }
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    Promise.all([
      fetch("/api/v1/admin/stats/overview", { credentials: "include", signal: controller.signal }),
      fetch("/api/v1/admin/stats/pv-uv?days=14", { credentials: "include", signal: controller.signal }),
    ]).then(async ([overviewResponse, trendResponse]) => {
      if (!overviewResponse.ok) throw new Error(await responseMessage(overviewResponse, "仪表盘概览加载失败"));
      if (!trendResponse.ok) throw new Error(await responseMessage(trendResponse, "访问趋势加载失败"));
      const [nextOverview, nextTrend] = await Promise.all([
        overviewResponse.json() as Promise<DashboardOverview>,
        trendResponse.json() as Promise<{ items: DashboardDailyStat[] }>,
      ]);
      setOverview(nextOverview);
      setTrend(nextTrend.items);
      setError("");
    }).catch((reason) => {
      if (!(reason instanceof DOMException && reason.name === "AbortError")) {
        setError(reason instanceof Error ? reason.message : "仪表盘加载失败");
      }
    });
    return () => controller.abort();
  }, []);
  useEffect(() => {
    const controller = new AbortController();
    const timer = window.setTimeout(() => void loadReviewQueue(controller.signal), 0);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [loadReviewQueue]);

  const approveDashboardComment = async (item: DashboardModeratedComment) => {
    setReviewBusyId(item.id);
    setReviewError("");
    try {
      const response = await fetch(`/api/v1/admin/comments/${item.id}`, {
        method: "PATCH",
        credentials: "include",
        headers: { "content-type": "application/json", accept: "application/json" },
        body: JSON.stringify({ status: "approved" }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "评论处理失败"));
      notify(`“${item.nick || "匿名访客"}”的评论已通过`, "success");
      await loadReviewQueue();
    } catch (reason) {
      setReviewError(reason instanceof Error ? reason.message : "评论处理失败");
    } finally {
      setReviewBusyId(null);
    }
  };

  const maxPv = Math.max(1, ...trend.map((item) => item.pv));
  const visitorDelta = overview ? overview.today_uv - overview.yesterday_uv : 0;
  const pendingReviewCount = commentQueue.counts.pending + friendQueue.counts.pending;
  const cards = overview ? [
    [overview.article_count.toLocaleString(), "文章总数", `${overview.published_count} 已发布 · ${overview.draft_count} 草稿`],
    [overview.total_visits.toLocaleString(), "累计访问", `今日 ${overview.today_pv} PV / ${overview.today_uv} UV`],
    [overview.today_uv.toLocaleString(), "今日访客", `${visitorDelta >= 0 ? "+" : ""}${visitorDelta} 较昨日`],
    [overview.uptime_days.toLocaleString(), "运行天数", "自站点创建日期起"],
  ] : [["—", "文章总数", "正在读取"], ["—", "累计访问", "正在读取"], ["—", "今日访客", "正在读取"], ["—", "运行天数", "正在读取"]];
  return <>
    <AdminTitle title="仪表盘" sub="WELCOME BACK, MASTER · LIVE SITE DATA" />
    {error && <div className="admin-dashboard-error" role="alert">{error}</div>}
    <div className="admin-stats">{cards.map(([n, l, d]) => <article key={l}><span>{l}</span><b>{n}</b><small>{d}</small></article>)}</div>
    <div className="dashboard-grid">
      <section className="admin-panel">
        <h2>访问趋势 <small>LAST 14 DAYS · PV</small></h2>
        <div className="chart">{trend.length ? trend.map((item) => <i key={item.date} title={`${item.date} · ${item.pv} PV / ${item.uv} UV`} style={{ height: `${Math.max(4, Math.round(item.pv / maxPv * 100))}%` }} />) : Array.from({ length: 14 }, (_, index) => <i className="loading" key={index} style={{ height: "4%" }} />)}</div>
      </section>
      <section className="admin-panel dashboard-review" aria-label="待审核内容">
        <h2><span>审核系统 <small>LIVE MODERATION</small></span><Link href="/admin/comments">全部审核 →</Link></h2>
        <div className="dashboard-review-counts">
          <Link href="/admin/comments?section=comments"><b>{commentQueue.counts.pending}</b><span>待审评论</span></Link>
          <Link href="/admin/comments?section=friends"><b>{friendQueue.counts.pending}</b><span>待审友链</span></Link>
        </div>
        {reviewLoading ? <div className="dashboard-review-state" role="status">正在读取审核队列…</div>
          : reviewError ? <div className="dashboard-review-state error" role="alert">{reviewError}<button type="button" onClick={() => void loadReviewQueue()}>重试</button></div>
            : pendingReviewCount === 0 ? <div className="dashboard-review-state"><b>审核队列已清空</b><span>当前没有待处理的评论或友链申请。</span></div>
              : <div className="dashboard-review-list">
                {commentQueue.items.map((item) => <article key={`comment-${item.id}`}>
                  <span aria-hidden="true">评</span>
                  <div><b>{item.nick || "匿名访客"}</b><p>{item.content}</p><small>{item.page_key || "未知页面"} · {dashboardReviewDate(item.date)}</small></div>
                  <button type="button" disabled={reviewBusyId === item.id} onClick={() => void approveDashboardComment(item)}>{reviewBusyId === item.id ? "处理中…" : "通过"}</button>
                </article>)}
                {friendQueue.items.map((item) => <article key={`friend-${item.id}`}>
                  <span className="friend" aria-hidden="true">链</span>
                  <div><b>{item.name}</b><p>{item.description || item.url}</p><small>{dashboardReviewDate(item.created_at)} 提交</small></div>
                  <Link href="/admin/comments?section=friends">审核</Link>
                </article>)}
              </div>}
      </section>
    </div>
    <section className="admin-panel quick"><h2>快速操作</h2><div><Link href="/admin/articles/new">✎<span>新建文章</span></Link><Link href="/admin/assets">▧<span>上传素材</span></Link><Link href="/admin/raiments">♙<span>管理灵衣</span></Link><Link href="/admin/settings">⚙<span>站点设置</span></Link></div></section>
  </>;
}

function ContentManager({ notify }: { notify: Notify }) {
  const [contentType, setContentType] = useState<"articles" | "moments">("articles");
  const [momentCreateSignal, setMomentCreateSignal] = useState(0);
  return <>
    <AdminTitle
      title="文章管理"
      sub={contentType === "articles" ? "BLOG ARTICLES" : "MOMENTS · 碎碎念"}
      action={contentType === "articles"
        ? <Link href="/admin/articles/new" className="admin-primary">＋ 新建文章</Link>
        : <button type="button" className="admin-primary" onClick={() => setMomentCreateSignal((value) => value + 1)}>＋ 发布说说</button>}
    />
    <div className="content-type-switch" role="tablist" aria-label="内容类型">
      <button type="button" role="tab" aria-selected={contentType === "articles"} className={contentType === "articles" ? "active" : ""} onClick={() => { setContentType("articles"); setMomentCreateSignal(0); }}>
        <i aria-hidden="true">▤</i><span><b>博客文章</b><small>长文、分类与标签</small></span>
      </button>
      <button type="button" role="tab" aria-selected={contentType === "moments"} className={contentType === "moments" ? "active" : ""} onClick={() => { setContentType("moments"); setMomentCreateSignal(0); }}>
        <i aria-hidden="true">✦</i><span><b>说说</b><small>碎碎念 · 联动前台时间轴</small></span>
      </button>
    </div>
    {contentType === "articles"
      ? <BlogArticleManager notify={notify} />
      : <MomentManager key={`moments-${momentCreateSignal}`} notify={notify} openInitially={momentCreateSignal > 0} />}
  </>;
}

function BlogArticleManager({ notify }: { notify: Notify }) {
  const [filter, setFilter] = useState("全部");
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const [rows, setRows] = useState<Post[]>([]);
  const [selected, setSelected] = useState<number[]>([]);
  const [deleteConfirmation, setDeleteConfirmation] = useState<{ ids: number[]; title?: string } | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [previewTarget, setPreviewTarget] = useState<Post | null>(null);
  const [previewArticle, setPreviewArticle] = useState<ArticleEditPayload | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState("");
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const loadSequenceRef = useRef(0);
  const perPage = 20;
  useArtalkCommentCounts(rows.map((post) => post.slug).join("|"));
  const load = useCallback(async () => {
    const sequence = ++loadSequenceRef.current;
    setLoading(true);
    const params = new URLSearchParams({ page: String(page), per_page: String(perPage) });
    if (filter === "已发布") params.set("status", "published");
    if (filter === "草稿") params.set("status", "draft");
    if (filter === "置顶") params.set("is_pinned", "true");
    if (query.trim()) params.set("search", query.trim());
    try {
      const response = await fetch(`/api/v1/admin/articles?${params}`, { credentials: "include" });
      if (!response.ok) throw new Error(await responseMessage(response, "文章列表加载失败"));
      const payload = await response.json() as ArticleListPayload;
      if (sequence !== loadSequenceRef.current) return;
      setRows(payload.items);
      setTotal(payload.total);
      const lastPage = Math.max(1, Math.ceil(payload.total / perPage));
      if (page > lastPage) setPage(lastPage);
      setSelected([]);
    } catch (error) {
      if (sequence !== loadSequenceRef.current) return;
      notify(error instanceof Error ? error.message : "文章列表加载失败", "danger");
    } finally {
      if (sequence === loadSequenceRef.current) setLoading(false);
    }
  }, [filter, notify, page, query]);
  useEffect(() => {
    const timer = window.setTimeout(() => void load(), 180);
    return () => window.clearTimeout(timer);
  }, [load]);
  const remove = (id: number, title: string) => setDeleteConfirmation({ ids: [id], title });
  const batch = async (action: "publish" | "unpublish" | "delete" | "pin") => {
    if (!selected.length) return;
    if (action === "delete") {
      setDeleteConfirmation({ ids: [...selected] });
      return;
    }
    const response = await fetch("/api/v1/admin/articles/batch", {
      method: "POST",
      credentials: "include",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ article_ids: selected, action }),
    });
    if (!response.ok) {
      notify(await responseMessage(response, "批量操作失败"), "danger");
      return;
    }
    const payload = await response.json() as { affected: number; failed_ids: number[] };
    notify(payload.failed_ids.length ? `${payload.affected} 篇已处理，${payload.failed_ids.length} 篇失败` : `已处理 ${payload.affected} 篇文章`, payload.failed_ids.length ? "danger" : "success");
    void load();
  };
  const confirmDelete = async () => {
    if (!deleteConfirmation || deleting) return;
    const { ids, title } = deleteConfirmation;
    setDeleting(true);
    try {
      if (ids.length === 1) {
        const response = await fetch(`/api/v1/admin/articles/${ids[0]}`, { method: "DELETE", credentials: "include" });
        if (!response.ok) throw new Error(await responseMessage(response, "删除失败"));
        notify(`已删除《${title || "所选文章"}》`, "success");
      } else {
        const response = await fetch("/api/v1/admin/articles/batch", {
          method: "POST",
          credentials: "include",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ article_ids: ids, action: "delete" }),
        });
        if (!response.ok) throw new Error(await responseMessage(response, "批量删除失败"));
        const payload = await response.json() as { affected: number; failed_ids: number[] };
        notify(payload.failed_ids.length ? `${payload.affected} 篇已删除，${payload.failed_ids.length} 篇失败` : `已删除 ${payload.affected} 篇文章`, payload.failed_ids.length ? "danger" : "success");
      }
      setDeleteConfirmation(null);
      void load();
    } catch (error) {
      notify(error instanceof Error ? error.message : "删除失败", "danger");
    } finally {
      setDeleting(false);
    }
  };
  useEffect(() => {
    if (!deleteConfirmation) return;
    const close = (event: KeyboardEvent) => event.key === "Escape" && !deleting && setDeleteConfirmation(null);
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [deleteConfirmation, deleting]);
  useEffect(() => {
    if (!previewTarget) return;
    const controller = new AbortController();
    void fetch(`/api/v1/admin/articles/${previewTarget.id}`, { credentials: "include", signal: controller.signal })
      .then(async (response) => {
        if (!response.ok) throw new Error(await responseMessage(response, "文章预览加载失败"));
        return response.json() as Promise<ArticleEditPayload>;
      })
      .then(setPreviewArticle)
      .catch((error) => {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          setPreviewError(error instanceof Error ? error.message : "文章预览加载失败");
        }
      })
      .finally(() => {
        if (!controller.signal.aborted) setPreviewLoading(false);
      });
    return () => controller.abort();
  }, [previewTarget]);
  const openPreview = (post: Post) => {
    setPreviewArticle(null);
    setPreviewError("");
    setPreviewLoading(true);
    setPreviewTarget(post);
  };
  const closePreview = useCallback(() => {
    setPreviewTarget(null);
    setPreviewArticle(null);
    setPreviewError("");
    setPreviewLoading(false);
  }, []);
  useEffect(() => {
    if (!previewTarget) return;
    const previousOverflow = document.body.style.overflow;
    const close = (event: KeyboardEvent) => event.key === "Escape" && closePreview();
    document.body.style.overflow = "hidden";
    window.addEventListener("keydown", close);
    return () => {
      window.removeEventListener("keydown", close);
      document.body.style.overflow = previousOverflow;
    };
  }, [closePreview, previewTarget]);
  const pageCount = Math.max(1, Math.ceil(total / perPage));
  return <>
    <div className="admin-toolbar"><div>{["全部", "已发布", "草稿", "置顶"].map((x) => <button key={x} className={filter === x ? "active" : ""} onClick={() => { setFilter(x); setPage(1); }}>{x}</button>)}</div><span className="admin-result-count">{total} 篇文章</span><input aria-label="搜索文章标题" value={query} onChange={(e) => { setQuery(e.target.value); setPage(1); }} placeholder="搜索标题…" /></div>
    {selected.length > 0 && <div className="admin-toolbar"><span>已选择 {selected.length} 篇</span><button onClick={() => void batch("publish")}>发布</button><button onClick={() => void batch("unpublish")}>撤回</button><button onClick={() => void batch("pin")}>置顶</button><button onClick={() => void batch("delete")}>删除</button></div>}
    <div className="admin-table">
      <div className="table-head"><span>选择</span><span>标题</span><span>分类</span><span>状态</span><span>数据</span><span>日期</span><span>操作</span></div>
      {loading && <div className="empty-panel">正在加载文章…</div>}
      {!loading && rows.map((p) => <div className="table-row" key={p.id}>
        <span><input type="checkbox" aria-label={`选择 ${p.title}`} checked={selected.includes(p.id)} onChange={(event) => setSelected((items) => event.target.checked ? [...items, p.id] : items.filter((id) => id !== p.id))} /></span>
        <b>{p.is_pinned && <em>置顶</em>}{p.title}</b>
        <span className="tag">{categoryName(p)}</span>
        <span className={p.status === "published" ? "published" : "draft"}>{p.status === "published" ? "● 已发布" : p.status === "hidden" ? "◌ 隐藏" : "◐ 草稿"}</span>
        <small>{p.view_count} 阅 · <span className="artalk-comment-count" data-page-key={articleCommentKey(p.slug)}>0</span> 评</small>
        <small>{articleDate(p).slice(5)}</small>
        <span className="row-actions"><Link href={`/admin/articles/${p.id}/edit`}>编辑</Link><button type="button" onClick={() => openPreview(p)}>预览</button><button onClick={() => remove(p.id, p.title)}>删除</button></span>
      </div>)}
      {!loading && !rows.length && <div className="empty-panel">没有符合当前筛选的文章。</div>}
    </div>
    {pageCount > 1 && <div className="pagination admin-article-pagination" aria-label="后台文章分页"><button disabled={page === 1 || loading} onClick={() => setPage((value) => Math.max(1, value - 1))} aria-label="上一页">◀</button><span>第 {page} / {pageCount} 页</span><button disabled={page === pageCount || loading} onClick={() => setPage((value) => Math.min(pageCount, value + 1))} aria-label="下一页">▶</button></div>}
    {deleteConfirmation && <div className="admin-account-dialog article-delete-overlay" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && !deleting && setDeleteConfirmation(null)}><section className="article-delete-dialog" role="dialog" aria-modal="true" aria-labelledby="article-delete-title"><header><div><span>ARTICLES / DELETE</span><h2 id="article-delete-title">确认删除文章</h2></div><button type="button" aria-label="关闭删除确认" disabled={deleting} onClick={() => setDeleteConfirmation(null)}>×</button></header><p>{deleteConfirmation.ids.length === 1 && deleteConfirmation.title ? `确定删除《${deleteConfirmation.title}》？` : `确定删除选中的 ${deleteConfirmation.ids.length} 篇文章？`}此操作不能撤销。</p><footer><button type="button" disabled={deleting} onClick={() => setDeleteConfirmation(null)}>取消</button><button className="danger" type="button" disabled={deleting} onClick={() => void confirmDelete()}>{deleting ? "正在删除…" : "确认删除"}</button></footer></section></div>}
    {previewTarget && typeof document !== "undefined" && createPortal(<div className="admin-article-preview-overlay" role="presentation" onClick={(event) => event.target === event.currentTarget && closePreview()}>
      <section role="dialog" aria-modal="true" aria-labelledby="admin-article-preview-title">
        <header><div><span>ARTICLE / LAYOUT PREVIEW</span><h2 id="admin-article-preview-title">排版预览</h2></div><button type="button" aria-label="关闭文章预览" onClick={closePreview}>×</button></header>
        <div className="admin-article-preview-scroll">
          {previewLoading && <div className="empty-panel">正在生成排版预览…</div>}
          {!previewLoading && previewError && <div className="empty-panel">{previewError}</div>}
          {!previewLoading && previewArticle && <article className="article-card admin-article-preview-card">
            <div className="breadcrumbs">{categoryName(previewTarget)}</div>
            <h1>{previewArticle.title || "未命名文章"}</h1>
            <div className="article-meta">{articleDate(previewTarget)} · {articleWords(previewTarget)} · {articleTime(previewTarget)}</div>
            {previewArticle.summary && <p>{previewArticle.summary}</p>}
            {previewTarget.cover_url && <Image className="article-image" src={previewTarget.cover_url} width={1200} height={675} sizes="(max-width: 820px) 100vw, 760px" alt={`${previewArticle.title} 封面`} unoptimized />}
            <MarkdownBody source={previewArticle.content_md || "这篇文章还没有正文。"} />
          </article>}
        </div>
      </section>
    </div>, document.body)}
  </>;
}

type MomentDraft = {
  id: number | null;
  content: string;
  asset_ids: number[];
  tag_ids: number[];
  created_at: string;
};

function localDateTimeValue(value: string | Date = new Date()) {
  const date = value instanceof Date ? value : new Date(value);
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60_000);
  return local.toISOString().slice(0, 16);
}

function newMomentDraft(): MomentDraft {
  return { id: null, content: "", asset_ids: [], tag_ids: [], created_at: localDateTimeValue() };
}

function MomentManager({ notify, openInitially }: { notify: Notify; openInitially: boolean }) {
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const [rows, setRows] = useState<Moment[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [draft, setDraft] = useState<MomentDraft | null>(() => openInitially ? newMomentDraft() : null);
  const [deleteTarget, setDeleteTarget] = useState<Moment | null>(null);
  const [assets, setAssets] = useState<AdminAsset[]>([]);
  const [assetsLoading, setAssetsLoading] = useState(true);
  const [tags, setTags] = useState<ArticleTag[]>([]);
  const [tagsLoading, setTagsLoading] = useState(true);
  const [newTag, setNewTag] = useState("");
  const [creatingTag, setCreatingTag] = useState(false);
  const loadSequenceRef = useRef(0);
  const perPage = 20;

  const load = useCallback(async () => {
    const sequence = ++loadSequenceRef.current;
    setLoading(true);
    const params = new URLSearchParams({ page: String(page), per_page: String(perPage) });
    if (query.trim()) params.set("search", query.trim());
    try {
      const response = await fetch(`/api/v1/admin/moments?${params}`, { credentials: "include" });
      if (!response.ok) throw new Error(await responseMessage(response, "说说列表加载失败"));
      const payload = await response.json() as MomentListPayload;
      if (sequence !== loadSequenceRef.current) return;
      setRows(payload.items);
      setTotal(payload.total);
      const lastPage = Math.max(1, Math.ceil(payload.total / perPage));
      if (page > lastPage) setPage(lastPage);
    } catch (error) {
      if (sequence === loadSequenceRef.current) notify(error instanceof Error ? error.message : "说说列表加载失败", "danger");
    } finally {
      if (sequence === loadSequenceRef.current) setLoading(false);
    }
  }, [notify, page, query]);

  useEffect(() => {
    const timer = window.setTimeout(() => void load(), 180);
    return () => window.clearTimeout(timer);
  }, [load]);

  useEffect(() => {
    const controller = new AbortController();
    void fetch("/api/v1/admin/assets?media_type=image&per_page=100", { credentials: "include", signal: controller.signal })
      .then(async (response) => {
        if (!response.ok) throw new Error(await responseMessage(response, "图片素材加载失败"));
        return response.json() as Promise<{ items: AdminAsset[] }>;
      })
      .then((payload) => setAssets(payload.items))
      .catch((error) => {
        if (!(error instanceof DOMException && error.name === "AbortError")) notify(error instanceof Error ? error.message : "图片素材加载失败", "danger");
      })
      .finally(() => {
        if (!controller.signal.aborted) setAssetsLoading(false);
      });
    return () => controller.abort();
  }, [notify]);

  useEffect(() => {
    const controller = new AbortController();
    void fetch("/api/v1/admin/tags", { credentials: "include", signal: controller.signal })
      .then(async (response) => {
        if (!response.ok) throw new Error(await responseMessage(response, "标签加载失败"));
        return response.json() as Promise<{ items: ArticleTag[] }>;
      })
      .then((payload) => setTags(payload.items))
      .catch((error) => {
        if (!(error instanceof DOMException && error.name === "AbortError")) notify(error instanceof Error ? error.message : "标签加载失败", "danger");
      })
      .finally(() => {
        if (!controller.signal.aborted) setTagsLoading(false);
      });
    return () => controller.abort();
  }, [notify]);

  useEffect(() => {
    if (!draft && !deleteTarget) return;
    const previousOverflow = document.body.style.overflow;
    const close = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || saving || deleting) return;
      setDraft(null);
      setDeleteTarget(null);
    };
    document.body.style.overflow = "hidden";
    window.addEventListener("keydown", close);
    return () => {
      document.body.style.overflow = previousOverflow;
      window.removeEventListener("keydown", close);
    };
  }, [deleteTarget, deleting, draft, saving]);

  const edit = (moment: Moment) => setDraft({
    id: moment.id,
    content: moment.content,
    asset_ids: moment.images.map((image) => image.asset_id),
    tag_ids: moment.tags.map((tag) => tag.id),
    created_at: localDateTimeValue(moment.created_at),
  });

  const toggleTag = (tagId: number) => {
    if (!draft) return;
    setDraft({
      ...draft,
      tag_ids: draft.tag_ids.includes(tagId)
        ? draft.tag_ids.filter((id) => id !== tagId)
        : [...draft.tag_ids, tagId],
    });
  };

  const createTag = async () => {
    const name = newTag.trim();
    if (!draft || !name || creatingTag) return;
    setCreatingTag(true);
    try {
      const response = await fetch("/api/v1/admin/tags", {
        method: "POST",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ name }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "标签创建失败"));
      const item = await response.json() as ArticleTag;
      setTags((items) => [...items, item].sort((left, right) => left.name.localeCompare(right.name, "zh-CN")));
      setDraft((current) => current
        ? { ...current, tag_ids: current.tag_ids.includes(item.id) ? current.tag_ids : [...current.tag_ids, item.id] }
        : current);
      setNewTag("");
      notify(`已新增共享标签「${item.name}」`, "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "标签创建失败", "danger");
    } finally {
      setCreatingTag(false);
    }
  };

  const toggleAsset = (assetId: number) => {
    if (!draft) return;
    if (draft.asset_ids.includes(assetId)) {
      setDraft({ ...draft, asset_ids: draft.asset_ids.filter((id) => id !== assetId) });
      return;
    }
    if (draft.asset_ids.length >= 9) {
      notify("每条说说最多选择 9 张图片", "danger");
      return;
    }
    setDraft({ ...draft, asset_ids: [...draft.asset_ids, assetId] });
  };

  const save = async () => {
    if (!draft || saving) return;
    if (!draft.content.trim() && !draft.asset_ids.length) {
      notify("说说内容和图片不能同时为空", "danger");
      return;
    }
    setSaving(true);
    try {
      const response = await fetch(draft.id ? `/api/v1/admin/moments/${draft.id}` : "/api/v1/admin/moments", {
        method: draft.id ? "PUT" : "POST",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          content: draft.content,
          asset_ids: draft.asset_ids,
          tag_ids: draft.tag_ids,
          created_at: new Date(draft.created_at).toISOString(),
        }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, draft.id ? "说说更新失败" : "说说发布失败"));
      notify(draft.id ? "说说已更新，前台时间轴已同步" : "说说已发布到前台时间轴", "success");
      setDraft(null);
      setPage(1);
      void load();
    } catch (error) {
      notify(error instanceof Error ? error.message : "说说保存失败", "danger");
    } finally {
      setSaving(false);
    }
  };

  const confirmDelete = async () => {
    if (!deleteTarget || deleting) return;
    setDeleting(true);
    try {
      const response = await fetch(`/api/v1/admin/moments/${deleteTarget.id}`, { method: "DELETE", credentials: "include" });
      if (!response.ok) throw new Error(await responseMessage(response, "说说删除失败"));
      notify("说说已删除，前台时间轴已同步", "success");
      setDeleteTarget(null);
      void load();
    } catch (error) {
      notify(error instanceof Error ? error.message : "说说删除失败", "danger");
    } finally {
      setDeleting(false);
    }
  };

  const pageCount = Math.max(1, Math.ceil(total / perPage));
  return <>
    <div className="admin-toolbar moment-admin-toolbar">
      <div><button type="button" className="active">全部说说</button><span className="admin-result-count">{total} 条动态 · 发布后自动进入前台时间轴</span></div>
      <input aria-label="搜索说说内容" value={query} onChange={(event) => { setQuery(event.target.value); setPage(1); }} placeholder="搜索说说内容…" />
    </div>
    <div className="admin-table moment-admin-table">
      <div className="table-head moment-table-grid"><span>内容</span><span>图片</span><span>点赞</span><span>发布时间</span><span>操作</span></div>
      {loading && <div className="empty-panel">正在加载说说…</div>}
      {!loading && rows.map((moment) => <div className="table-row moment-table-grid" key={moment.id}>
        <b title={moment.content}>{moment.content || "（仅图片）"}</b>
        <span className="moment-admin-thumbs">{moment.images.slice(0, 3).map((image) => <Image key={image.asset_id} src={image.url} width={42} height={42} unoptimized alt="" />)}{moment.images.length > 3 && <i>+{moment.images.length - 3}</i>}{!moment.images.length && <small>无图片</small>}</span>
        <small>{moment.like_count} 赞</small>
        <small>{new Date(moment.created_at).toLocaleString("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" })}</small>
        <span className="row-actions"><button type="button" onClick={() => edit(moment)}>编辑</button><button type="button" onClick={() => setDeleteTarget(moment)}>删除</button></span>
      </div>)}
      {!loading && !rows.length && <div className="empty-panel">还没有符合条件的说说。</div>}
    </div>
    {pageCount > 1 && <div className="pagination admin-article-pagination" aria-label="后台说说分页"><button disabled={page === 1 || loading} onClick={() => setPage((value) => Math.max(1, value - 1))} aria-label="上一页">◀</button><span>第 {page} / {pageCount} 页</span><button disabled={page === pageCount || loading} onClick={() => setPage((value) => Math.min(pageCount, value + 1))} aria-label="下一页">▶</button></div>}
    {draft && typeof document !== "undefined" && createPortal(<div className="moment-editor-overlay" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && !saving && setDraft(null)}>
      <section role="dialog" aria-modal="true" aria-labelledby="moment-editor-title">
        <header><div><span>MOMENTS / TIMELINE</span><h2 id="moment-editor-title">{draft.id ? "编辑说说" : "发布说说"}</h2></div><button type="button" aria-label="关闭说说编辑器" disabled={saving} onClick={() => setDraft(null)}>×</button></header>
        <div className="moment-editor-body">
          <label className="moment-editor-content"><span>碎碎念 <small>{draft.content.length}/5000</small></span><textarea autoFocus maxLength={5000} value={draft.content} onChange={(event) => setDraft({ ...draft, content: event.target.value })} placeholder="记录此刻的想法，也可以只发图片…" /></label>
          <div className="moment-editor-side">
            <label className="moment-editor-time"><span>时间轴时间</span><input type="datetime-local" value={draft.created_at} onChange={(event) => setDraft({ ...draft, created_at: event.target.value })} /></label>
            <section className="moment-editor-tags">
              <div><span>标签</span></div>
              {tagsLoading ? <div className="moment-editor-tags-empty">正在加载标签…</div>
                : <div className="editor-tag-picker">{tags.length ? tags.map((item) => <button type="button" key={item.id} className={draft.tag_ids.includes(item.id) ? "selected" : ""} onClick={() => toggleTag(item.id)}>{item.name}<i>{draft.tag_ids.includes(item.id) ? "✓" : "+"}</i></button>) : <em>暂无可用标签</em>}</div>}
              <div className="taxonomy-create-row"><input value={newTag} onChange={(event) => setNewTag(event.target.value)} onKeyDown={(event) => event.key === "Enter" && void createTag()} placeholder="新增标签" aria-label="新增标签名称" /><button type="button" onClick={() => void createTag()} disabled={!newTag.trim() || creatingTag}>＋ 添加</button></div>
            </section>
          </div>
          <section className="moment-editor-assets">
            <div><span>配图素材</span><small>已选 {draft.asset_ids.length} / 9 · 点击图片选择并排序</small></div>
            {assetsLoading ? <div className="empty-panel">正在加载图片素材…</div>
              : assets.length ? <div className="moment-editor-asset-grid">{assets.map((asset) => {
                const selectedIndex = draft.asset_ids.indexOf(asset.id);
                return <button type="button" key={asset.id} className={selectedIndex >= 0 ? "selected" : ""} onClick={() => toggleAsset(asset.id)}><Image src={asset.file.url} width={180} height={120} unoptimized alt={asset.name} /><span>{selectedIndex >= 0 ? selectedIndex + 1 : "+"}</span><b>{asset.name}</b></button>;
              })}</div>
                : <div className="moment-editor-no-assets"><p>素材库中还没有图片。</p><Link href="/admin/assets">去素材库上传 →</Link></div>}
          </section>
        </div>
        <footer><small>保存后会立即同步到博客前台的“时间轴”。</small><div><button type="button" disabled={saving} onClick={() => setDraft(null)}>取消</button><button type="button" className="admin-primary" disabled={saving || (!draft.content.trim() && !draft.asset_ids.length)} onClick={() => void save()}>{saving ? "正在保存…" : draft.id ? "保存并同步" : "发布到时间轴"}</button></div></footer>
      </section>
    </div>, document.body)}
    {deleteTarget && typeof document !== "undefined" && createPortal(<div className="admin-account-dialog article-delete-overlay" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && !deleting && setDeleteTarget(null)}><section className="article-delete-dialog" role="dialog" aria-modal="true" aria-labelledby="moment-delete-title"><header><div><span>MOMENTS / DELETE</span><h2 id="moment-delete-title">确认删除说说</h2></div><button type="button" aria-label="关闭删除确认" disabled={deleting} onClick={() => setDeleteTarget(null)}>×</button></header><p>这条说说会同时从前台时间轴移除，已有点赞记录也会删除。此操作不能撤销。</p><footer><button type="button" disabled={deleting} onClick={() => setDeleteTarget(null)}>取消</button><button className="danger" type="button" disabled={deleting} onClick={() => void confirmDelete()}>{deleting ? "正在删除…" : "确认删除"}</button></footer></section></div>, document.body)}
  </>;
}

type ArticleEditPayload = {
  title: string;
  summary: string;
  content_md: string;
  category_id: number | null;
  tag_ids: number[];
  cover_asset_id: number | null;
  content_asset_ids: number[];
  is_pinned: boolean;
  allow_comment: boolean;
  kanban_ref: boolean;
  status: Post["status"];
  slug: string;
  updated_at: string;
};

type EditorLlmConnection = {
  id: number;
  display_name: string;
  model: string;
  api_key_configured: boolean;
  enabled: boolean;
};
type EditorModelOption = { id: string; name: string };
type PolishTargetKey = "summary" | "content_md";
type EditorMergeSide = "before" | "after";
type EditorMergeRow = {
  id: number;
  beforeLine: number | null;
  afterLine: number | null;
  beforeText: string | null;
  afterText: string | null;
  changed: boolean;
};
type PolishCandidate = {
  target: PolishTargetKey;
  before: string;
  after: string;
  rows: EditorMergeRow[];
};

const DEFAULT_POLISH_PROMPTS: Record<PolishTargetKey, string> = {
  summary: "将原摘要润色为简洁、准确、有吸引力的一段摘要。只返回摘要正文，不要标题、解释、换行或 Markdown；包含标点和空格在内不得超过 120 个字符，建议控制在 80–110 个字符。保持原意和事实，不要扩写成正文。",
  content_md: "保持原意和事实，改善 Markdown 结构与表达，并补足必要的过渡和说明。不要虚构信息，只返回可直接替换的正文。",
};

function diffPartLines(value: string) {
  const lines = value.split("\n");
  if (lines[lines.length - 1] === "") lines.pop();
  return lines;
}

async function buildEditorMergeRows(before: string, after: string): Promise<EditorMergeRow[]> {
  const { diffLines } = await import("diff");
  const parts = diffLines(before, after);
  const rows: EditorMergeRow[] = [];
  let beforeLine = 1;
  let afterLine = 1;
  let partIndex = 0;
  while (partIndex < parts.length) {
    const part = parts[partIndex];
    if (!part.added && !part.removed) {
      diffPartLines(part.value).forEach((text) => {
        rows.push({ id: rows.length, beforeLine: beforeLine++, afterLine: afterLine++, beforeText: text, afterText: text, changed: false });
      });
      partIndex += 1;
      continue;
    }
    const beforeLines: string[] = [];
    const afterLines: string[] = [];
    while (partIndex < parts.length && (parts[partIndex].added || parts[partIndex].removed)) {
      const changedPart = parts[partIndex];
      const lines = diffPartLines(changedPart.value);
      if (changedPart.removed) beforeLines.push(...lines);
      if (changedPart.added) afterLines.push(...lines);
      partIndex += 1;
    }
    const rowCount = Math.max(beforeLines.length, afterLines.length);
    for (let index = 0; index < rowCount; index += 1) {
      const beforeText = beforeLines[index] ?? null;
      const afterText = afterLines[index] ?? null;
      rows.push({
        id: rows.length,
        beforeLine: beforeText === null ? null : beforeLine++,
        afterLine: afterText === null ? null : afterLine++,
        beforeText,
        afterText,
        changed: true,
      });
    }
  }
  return rows;
}

function defaultMergeChoices(rows: EditorMergeRow[], side: EditorMergeSide = "after") {
  return Object.fromEntries(rows.filter((row) => row.changed).map((row) => [row.id, side])) as Record<number, EditorMergeSide>;
}

function mergeEditorRows(candidate: PolishCandidate, choices: Record<number, EditorMergeSide>) {
  const changedRows = candidate.rows.filter((row) => row.changed);
  if (changedRows.every((row) => (choices[row.id] ?? "after") === "before")) return candidate.before;
  if (changedRows.every((row) => (choices[row.id] ?? "after") === "after")) return candidate.after;
  const result = candidate.rows.flatMap((row) => {
    const side = row.changed ? (choices[row.id] ?? "after") : "after";
    const text = side === "before" ? row.beforeText : row.afterText;
    return text === null ? [] : [text];
  }).join("\n");
  return candidate.before.endsWith("\n") && candidate.after.endsWith("\n") ? `${result}\n` : result;
}

function sortEditorModels(items: EditorModelOption[]) {
  return [...items].sort((left, right) => left.id.localeCompare(right.id, "en", { numeric: true, sensitivity: "base" }));
}

const emptyArticle: ArticleEditPayload = {
  title: "",
  summary: "",
  content_md: "",
  category_id: null,
  tag_ids: [],
  cover_asset_id: null,
  content_asset_ids: [],
  is_pinned: false,
  allow_comment: true,
  kanban_ref: true,
  status: "draft",
  slug: "待保存",
  updated_at: "",
};

const editorDraftKey = (id: number | null) => id ? `helt-article-editor-${id}` : "helt-article-editor-new";

function readEditorDraft(key: string) {
  try {
    const raw = window.localStorage.getItem(key);
    return raw ? JSON.parse(raw) as Partial<ArticleEditPayload> : null;
  } catch {
    return null;
  }
}

function ArticleEditor({ pathname, theme, notify }: { pathname: string; theme: Theme; notify: Notify }) {
  const [initialArticleId] = useState<number | null>(() => {
    const match = pathname.match(/\/admin\/articles\/(\d+)\/edit$/);
    return match ? Number(match[1]) : null;
  });
  const [articleId, setArticleId] = useState<number | null>(initialArticleId);
  const [data, setData] = useState<ArticleEditPayload | null>(null);
  const [categories, setCategories] = useState<ArticleCategory[]>([]);
  const [tags, setTags] = useState<ArticleTag[]>([]);
  const [assets, setAssets] = useState<AdminAsset[]>([]);
  const [preview, setPreview] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [lastSavedAt, setLastSavedAt] = useState("");
  const [autoSavedAt, setAutoSavedAt] = useState("");
  const [coverPickerOpen, setCoverPickerOpen] = useState(false);
  const [newCategory, setNewCategory] = useState("");
  const [newTag, setNewTag] = useState("");
  const [creatingTaxonomy, setCreatingTaxonomy] = useState<"category" | "tag" | "">("");
  const [llmConnections, setLlmConnections] = useState<EditorLlmConnection[]>([]);
  const [llmLoading, setLlmLoading] = useState(true);
  const [llmError, setLlmError] = useState("");
  const [aiConnectionId, setAiConnectionId] = useState<number | null>(null);
  const [aiModels, setAiModels] = useState<EditorModelOption[]>([]);
  const [aiModel, setAiModel] = useState("");
  const [aiPrompts, setAiPrompts] = useState<Record<PolishTargetKey, string>>(DEFAULT_POLISH_PROMPTS);
  const [aiTarget, setAiTarget] = useState<PolishTargetKey>("content_md");
  const [modelsLoading, setModelsLoading] = useState(false);
  const [polishing, setPolishing] = useState(false);
  const [polishCandidate, setPolishCandidate] = useState<PolishCandidate | null>(null);
  const [polishChoices, setPolishChoices] = useState<Record<number, EditorMergeSide>>({});
  const titleRef = useRef<HTMLInputElement>(null);
  const saveInFlightRef = useRef(false);
  const aiPrompt = aiPrompts[aiTarget];
  useEffect(() => {
    const controller = new AbortController();
    const load = async () => {
      try {
        const [categoryResponse, tagResponse, assetResponse] = await Promise.all([
          fetch("/api/v1/admin/categories", { credentials: "include", signal: controller.signal }),
          fetch("/api/v1/admin/tags", { credentials: "include", signal: controller.signal }),
          fetch("/api/v1/admin/assets?media_type=image&per_page=100", { credentials: "include", signal: controller.signal }),
        ]);
        if (!categoryResponse.ok) throw new Error(await responseMessage(categoryResponse, "分类加载失败"));
        if (!tagResponse.ok) throw new Error(await responseMessage(tagResponse, "标签加载失败"));
        let article = emptyArticle;
        if (initialArticleId) {
          const articleResponse = await fetch(`/api/v1/admin/articles/${initialArticleId}`, { credentials: "include", signal: controller.signal });
          if (!articleResponse.ok) throw new Error(await responseMessage(articleResponse, "文章加载失败"));
          article = await articleResponse.json() as ArticleEditPayload;
        }
        const categoryPayload = await categoryResponse.json() as { items: ArticleCategory[] };
        const tagPayload = await tagResponse.json() as { items: ArticleTag[] };
        const savedDraft = readEditorDraft(editorDraftKey(initialArticleId));
        setData({ ...article, ...(savedDraft || {}) });
        setCategories(categoryPayload.items);
        setTags(tagPayload.items);
        if (assetResponse.ok) {
          const assetPayload = await assetResponse.json() as { items: AdminAsset[] };
          setAssets(assetPayload.items);
        }
        setDirty(Boolean(savedDraft));
        setAutoSavedAt(savedDraft ? "已恢复上次自动保存" : "");
      } catch (error) {
        if (!(error instanceof DOMException && error.name === "AbortError")) notify(error instanceof Error ? error.message : "文章加载失败", "danger");
      } finally {
        setLoading(false);
      }
    };
    void load();
    return () => controller.abort();
  }, [initialArticleId, notify]);

  useEffect(() => {
    const controller = new AbortController();
    void fetch("/api/v1/admin/llm", { credentials: "include", signal: controller.signal })
      .then(async (response) => {
        if (!response.ok) throw new Error(await responseMessage(response, "无法读取 LLM Key"));
        const payload = await response.json() as { connections?: EditorLlmConnection[] };
        setLlmConnections((payload.connections ?? []).filter((connection) => connection.enabled && connection.api_key_configured));
        setLlmError("");
      })
      .catch((error) => {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          setLlmError(error instanceof Error ? error.message : "无法读取 LLM Key");
        }
      })
      .finally(() => setLlmLoading(false));
    return () => controller.abort();
  }, []);

  const draftKey = editorDraftKey(articleId ?? initialArticleId);
  useEffect(() => {
    if (!data || loading || !dirty) return;
    const timer = window.setTimeout(() => {
      try {
        window.localStorage.setItem(draftKey, JSON.stringify(data));
        setAutoSavedAt(new Date().toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" }));
      } catch {
        // 浏览器禁用本地存储时仍可继续编辑和手动保存。
      }
    }, 900);
    return () => {
      window.clearTimeout(timer);
    };
  }, [data, dirty, draftKey, loading]);

  const update = <K extends keyof ArticleEditPayload>(key: K, value: ArticleEditPayload[K]) => {
    setDirty(true);
    setData((current) => current ? { ...current, [key]: value } : current);
  };
  const loadAiModels = async (connectionId: number) => {
    setModelsLoading(true);
    setLlmError("");
    try {
      const response = await fetch("/api/v1/admin/llm/models", {
        method: "POST",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ connection_id: connectionId }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "无法获取模型列表"));
      const payload = await response.json() as { items: EditorModelOption[] };
      const sortedModels = sortEditorModels(payload.items);
      setAiModels(sortedModels);
      if (!sortedModels.some((item) => item.id === aiModel)) setAiModel("");
    } catch (error) {
      const message = error instanceof Error ? error.message : "无法获取模型列表";
      setAiModels([]);
      setAiModel("");
      setLlmError(message);
    } finally {
      setModelsLoading(false);
    }
  };
  const selectAiConnection = (value: string) => {
    const connectionId = value ? Number(value) : null;
    setAiConnectionId(connectionId);
    setAiModels([]);
    setAiModel("");
    setLlmError("");
    if (connectionId) void loadAiModels(connectionId);
  };
  const polishArticle = async () => {
    const source = aiTarget === "summary" ? data.summary : data.content_md;
    const targetLabel = aiTarget === "summary" ? "摘要" : "正文";
    if (!source.trim()) {
      notify(`先写一点${targetLabel}草稿，再交给 AI 润色`, "danger");
      return;
    }
    if (llmLoading) {
      notify("正在读取可用 Key，请稍候", "danger");
      return;
    }
    if (!aiConnectionId) {
      notify("请先选择用于润色的 Key", "danger");
      return;
    }
    if (modelsLoading) {
      notify("正在读取模型列表，请稍候", "danger");
      return;
    }
    if (!aiModel) {
      notify("请先选择用于润色的模型", "danger");
      return;
    }
    if (!aiPrompt.trim()) {
      notify("请填写润色提示词", "danger");
      return;
    }
    setPolishing(true);
    setLlmError("");
    try {
      const apiTarget = aiTarget === "summary" ? "summary" : "content";
      const response = await fetch("/api/v1/admin/llm/polish", {
        method: "POST",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          connection_id: aiConnectionId,
          model: aiModel,
          prompt: aiPrompt,
          target: apiTarget,
          title: data.title,
          summary: data.summary,
          content_md: data.content_md,
        }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "AI 润色失败"));
      const payload = await response.json() as { target: "summary" | "content"; text: string };
      if (payload.target !== apiTarget || !payload.text?.trim()) throw new Error(`模型没有返回可用${targetLabel}`);
      if (payload.text === source) throw new Error(`模型返回的${targetLabel}与原文相同`);
      const rows = await buildEditorMergeRows(source, payload.text);
      setPolishCandidate({ target: aiTarget, before: source, after: payload.text, rows });
      setPolishChoices(defaultMergeChoices(rows));
      notify(`${targetLabel}候选稿已生成，请对比后决定是否替换`, "success");
    } catch (error) {
      const message = error instanceof Error ? error.message : "AI 润色失败";
      setLlmError(message);
      notify(message, "danger");
    } finally {
      setPolishing(false);
    }
  };
  const applyPolishCandidate = () => {
    if (!polishCandidate) return;
    const targetLabel = polishCandidate.target === "summary" ? "摘要" : "正文";
    if (data[polishCandidate.target] !== polishCandidate.before) {
      setPolishCandidate(null);
      notify(`${targetLabel}已在对比期间发生变化，请重新生成候选稿`, "danger");
      return;
    }
    const mergedText = mergeEditorRows(polishCandidate, polishChoices);
    if (!mergedText.trim()) {
      notify(`合并后的${targetLabel}为空，请至少保留一行内容`, "danger");
      return;
    }
    update(polishCandidate.target, mergedText);
    if (polishCandidate.target === "content_md") setPreview(false);
    setPolishCandidate(null);
    setPolishChoices({});
    notify(`已应用${targetLabel}合并结果，保存文章后才会生效`, "success");
  };
  const closePolishComparison = () => {
    setPolishCandidate(null);
    setPolishChoices({});
  };
  const choosePolishLine = (rowId: number, side: EditorMergeSide) => {
    setPolishChoices((current) => ({ ...current, [rowId]: side }));
  };
  const chooseAllPolishLines = (side: EditorMergeSide) => {
    if (polishCandidate) setPolishChoices(defaultMergeChoices(polishCandidate.rows, side));
  };
  const save = useCallback(async (status: Post["status"]) => {
    if (!data || saveInFlightRef.current) return;
    if (!data.title.trim()) {
      notify("先给文章写个标题", "danger");
      titleRef.current?.focus();
      return;
    }
    if (status === "published" && !data.content_md.trim()) {
      notify("发布前先写一点正文", "danger");
      return;
    }
    if (status === "published" && !data.category_id) {
      notify("发布前请选择一个分类", "danger");
      return;
    }
    saveInFlightRef.current = true;
    setSaving(true);
    try {
      let id = articleId;
      let expectedUpdatedAt = data.updated_at;
      if (!id) {
        const created = await fetch("/api/v1/admin/articles", {
          method: "POST",
          credentials: "include",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ title: data.title }),
        });
        if (!created.ok) throw new Error(await responseMessage(created, "无法创建文章"));
        const createdArticle = await created.json() as { id: number; updated_at: string };
        id = createdArticle.id;
        expectedUpdatedAt = createdArticle.updated_at;
        setArticleId(id);
        window.history.replaceState({}, "", `/admin/articles/${id}/edit`);
      }
      const response = await fetch(`/api/v1/admin/articles/${id}`, {
        method: "PUT",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          ...data,
          status,
          expected_updated_at: expectedUpdatedAt || undefined,
        }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "文章保存失败"));
      const result = await response.json() as { status: Post["status"]; updated_at: string };
      try {
        window.localStorage.removeItem(editorDraftKey(articleId));
        window.localStorage.removeItem(editorDraftKey(id));
      } catch {
        // 本地存储不可用时不影响服务端保存结果。
      }
      setData((current) => current ? { ...current, status: result.status, updated_at: result.updated_at } : current);
      setDirty(false);
      setAutoSavedAt("");
      setLastSavedAt(new Date().toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" }));
      notify(status === "published" ? "文章已发布" : "草稿已保存", "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "文章保存失败", "danger");
    } finally {
      saveInFlightRef.current = false;
      setSaving(false);
    }
  }, [articleId, data, notify]);

  const createCategory = async () => {
    const name = newCategory.trim();
    if (!name || creatingTaxonomy) return;
    setCreatingTaxonomy("category");
    try {
      const slug = /^[a-z0-9](?:[a-z0-9-_]*[a-z0-9])?$/i.test(name) ? name.toLowerCase() : `category-${Date.now()}`;
      const response = await fetch("/api/v1/admin/categories", { method: "POST", credentials: "include", headers: { "content-type": "application/json" }, body: JSON.stringify({ name, slug }) });
      if (!response.ok) throw new Error(await responseMessage(response, "分类创建失败"));
      const item = await response.json() as ArticleCategory;
      setCategories((items) => [...items, item].sort((left, right) => left.name.localeCompare(right.name, "zh-CN")));
      update("category_id", item.id);
      setNewCategory("");
      notify(`已新增分类「${item.name}」`, "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "分类创建失败", "danger");
    } finally {
      setCreatingTaxonomy("");
    }
  };

  const createTag = async () => {
    const name = newTag.trim();
    if (!name || creatingTaxonomy) return;
    setCreatingTaxonomy("tag");
    try {
      const response = await fetch("/api/v1/admin/tags", { method: "POST", credentials: "include", headers: { "content-type": "application/json" }, body: JSON.stringify({ name }) });
      if (!response.ok) throw new Error(await responseMessage(response, "标签创建失败"));
      const item = await response.json() as ArticleTag;
      setTags((items) => [...items, item].sort((left, right) => left.name.localeCompare(right.name, "zh-CN")));
      setDirty(true);
      setData((current) => current ? { ...current, tag_ids: current.tag_ids.includes(item.id) ? current.tag_ids : [...current.tag_ids, item.id] } : current);
      setNewTag("");
      notify(`已新增标签「${item.name}」`, "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "标签创建失败", "danger");
    } finally {
      setCreatingTaxonomy("");
    }
  };

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = event.ctrlKey || event.metaKey;
      if (modifier && event.key.toLowerCase() === "s") {
        event.preventDefault();
        if (saving) return;
        void save(data?.status === "published" ? "published" : "draft");
      }
      if (modifier && event.shiftKey && event.key.toLowerCase() === "p") {
        event.preventDefault();
        setPreview((value) => !value);
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [data?.status, save, saving]);
  useEffect(() => {
    if (!coverPickerOpen) return;
    const close = (event: KeyboardEvent) => event.key === "Escape" && setCoverPickerOpen(false);
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [coverPickerOpen]);
  useEffect(() => {
    if (!polishCandidate) return;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    const close = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        setPolishCandidate(null);
        setPolishChoices({});
      }
    };
    window.addEventListener("keydown", close);
    return () => {
      window.removeEventListener("keydown", close);
      document.body.style.overflow = previousOverflow;
    };
  }, [polishCandidate]);
  if (loading || !data) return <div className="empty-panel">正在加载文章编辑器…</div>;
  const wordCount = data.content_md.replace(/\s/g, "").length;
  const readingMinutes = Math.max(1, Math.ceil(wordCount / 350));
  const statusLabel = data.status === "published" ? "已发布" : data.status === "hidden" ? "隐藏" : "草稿";
  const toggleTag = (tagId: number) => update("tag_ids", data.tag_ids.includes(tagId) ? data.tag_ids.filter((id) => id !== tagId) : [...data.tag_ids, tagId]);
  const cover = assets.find((asset) => asset.id === data.cover_asset_id);
  const polishSource = aiTarget === "summary" ? data.summary : data.content_md;
  const polishTargetLabel = aiTarget === "summary" ? "摘要" : "正文";
  const polishHint = polishing
    ? `正在生成${polishTargetLabel}候选稿，请稍候…`
    : !polishSource.trim()
      ? ""
      : llmLoading
        ? "正在读取可用 Key…"
        : !aiConnectionId
          ? "还差一步：请选择 Key"
          : modelsLoading
            ? "正在读取模型列表…"
            : !aiModel
              ? "还差一步：请选择模型"
              : !aiPrompt.trim()
                ? "还差一步：请填写润色提示词"
                : "";
  const candidateMergeRows = polishCandidate?.rows ?? [];
  let mergedResultLine = 0;
  const mergedPreviewRows = candidateMergeRows.map((row) => {
    const side = row.changed ? (polishChoices[row.id] ?? "after") : "after";
    const text = side === "before" ? row.beforeText : row.afterText;
    return { row, side, text, resultLine: text === null ? null : ++mergedResultLine };
  });
  const addedLines = candidateMergeRows.filter((row) => row.changed && row.afterText !== null).length;
  const removedLines = candidateMergeRows.filter((row) => row.changed && row.beforeText !== null).length;
  const selectedBeforeLines = candidateMergeRows.filter((row) => row.changed && (polishChoices[row.id] ?? "after") === "before").length;
  const selectedAfterLines = candidateMergeRows.filter((row) => row.changed && (polishChoices[row.id] ?? "after") === "after").length;
  return (
    <div className="article-editor-page">
      <div className="editor-heading">
        <div className="editor-breadcrumb"><Link href="/admin/articles">文章管理</Link><span>／</span><b>{articleId ? "编辑文章" : "新文章"}</b><span className={cx("editor-status", data.status)}>{statusLabel}</span></div>
        <div className="editor-heading-row">
          <div>
            <h1>撰写文章</h1>
            <p>EDITOR · {data.slug}</p>
          </div>
          <div className="editor-actions">
            <span className={cx("editor-save-state", dirty && "is-dirty")} role="status">{saving ? "正在保存…" : dirty ? (autoSavedAt ? `已自动保存到本机 ${autoSavedAt}` : "将自动保存到本机…") : lastSavedAt ? `已保存 ${lastSavedAt}` : articleId ? "草稿已载入" : "尚未保存"}</span>
            <button disabled={saving} onClick={() => void save(data.status === "published" ? "published" : "draft")}>{data.status === "published" ? "保存修改" : "保存草稿"} <kbd>⌘S</kbd></button>
            <button disabled={saving} className="admin-primary" onClick={() => void save("published")}>{saving ? "处理中…" : data.status === "published" ? "更新文章" : "发布文章"} <span>↗</span></button>
          </div>
        </div>
      </div>
      <div className="editor-workbench">
        <section className="editor-main-column">
          <section className="editor-title-card">
            <label className="editor-field-label" htmlFor="article-title">文章标题</label>
            <input ref={titleRef} id="article-title" className="editor-title-input" value={data.title} onChange={(e) => update("title", e.target.value)} placeholder="给这篇文章起个标题…" />
            <label className="editor-summary-field" htmlFor="article-summary"><span>摘要 <small>{data.summary.length}/120</small></span><textarea id="article-summary" maxLength={120} value={data.summary} onChange={(e) => update("summary", e.target.value)} placeholder="用一两句话告诉读者，这篇文章值得读什么…" /></label>
          </section>
          <section className="editor-writing-card">
            <div className="editor-card-heading">
              <div><span className="editor-kicker">CONTENT</span><h2>正文</h2></div>
              <div className="editor-view-switch" role="tablist" aria-label="正文视图">
                <button type="button" className={!preview ? "active" : ""} onClick={() => setPreview(false)} role="tab" aria-selected={!preview}>编辑</button>
                <button type="button" className={preview ? "active" : ""} onClick={() => setPreview(true)} role="tab" aria-selected={preview}>预览</button>
              </div>
            </div>
            <div className={cx("editor-area", preview && "split")}>
              <div className="plugin-editor" data-color-mode={theme === "night" ? "dark" : "light"}><MDEditor value={data.content_md} onChange={(value) => update("content_md", value || "")} preview={preview ? "preview" : "edit"} height={500} visibleDragbar={false} textareaProps={{ "aria-label": "文章正文", placeholder: "从这里开始写下你的想法…" }} /></div>
            </div>
            <div className="editor-writing-footer"><span><b>{wordCount.toLocaleString()}</b> 字 · 约 {readingMinutes} 分钟阅读</span></div>
          </section>
        </section>
        <aside className="editor-sidebar">
          <section className="editor-side-card editor-ai-card">
            <header><div><span>AI POLISH</span><h2>AI 润色</h2></div></header>
            <div className="editor-ai-target" role="group" aria-label="润色目标"><button type="button" className={aiTarget === "summary" ? "active" : ""} aria-pressed={aiTarget === "summary"} disabled={polishing} onClick={() => { setAiTarget("summary"); setLlmError(""); }}>摘要</button><button type="button" className={aiTarget === "content_md" ? "active" : ""} aria-pressed={aiTarget === "content_md"} disabled={polishing} onClick={() => { setAiTarget("content_md"); setLlmError(""); }}>正文</button></div>
            <div className="editor-ai-fields">
              <label>使用 Key<select value={aiConnectionId ?? ""} disabled={llmLoading || polishing} onChange={(event) => selectAiConnection(event.target.value)}><option value="">{llmLoading ? "正在读取 Key…" : "请选择已保存的 Key"}</option>{llmConnections.map((connection) => <option value={connection.id} key={connection.id}>{connection.display_name}</option>)}</select></label>
              <label>使用模型<span className="editor-ai-model-row"><select value={aiModel} disabled={!aiConnectionId || modelsLoading || polishing} onChange={(event) => setAiModel(event.target.value)}><option value="">{modelsLoading ? "正在获取模型…" : aiConnectionId ? "请选择模型" : "先选择 Key"}</option>{aiModels.map((model) => <option value={model.id} key={model.id}>{model.name === model.id ? model.id : `${model.name} · ${model.id}`}</option>)}</select><button type="button" aria-label="刷新模型列表" disabled={!aiConnectionId || modelsLoading || polishing} onClick={() => aiConnectionId && void loadAiModels(aiConnectionId)}>↻</button></span></label>
              <label className="editor-ai-prompt">润色提示词<textarea value={aiPrompt} maxLength={12000} disabled={polishing} onChange={(event) => setAiPrompts((current) => ({ ...current, [aiTarget]: event.target.value }))} placeholder="告诉 AI 希望保留什么、补充什么，以及想要的语气…" /></label>
            </div>
            {(!llmLoading && !llmConnections.length) && <div className="editor-ai-empty">还没有可用 Key。<Link href="/admin/llm">先去 LLM 管理新增并验证 →</Link></div>}
            {llmError && <div className="editor-ai-error" role="alert">{llmError}</div>}
            <footer>{polishHint && <span role="status">{polishHint}</span>}<button className="admin-primary" type="button" aria-busy={polishing} onClick={() => void polishArticle()} disabled={polishing}>{polishing ? <><i className="editor-ai-wait-mark" aria-hidden="true">◆</i> 正在生成{polishTargetLabel}候选…</> : aiTarget === "summary" ? "✦ 生成摘要候选" : "✦ 生成正文候选"}</button></footer>
          </section>
          <section className="editor-side-card editor-publishing-card">
            <div className="editor-side-heading"><h2>发布设置</h2><span className={cx("editor-status-dot", data.status)}>{statusLabel}</span></div>
            <div className="editor-control"><span>分类</span><div className="taxonomy-select-row"><select value={data.category_id ?? ""} onChange={(e) => update("category_id", e.target.value ? Number(e.target.value) : null)}><option value="">选择分类</option>{categories.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select><input value={newCategory} onChange={(e) => setNewCategory(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void createCategory()} placeholder="新增分类" aria-label="新增分类名称" /><button type="button" onClick={() => void createCategory()} disabled={!newCategory.trim() || creatingTaxonomy === "category"}>＋</button></div></div>
            <div className="editor-side-divider" />
            <div className="editor-control"><span>标签</span><div className="editor-tag-picker">{tags.length ? tags.map((item) => <button type="button" key={item.id} className={data.tag_ids.includes(item.id) ? "selected" : ""} onClick={() => toggleTag(item.id)}>{item.name}<i>{data.tag_ids.includes(item.id) ? "✓" : "+"}</i></button>) : <em>暂无可用标签</em>}</div><div className="taxonomy-create-row"><input value={newTag} onChange={(e) => setNewTag(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void createTag()} placeholder="新增标签" aria-label="新增标签名称" /><button type="button" onClick={() => void createTag()} disabled={!newTag.trim() || creatingTaxonomy === "tag"}>＋ 添加标签</button></div></div>
          </section>
          <section className="editor-side-card editor-article-options-card">
            <div className="editor-side-heading"><h2>文章选项</h2><span>OPTIONS</span></div>
            <label className="editor-toggle"><input type="checkbox" checked={data.is_pinned} onChange={(e) => update("is_pinned", e.target.checked)} /><span><b>置顶文章</b></span><i /></label>
            <label className="editor-toggle"><input type="checkbox" checked={data.allow_comment} onChange={(e) => update("allow_comment", e.target.checked)} /><span><b>允许评论</b></span><i /></label>
            <label className="editor-toggle"><input type="checkbox" checked={data.kanban_ref} onChange={(e) => update("kanban_ref", e.target.checked)} /><span><b>看板娘参与</b></span><i /></label>
          </section>
          <section className="editor-side-card editor-cover-card">
            <div className="editor-side-heading"><h2>封面素材</h2><span>OPTIONAL</span></div>
            <div className="editor-cover-picker-row">{cover ? <div className="editor-cover-preview"><Image src={cover.file.url} width={180} height={100} unoptimized alt={cover.name} /><button type="button" onClick={() => update("cover_asset_id", null)}>移除</button></div> : <div className="editor-cover-empty"><span>▧</span><p>还没有选择封面</p></div>}<button type="button" className="editor-cover-select" onClick={() => setCoverPickerOpen(true)}>从素材库选择</button></div>
          </section>
        </aside>
      </div>
      {typeof document !== "undefined" && polishCandidate && createPortal(<div className="editor-diff-modal" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && closePolishComparison()}>
        <section role="dialog" aria-modal="true" aria-labelledby="polish-diff-title">
          <header><div><span>AI POLISH / THREE-WAY MERGE</span><h2 id="polish-diff-title">逐行合并{polishCandidate.target === "summary" ? "摘要" : "正文"}</h2></div><button type="button" aria-label="关闭润色对比" onClick={closePolishComparison}>×</button></header>
          <div className="editor-merge-toolbar">
            <div><code>−{removedLines} / +{addedLines}</code><span>已选：原文 {selectedBeforeLines} 行 · 润色 {selectedAfterLines} 行</span></div>
            <div><button type="button" onClick={() => chooseAllPolishLines("before")}>全部保留原文</button><button type="button" onClick={() => chooseAllPolishLines("after")}>全部采用润色</button></div>
          </div>
          <div className="editor-merge-grid" role="grid" aria-label="润色三方合并">
            <div className="editor-merge-column-head before" role="columnheader"><b>原文</b><small>点击变化行，送入中间结果</small></div>
            <div className="editor-merge-column-head result" role="columnheader"><b>最终保留</b><small>应用时写回编辑器</small></div>
            <div className="editor-merge-column-head after" role="columnheader"><b>AI 润色</b><small>点击变化行，送入中间结果</small></div>
            {mergedPreviewRows.map(({ row, side, text, resultLine }) => <div className="editor-merge-row" role="row" key={row.id}>
              <button type="button" role="gridcell" className={cx("editor-merge-cell", "before", row.changed && "changed", row.changed && side === "before" && "selected", row.beforeText === null && "empty")} disabled={!row.changed} aria-label={row.changed ? `选择原文第 ${row.beforeLine ?? "空"} 行` : undefined} onClick={() => choosePolishLine(row.id, "before")}><code>{row.beforeLine ?? "·"}</code><i aria-hidden="true">{row.changed ? "→" : ""}</i><pre>{row.beforeText ?? "（不保留此行）"}</pre></button>
              <div role="gridcell" className={cx("editor-merge-cell", "result", row.changed && "changed", row.changed && side)}><code>{resultLine ?? "·"}</code><i aria-hidden="true">{row.changed ? (side === "before" ? "L" : "R") : ""}</i><pre>{text ?? "（此行已从结果移除）"}</pre></div>
              <button type="button" role="gridcell" className={cx("editor-merge-cell", "after", row.changed && "changed", row.changed && side === "after" && "selected", row.afterText === null && "empty")} disabled={!row.changed} aria-label={row.changed ? `选择润色稿第 ${row.afterLine ?? "空"} 行` : undefined} onClick={() => choosePolishLine(row.id, "after")}><code>{row.afterLine ?? "·"}</code><i aria-hidden="true">{row.changed ? "←" : ""}</i><pre>{row.afterText ?? "（删除原文此行）"}</pre></button>
            </div>)}
          </div>
          <footer><p>点击左右两侧的变化行选择版本；中间栏就是最终写回内容，应用后仍需手动保存文章。</p><div><button type="button" onClick={closePolishComparison}>取消合并</button><button className="admin-primary" type="button" onClick={applyPolishCandidate}>应用合并结果</button></div></footer>
        </section>
      </div>, document.body)}
      {coverPickerOpen && <div className="editor-cover-modal" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && setCoverPickerOpen(false)}><section role="dialog" aria-modal="true" aria-labelledby="cover-picker-title"><header><div><span>ASSET LIBRARY</span><h2 id="cover-picker-title">选择封面素材</h2></div><button type="button" aria-label="关闭封面素材选择" onClick={() => setCoverPickerOpen(false)}>×</button></header><div className="editor-cover-grid">{assets.length ? assets.map((asset) => <button type="button" key={asset.id} className={cx(asset.id === data.cover_asset_id && "selected")} onClick={() => { update("cover_asset_id", asset.id); setCoverPickerOpen(false); }}><Image src={asset.file.url} width={180} height={105} unoptimized alt={asset.name} /><b>{asset.name}</b><small>{assetLabels[asset.media_type]}</small></button>) : <p>素材库中还没有图片，请先上传图片素材。</p>}</div><footer><Link href="/admin/assets">去素材库上传 →</Link><button type="button" onClick={() => setCoverPickerOpen(false)}>取消</button></footer></section></div>}
    </div>
  );
}
