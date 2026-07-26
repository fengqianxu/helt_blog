"use client";

import Link from "next/link";
import Image from "next/image";
import dynamic from "next/dynamic";
import { createContext, FormEvent, useCallback, useContext, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { usePathname } from "next/navigation";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { diffLines } from "diff";
import "@uiw/react-md-editor/markdown-editor.css";
import "@uiw/react-markdown-preview/markdown.css";

import { AdminAccountCenter, AdminProfileAvatar } from "./admin/AdminAccountCenter";
import { AdminLogin } from "./admin/AdminLogin";
import { AssetManager } from "./admin/AssetManager";
import { LlmSettings } from "./admin/LlmSettings";
import { RaimentSettings } from "./admin/RaimentSettings";
import { SiteSettings } from "./admin/SiteSettings";
import {
  AdminIdentity,
  AdminAsset,
  assetLabels,
  cx,
  DEFAULT_PROFILE_AVATAR_URL,
  isJsonResponse,
  Notify,
  PublicRaimentPayload,
  PublicProfile,
  RaimentSchedule,
  responseMessage,
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
  activeId: "saber",
  schedule: {
    revision: 1,
    periods: [
      { id: "period-saber", start_at: "07:00", end_at: "19:00", raiment_id: "saber" },
      { id: "period-alter-saber", start_at: "19:00", end_at: "07:00", raiment_id: "alter-saber" },
    ],
  },
};

const RaimentContext = createContext<RaimentCatalog>(DEFAULT_RAIMENT_CATALOG);

const resolveRaiment = (catalog: RaimentCatalog): Raiment =>
  catalog.items[catalog.activeId]
  || catalog.items[catalog.order[0]]
  || DEFAULT_RAIMENTS.saber;

const useRaiment = () => resolveRaiment(useContext(RaimentContext));

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
  activeId: payload.items[0]?.id || "saber",
  schedule: payload.schedule,
});

const raimentFromSchedule = (catalog: RaimentCatalog): string => {
  if (!catalog.order.length) return "saber";
  const now = new Date();
  const minutes = now.getHours() * 60 + now.getMinutes();
  const active = catalog.schedule.periods.find((period) => {
    const [startHour, startMinute] = period.start_at.split(":").map(Number);
    const [endHour, endMinute] = period.end_at.split(":").map(Number);
    const start = startHour * 60 + startMinute;
    const end = endHour * 60 + endMinute;
    return start < end
      ? minutes >= start && minutes < end
      : minutes >= start || minutes < end;
  });
  return active && catalog.items[active.raiment_id]
    ? active.raiment_id
    : catalog.order[0];
};

type ArticleCategory = { id: number; name: string; slug: string; color: string };
type ArticleTag = { id: number; name: string; article_count?: number | null };
type Post = {
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
type ArticleDetailPayload = {
  article: Post;
  previous: Post | null;
  next: Post | null;
  related: Post[];
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
const MDEditor = dynamic(() => import("@uiw/react-md-editor/nohighlight"), { ssr: false });

export function BlogApp() {
  const pathname = usePathname() || "/";
  const [raimentCatalog, setRaimentCatalog] = useState<RaimentCatalog>(DEFAULT_RAIMENT_CATALOG);
  const [activeRaimentId, setActiveRaimentId] = useState("saber");
  const theme = raimentCatalog.items[activeRaimentId]?.mode || "day";
  const [menuOpen, setMenuOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [playerOpen, setPlayerOpen] = useState(true);
  const [themeTransition, setThemeTransition] = useState(false);
  const [toast, setToast] = useState<{ message: string; tone: "normal" | "success" | "danger" } | null>(null);
  const [easterEgg, setEasterEgg] = useState(false);
  const toastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const storedRaimentPreference = useRef(false);
  const storedRaimentId = useRef<string | null>(null);

  useEffect(() => {
    const saved = localStorage.getItem("helt-raiment");
    const legacyTheme = localStorage.getItem("helt-theme") as Theme | null;
    const fallbackId = legacyTheme === "night" ? "alter-saber" : "saber";
    const nextId = saved || fallbackId;
    storedRaimentPreference.current = Boolean(saved || legacyTheme);
    storedRaimentId.current = nextId;
    const nextTheme = DEFAULT_RAIMENTS[nextId]?.mode || "day";
    document.documentElement.dataset.theme = nextTheme;
    window.requestAnimationFrame(() => {
      setActiveRaimentId(nextId);
    });
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    const loadRaiments = () => {
      void fetch("/api/v1/raiments", { signal: controller.signal, cache: "no-store" })
        .then(async (response) => {
          if (!response.ok) throw new Error("灵衣目录加载失败");
          return response.json() as Promise<PublicRaimentPayload>;
        })
        .then((payload) => {
          const catalog = catalogFromPayload(payload);
          setRaimentCatalog(catalog);
          const saved = storedRaimentId.current;
          const nextId = saved && catalog.items[saved]
            ? saved
            : raimentFromSchedule(catalog);
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
      controller.abort();
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
    const sequence = ["ArrowUp", "ArrowUp", "ArrowDown", "ArrowDown", "ArrowLeft", "ArrowRight", "ArrowLeft", "ArrowRight", "b", "a"];
    let cursor = 0;
    const onKey = (event: KeyboardEvent) => {
      cursor = event.key.toLowerCase() === sequence[cursor].toLowerCase() ? cursor + 1 : 0;
      if (cursor === sequence.length) { setEasterEgg(true); cursor = 0; }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

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
    setThemeTransition(true);
    window.setTimeout(() => {
      const currentIndex = raimentCatalog.order.indexOf(activeRaimentId);
      const nextId = raimentCatalog.order[(currentIndex + 1) % raimentCatalog.order.length]
        || raimentCatalog.order[0]
        || "saber";
      storedRaimentPreference.current = true;
      storedRaimentId.current = nextId;
      localStorage.setItem("helt-raiment", nextId);
      localStorage.removeItem("helt-theme");
      setActiveRaimentId(nextId);
    }, 310);
    window.setTimeout(() => setThemeTransition(false), 760);
  };
  const common = { theme, toggleTheme, pathname, menuOpen, setMenuOpen, searchOpen, setSearchOpen };

  if (pathname.startsWith("/admin")) return <RaimentContext.Provider value={activeCatalog}><AdminRouter pathname={pathname} theme={theme} toggleTheme={toggleTheme} notify={notify} />{themeTransition && <ThemeBlade />}{toast && <Toast {...toast} />}</RaimentContext.Provider>;

  let page: React.ReactNode;
  if (pathname === "/") page = <HomePage key={activeRaimentId} theme={theme} toggleTheme={toggleTheme} notify={notify} onSearch={() => setSearchOpen(true)} />;
  else if (pathname.startsWith("/posts/")) {
    const slug = pathname.split("/").filter(Boolean)[1];
    page = <ArticlePage slug={slug} theme={theme} notify={notify} />;
  }
  else if (pathname === "/archives") page = <ArchivesPage />;
  else if (pathname === "/moments") page = <MomentsPage />;
  else if (pathname === "/anime") page = <MediaPage />;
  else if (pathname === "/about") page = <AboutPage notify={notify} />;
  else if (pathname === "/friends") page = <FriendsPage notify={notify} />;
  else page = <NotFound />;

  return (
    <RaimentContext.Provider value={activeCatalog}><div className="site-shell">
      {pathname !== "/" && <TopNav {...common} />}
      {page}
      {pathname !== "/" && <Footer />}
      <FloatingTools theme={theme} toggleTheme={toggleTheme} playerOpen={playerOpen} setPlayerOpen={setPlayerOpen} notify={notify} />
      {searchOpen && <SearchOverlay onClose={() => setSearchOpen(false)} />}
      {themeTransition && <ThemeBlade />}
      {toast && <Toast {...toast} />}
      {easterEgg && <EasterEgg onClose={() => setEasterEgg(false)} />}
    </div></RaimentContext.Provider>
  );
}

function ThemeBlade() { return <div className="theme-blade" aria-hidden="true"><i /><i /><i /></div>; }

function Toast({ message, tone }: { message: string; tone: "normal" | "success" | "danger" }) {
  return <div className={cx("toast", `toast-${tone}`)} role="status"><span>{tone === "success" ? "✓" : tone === "danger" ? "!" : "◆"}</span>{message}</div>;
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

function ThemeSwitch({ theme, onClick, compact = false }: { theme: Theme; onClick: () => void; compact?: boolean }) {
  const catalog = useContext(RaimentContext);
  const current = resolveRaiment(catalog);
  const currentIndex = catalog.order.indexOf(current.id);
  const nextId = catalog.order[(currentIndex + 1) % catalog.order.length] || catalog.order[0];
  const next = catalog.items[nextId] || current;
  return (
    <button className={cx("theme-switch", compact && "compact")} onClick={onClick} aria-label={`切换到${next.name}`} aria-pressed={theme === "night"}>
      <span className="theme-label active">{current.name}</span>
      <span className="switch-track"><i /></span>
      {!compact && <span className="theme-label muted">{next.name}</span>}
    </button>
  );
}

function TopNav({ pathname, theme, toggleTheme, menuOpen, setMenuOpen, setSearchOpen, floating = false, elevated = false }: { pathname: string; theme: Theme; toggleTheme: () => void; menuOpen: boolean; setMenuOpen: (v: boolean) => void; searchOpen: boolean; setSearchOpen: (v: boolean) => void; floating?: boolean; elevated?: boolean }) {
  return (
    <header className={cx("top-nav", floating && "home-touchbar", elevated && "is-elevated")}>
      <Link href="/" className="brand">helt<span>.</span></Link>
      <nav id="primary-navigation" aria-label="主导航" className={cx("main-nav", menuOpen && "open")}>
        {navItems.map(([href, label]) => <Link key={href} href={href} aria-current={pathname === href ? "page" : undefined} className={pathname === href ? "active" : ""}>{label}</Link>)}
      </nav>
      <div className="nav-actions">
        <button className="search-button" onClick={() => setSearchOpen(true)} aria-label="搜索文章">⌕ <span>搜索文章…</span></button>
        <ThemeSwitch theme={theme} onClick={toggleTheme} compact />
        <button className="menu-button" onClick={() => setMenuOpen(!menuOpen)} aria-label={menuOpen ? "关闭菜单" : "打开菜单"} aria-expanded={menuOpen} aria-controls="primary-navigation">{menuOpen ? "×" : "☰"}</button>
      </div>
    </header>
  );
}

function HomePage({ theme, toggleTheme, notify, onSearch }: { theme: Theme; toggleTheme: () => void; notify: Notify; onSearch: () => void }) {
  const raiment = useRaiment();
  const [page, setPage] = useState(1);
  const [posts, setPosts] = useState<Post[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [menuOpen, setMenuOpen] = useState(false);
  const [navElevated, setNavElevated] = useState(false);
  const [voicePlaying, setVoicePlaying] = useState(false);
  const [voiceSeconds, setVoiceSeconds] = useState(0);
  const [voiceDuration, setVoiceDuration] = useState(0);
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
  useArtalkCommentCounts(!loading && !error ? commentCountKey : "");
  const enter = () => document.getElementById("articles")?.scrollIntoView({ behavior: "smooth" });
  useEffect(() => {
    const updateNav = () => setNavElevated(window.scrollY > 56);
    updateNav();
    window.addEventListener("scroll", updateNav, { passive: true });
    return () => window.removeEventListener("scroll", updateNav);
  }, []);
  useEffect(() => () => {
    voiceRef.current?.pause();
    voiceRef.current = null;
  }, [raiment.coverVoiceUrl]);
  const toggleCoverVoice = () => {
    if (!raiment.coverVoiceUrl) {
      notify("当前灵衣还没有配置封面语音", "danger");
      return;
    }
    let audio = voiceRef.current;
    if (!audio || audio.src !== new URL(raiment.coverVoiceUrl, window.location.href).href) {
      audio?.pause();
      audio = new Audio(raiment.coverVoiceUrl);
      audio.preload = "metadata";
      audio.ontimeupdate = () => setVoiceSeconds(Math.floor(audio?.currentTime || 0));
      audio.onloadedmetadata = () => setVoiceDuration(Math.floor(audio?.duration || 0));
      audio.onended = () => {
        setVoicePlaying(false);
        setVoiceSeconds(0);
        notify("语音播放完成", "success");
      };
      voiceRef.current = audio;
    }
    if (voicePlaying) {
      audio.pause();
      setVoicePlaying(false);
      notify("语音已暂停");
    } else {
      void audio.play().then(() => {
        setVoicePlaying(true);
        notify("开始播放开屏语音");
      }).catch(() => notify("浏览器阻止了语音播放，请再试一次", "danger"));
    }
  };
  const changePage = (next: number) => {
    const target = Math.min(pageCount, Math.max(1, next));
    if (target === page) return;
    setPage(target);
    document.querySelector(".section-intro")?.scrollIntoView({ behavior: "smooth", block: "start" });
  };
  const latestLabel = posts[0]
    ? `最近更新：《${posts[0].title}》 · ${articleDate(posts[0])}`
    : loading
      ? "正在读取最新文章…"
      : "暂时还没有已发布文章。";
  return (
    <>
      <TopNav pathname="/" theme={theme} toggleTheme={toggleTheme} menuOpen={menuOpen} setMenuOpen={setMenuOpen} searchOpen={false} setSearchOpen={() => onSearch()} floating elevated={navElevated} />
      <section className="hero">
        <div className="hero-stripe stripe-one" /><div className="hero-stripe stripe-two" />
        <Image className="hero-art" src={raiment.cover} width={5120} height={2160} sizes="(max-width: 768px) 100vw, 64vw" priority unoptimized alt={`${raiment.name} 灵衣封面`} />
        <div className="hero-copy">
          <div className="eyebrow"><i /> SINCE 2020 · HELT&apos;S BLOG</div>
          <h1>{raiment.coverTitle}</h1>
          <p>{raiment.coverSubtitle}</p>
          <div className="hero-actions">
            <button className={cx("voice-button", voicePlaying && "is-playing")} aria-pressed={voicePlaying} onClick={toggleCoverVoice}><b>{voicePlaying ? "Ⅱ" : "▶"}</b><span className="wave"><i /><i /><i /><i /><i /><i /></span><span>{voicePlaying ? `播放中 ${Math.floor(voiceSeconds / 60)}:${String(voiceSeconds % 60).padStart(2, "0")} / ${Math.floor(voiceDuration / 60)}:${String(voiceDuration % 60).padStart(2, "0")}` : raiment.coverVoiceLabel}</span></button>
            <button className="primary-button" onClick={enter}>ENTER · 进入博客 ▾</button>
          </div>
        </div>
        <div className="dialog-box hero-dialog"><b>{raiment.coverCharacterName}</b><p>{raiment.coverDialogue}{latestLabel}</p><span>▼</span></div>
        <button className="scroll-cue" onClick={enter}>SCROLL ▼</button>
      </section>
      <section id="articles" className="home-content">
        <Stats total={total} />
        <div className="section-intro reveal"><div><span>RECENT WRITING</span><h2>最近写下的东西</h2></div><p>技术、生活与热爱的作品。每一页都尽量留下真实的温度。</p></div>
        <div className="post-list">
          <div className="post-page" key={page}>
            {loading && <div className="empty-panel">正在读取文章…</div>}
            {!loading && error && <div className="empty-panel">{error}</div>}
            {!loading && !error && !visiblePosts.length && <div className="empty-panel">暂时还没有已发布文章。</div>}
            {!loading && !error && visiblePosts.map((post) => <PostCard key={post.id} post={post} />)}
          </div>
          <div className="pagination" aria-label="文章分页"><button onClick={() => changePage(page - 1)} disabled={page === 1} aria-label="上一页">◀</button>{Array.from({ length: pageCount }, (_, i) => i + 1).map((item) => <button key={item} className={page === item ? "current" : ""} onClick={() => changePage(item)} aria-current={page === item ? "page" : undefined}>{item}</button>)}<button onClick={() => changePage(page + 1)} disabled={page === pageCount} aria-label="下一页">▶</button></div>
        </div>
      </section>
    </>
  );
}

function Stats({ total }: { total: number }) {
  return <div className="stats">{[[total.toLocaleString(), "文章"], ["—", "总字数"], ["—", "访客"], ["—", "运行天数"]].map(([n, l]) => <div key={l}><b>{n}</b><span>{l}</span></div>)}</div>;
}

function PostCard({ post }: { post: Post }) {
  return (
    <Link href={`/posts/${post.slug}`} className={cx("post-card", post.is_pinned && "pinned")}>
      {post.is_pinned && <span className="pin">置顶 PINNED</span>}
      <div className="post-main"><div className="post-meta"><span className="tag">{categoryName(post)}</span><span>{articleDate(post)}</span></div><h2>{post.title}</h2><p>{post.summary}</p><div className="post-stats"><span>{articleTime(post)}</span><span>{articleWords(post)}</span><span>评论 <span className="artalk-comment-count" data-page-key={articleCommentKey(post.slug)}>0</span></span></div></div>
      {post.cover_url && <Image src={post.cover_url} width={512} height={288} sizes="240px" alt={`${post.title} 封面`} unoptimized />}
    </Link>
  );
}

function PageHeading({ title, subtitle }: { title: string; subtitle: string }) { return <div className="page-heading"><h1>{title}</h1><span>{subtitle}</span></div>; }

function ArticlePage({ slug, theme, notify }: { slug: string; theme: Theme; notify: Notify }) {
  const [progress, setProgress] = useState(0);
  const [liked, setLiked] = useState(false);
  const [payload, setPayload] = useState<ArticleDetailPayload | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    const controller = new AbortController();
    fetch(`/api/v1/articles/${encodeURIComponent(slug)}`, { signal: controller.signal })
      .then(async (response) => {
        if (!response.ok) throw new Error(await responseMessage(response, "文章不存在"));
        return response.json() as Promise<ArticleDetailPayload>;
      })
      .then((value) => {
        setPayload(value);
        setError("");
      })
      .catch((reason) => {
        if (!(reason instanceof DOMException && reason.name === "AbortError")) {
          setError(reason instanceof Error ? reason.message : "文章加载失败");
        }
      });
    return () => controller.abort();
  }, [slug]);
  useEffect(() => {
    const update = () => {
      const max = document.documentElement.scrollHeight - window.innerHeight;
      setProgress(max > 0 ? Math.min(100, (window.scrollY / max) * 100) : 0);
    };
    update(); window.addEventListener("scroll", update, { passive: true });
    return () => window.removeEventListener("scroll", update);
  }, []);
  const shareArticle = async () => {
    try {
      if (!navigator.clipboard) throw new Error("Clipboard API unavailable");
      await navigator.clipboard.writeText(location.href);
      notify("文章链接已复制", "success");
    } catch {
      notify("暂时无法复制，请从地址栏复制链接", "danger");
    }
  };
  if (error) return <NotFound message={error} />;
  if (!payload) return <main className="empty-state"><b>◆</b><h1>正在读取文章</h1><p>请稍候，Master。</p></main>;
  const post = payload.article;
  const previousPost = payload.previous;
  const nextPost = payload.next;
  const related = payload.related;
  return (
    <><div className="article-reading-bar"><i style={{ width: `${progress}%` }} /></div><main className="page-wrap article-layout page-enter">
      <article className="article-card">
        <div className="breadcrumbs"><Link href="/">首页</Link> / <Link href="/archives">{categoryName(post)}</Link></div>
        <h1>{post.title}</h1>
        <div className="article-meta">{articleDate(post)} · {articleWords(post)} · {articleTime(post)} · 阅读 {post.view_count}</div>
        <p>{post.summary}</p>
        {post.cover_url && <Image className="article-image" src={post.cover_url} width={1200} height={675} sizes="(max-width: 768px) 100vw, 680px" alt={`${post.title} 封面`} unoptimized />}
        <MarkdownBody source={post.content_md || "这篇文章还没有正文。"} />
        <div id="article-actions" className="article-actions"><button className={liked ? "liked" : ""} onClick={() => { setLiked(!liked); notify(liked ? "已取消喜欢" : "感谢你的喜欢", "success"); }}>{liked ? "♥ 已喜欢" : "♡ 喜欢"}</button><button onClick={shareArticle}>⌁ 分享文章</button></div>
        <div className="article-nav">{previousPost ? <Link href={`/posts/${previousPost.slug}`}>← 上一篇<br /><b>{previousPost.title}</b></Link> : <span />}{nextPost ? <Link href={`/posts/${nextPost.slug}`}>下一篇 →<br /><b>{nextPost.title}</b></Link> : <span />}</div>
        {payload.allow_comment
          ? <Comments slug={post.slug} title={post.title} theme={theme} />
          : <section className="comments comments-disabled"><h2>评论</h2><p>这篇文章已关闭评论。</p></section>}
      </article>
      <aside className="article-aside"><div className="toc"><b>目录 CONTENTS <small>{Math.round(progress)}%</small></b><Link href="#article-content" className="active">正文</Link><Link href="#article-actions">互动</Link></div><div className="recommend"><b>相关文章</b>{related.length ? related.map((item) => <Link key={item.id} href={`/posts/${item.slug}`}>{item.title}</Link>) : <Link href="/archives">浏览全部文章</Link>}</div></aside>
    </main></>
  );
}

function MarkdownBody({ source }: { source: string }) {
  return <div id="article-content" className="article-content markdown-renderer"><ReactMarkdown remarkPlugins={[remarkGfm]}>{source}</ReactMarkdown></div>;
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
    }).catch(() => {
      if (!cancelled) setLoadError("评论组件加载失败，请刷新页面后重试。");
    });
    return () => {
      cancelled = true;
      artalkRef.current?.destroy();
      artalkRef.current = null;
    };
  }, [slug, title]);

  useEffect(() => {
    artalkRef.current?.setDarkMode(theme === "night");
  }, [theme]);

  return <section className="comments" aria-labelledby="article-comments-title">
    <h2 id="article-comments-title">评论 · <span className="artalk-comment-count" data-page-key={articleCommentKey(slug)}>0</span></h2>
    {loadError && <p className="comment-load-error" role="alert">{loadError}</p>}
    <div ref={containerRef} className="artalk-host" />
    <small className="comment-privacy-note">评论由 Artalk 提供；提交时会处理昵称、邮箱、IP 地址与浏览器信息，用于身份识别和反垃圾。</small>
  </section>;
}

function ArchivesPage() {
  const [view, setView] = useState("time");
  const [selected, setSelected] = useState("");
  const [posts, setPosts] = useState<Post[]>([]);
  const [categories, setCategories] = useState<ArticleCategory[]>([]);
  const [tags, setTags] = useState<ArticleTag[]>([]);
  const [total, setTotal] = useState(0);
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
    void load().catch(() => undefined);
    return () => controller.abort();
  }, []);
  const choices = view === "category"
    ? categories.map((item) => `${item.name} ${posts.filter((post) => post.category?.id === item.id).length}`)
    : tags.map((item) => item.name);
  const matchedPosts = selected
    ? posts.filter((post) => categoryName(post) === selected || post.tags.some((tag) => tag.name === selected) || `${post.title} ${post.summary}`.includes(selected))
    : [];
  return <main className="page-wrap page-enter"><PageHeading title="归档" subtitle={`ARCHIVE · ${total} 篇`} /><div className="view-tabs" role="tablist" aria-label="归档浏览方式"><button role="tab" aria-selected={view === "time"} className={view === "time" ? "active" : ""} onClick={() => { setView("time"); setSelected(""); }}>时间</button><button role="tab" aria-selected={view === "category"} className={view === "category" ? "active" : ""} onClick={() => { setView("category"); setSelected(""); }}>分类</button><button role="tab" aria-selected={view === "tag"} className={view === "tag" ? "active" : ""} onClick={() => { setView("tag"); setSelected(""); }}>标签</button></div>{view === "time" ? <div className="archive-grid"><div className="timeline-list"><h2>文章</h2>{posts.map((p) => <Link key={p.id} href={`/posts/${p.slug}`}><time>{articleDate(p).slice(5)}</time><b>{p.title}</b><span className="tag">{categoryName(p)}</span></Link>)}</div><aside className="archive-side"><b>分类</b>{categories.map((item) => <button key={item.id} onClick={() => { setView("category"); setSelected(item.name); }}><span>{item.name}</span><span>{posts.filter((post) => post.category?.id === item.id).length}</span></button>)}</aside></div> : <><div className="cloud-panel">{choices.map((item, i) => { const label = item.split(" ")[0]; return <button className={selected === label ? "active" : ""} key={item} style={{ fontSize: `${14 + (i % 4) * 4}px` }} onClick={() => setSelected(label)}>{item}</button>; })}</div>{selected && <div className="archive-selection"><span>正在浏览</span><b>{selected}</b><p>共找到 {matchedPosts.length} 篇相关内容</p><button onClick={() => setSelected("")}>清除筛选 ×</button></div>}{selected && <div className="archive-results">{matchedPosts.length ? matchedPosts.map((post) => <Link key={post.id} href={`/posts/${post.slug}`}><span className="tag">{categoryName(post)}</span><b>{post.title}</b><small>{articleDate(post)}</small></Link>) : <div className="empty-panel">没有匹配的已发布文章。</div>}</div>}</>}</main>;
}

function MomentsPage() {
  const [liked, setLiked] = useState<number[]>([]);
  const moments = [{ date: "07.21", text: "新博客的开屏动画调了一晚上，语音淡入的时机终于对了。就是这个感觉。", mood: "开发日志" }, { date: "07.14", text: "周末去了漫展，战利品合影。", mood: "日常" }, { date: "07.02", text: "博客运行满 2000 天了。谢谢每一个来过的人。", mood: "纪念" }];
  return <main className="page-wrap narrow page-enter"><PageHeading title="时间轴" subtitle="MOMENTS · 碎碎念" /><div className="moments">{moments.map((m, i) => <article key={m.date}><div className="moment-date"><b>{m.date}</b><span>2026</span></div><div className="moment-card"><span className="tag">{m.mood}</span><p>{m.text}</p>{i === 1 && <div className="photo-placeholder"><span>COMIC MARKET</span><b>MEMORY / 07.14</b></div>}<div className="moment-actions"><button className={liked.includes(i) ? "liked" : ""} onClick={() => setLiked((items) => items.includes(i) ? items.filter((x) => x !== i) : [...items, i])}>{liked.includes(i) ? "♥" : "♡"} {18 - i * 3 + (liked.includes(i) ? 1 : 0)}</button></div></div></article>)}</div></main>;
}

function AboutPage({ notify }: { notify: Notify }) {
  const [profile, setProfile] = useState<PublicProfile>({
    username: "helt",
    email: "",
    avatar_url: null,
  });

  useEffect(() => {
    const controller = new AbortController();
    void fetch("/api/v1/profile", {
      headers: { accept: "application/json" },
      signal: controller.signal,
    })
      .then((response) => response.ok ? response.json() as Promise<PublicProfile> : null)
      .then((payload) => payload && setProfile(payload))
      .catch((error) => {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          notify("暂时无法读取站点资料");
        }
      });
    return () => controller.abort();
  }, [notify]);

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
  const displayName = profile.username || "helt";
  const avatarUrl = profile.avatar_url || DEFAULT_PROFILE_AVATAR_URL;
  return <main className="page-wrap about-layout page-enter"><aside className="profile-card"><div className="avatar"><Image src={avatarUrl} width={216} height={216} sizes="108px" unoptimized alt={`${displayName} 的头像`} /></div><h1>{displayName}.</h1><p>写代码 / 追番 / 折腾博客<br />骑士王的头号 Master</p><div className="profile-stats"><span><b>128</b>文章</span><span><b>2,048</b>天</span></div><div className="socials"><button aria-label="GitHub" onClick={() => notify("GitHub 主页为演示链接")}>GH</button><button aria-label="哔哩哔哩" onClick={() => notify("哔哩哔哩主页为演示链接")}>BL</button><button aria-label="复制联系邮箱" onClick={copyEmail} disabled={!profile.email}>✉</button></div></aside><div className="about-content"><PageHeading title="关于我" subtitle="ABOUT · MASTER PROFILE" /><div className="dialog-box"><b>{displayName}</b><p>你好，欢迎来到我的小站。白天是普通的程序员，晚上是熬夜追番的 Master。这个博客从 2020 年写到现在，记录技术、生活，和那些让我热血的作品。</p></div><SectionTitle index="01" title="技能与兴趣" /><div className="skill-grid">{["React / TypeScript", "Node.js", "UI Engineering", "Fate Series", "摄影与键盘", "动画与游戏"].map((s) => <span key={s}>{s}</span>)}</div><SectionTitle index="02" title="关于本站" /><p>本站从设计到代码都在持续重构中。按下 Konami 秘技（↑↑↓↓←→←→BA）会触发隐藏彩蛋；日夜切换时，Saber 与 Alter 的视觉也会一起切换。</p></div></main>;
}

function FriendsPage({ notify }: { notify: Notify }) {
  const [sent, setSent] = useState(false);
  const [selected, setSelected] = useState<{ name: string; desc: string; color: string } | null>(null);
  const friends = [{ name: "Aki's Notes", desc: "设计、代码与生活碎片", color: "blue" }, { name: "Rin Lab", desc: "前端工程与视觉实验", color: "red" }, { name: "Mooncell", desc: "Fate 系作品考据笔记", color: "gold" }, { name: "夜航船", desc: "独立开发与数字生活", color: "violet" }, { name: "Kumo", desc: "摄影、旅行和咖啡", color: "green" }, { name: "404 Garden", desc: "在互联网角落种花", color: "dark" }];
  const submit = (e: FormEvent) => { e.preventDefault(); setSent(true); notify("友链申请已提交", "success"); };
  useEffect(() => {
    if (!selected) return;
    const close = (event: KeyboardEvent) => event.key === "Escape" && setSelected(null);
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [selected]);
  return <main className="page-wrap page-enter"><PageHeading title="友情链接" subtitle="FRIENDS · 12 位" /><div className="friends-grid">{friends.map((f) => <button key={f.name} className="friend-card" onClick={() => setSelected(f)}><span className={`friend-avatar ${f.color}`}>{f.name[0]}</span><span><b>{f.name}</b><p>{f.desc}</p></span><i>→</i></button>)}</div><form className="friend-form dialog-box" onSubmit={submit}><b>申请友链</b><p>交换的不只是链接，也是彼此在网络世界留下的一盏灯。</p><div className="form-grid"><input aria-label="站点名称" placeholder="站点名称" required /><input aria-label="站点地址" placeholder="站点地址" type="url" required /><input aria-label="头像地址" placeholder="头像地址" type="url" /><input aria-label="联系邮箱" placeholder="联系邮箱" type="email" required /></div><textarea aria-label="站点介绍" placeholder="一句话介绍你的小站" required /><button className="primary-button">提交申请</button>{sent && <span className="success" role="status">申请已提交（Mock）</span>}</form>{selected && <div className="friend-drawer" role="dialog" aria-modal="true" aria-labelledby="friend-drawer-title"><button aria-label="关闭友链详情" onClick={() => setSelected(null)}>×</button><span className={`friend-avatar ${selected.color}`}>{selected.name[0]}</span><small>FRIEND PROFILE</small><h2 id="friend-drawer-title">{selected.name}</h2><p>{selected.desc}</p><button className="primary-button" onClick={() => notify("这是 Mock 友链，暂未配置外部地址")}>访问站点 ↗</button></div>}</main>;
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

function FloatingTools({ theme, toggleTheme, playerOpen, setPlayerOpen, notify }: { theme: Theme; toggleTheme: () => void; playerOpen: boolean; setPlayerOpen: (v: boolean) => void; notify: Notify }) {
  const raiment = useRaiment();
  const tracks = [{ title: "THIS ILLUSION", artist: "LiSA · Fate OST" }, { title: "to the beginning", artist: "Kalafina" }, { title: "花の唄", artist: "Aimer" }];
  const [track, setTrack] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [showTop, setShowTop] = useState(false);
  const [chatOpen, setChatOpen] = useState(false);
  const [chatText, setChatText] = useState("");
  const [chatReply, setChatReply] = useState<{ theme: Theme; text: string } | null>(null);
  useEffect(() => { const watch = () => setShowTop(window.scrollY > 500); watch(); window.addEventListener("scroll", watch, { passive: true }); return () => window.removeEventListener("scroll", watch); }, []);
  const moveTrack = (delta: number) => { const next = (track + delta + tracks.length) % tracks.length; setTrack(next); setPlaying(true); notify(`正在播放：${tracks[next].title}`); };
  const sendChat = (text = chatText) => { if (!text.trim()) return; setChatReply({ theme, text: "我明白了，Master。这个演示暂时由 Mock 数据回应，但交互链路已经完整。" }); setChatText(""); };
  return <><div className={cx("music-player", !playerOpen && "collapsed", playing && "is-playing")}><button className="disc" onClick={() => playerOpen ? setPlaying(!playing) : setPlayerOpen(true)} aria-label={playerOpen ? (playing ? "暂停音乐" : "播放音乐") : "展开播放器"}><i /></button>{playerOpen && <><div><b>{tracks[track].title}</b><span>{tracks[track].artist}</span><i className="progress"><i /></i></div><button onClick={() => moveTrack(-1)} aria-label="上一首">⏮</button><button onClick={() => { setPlaying(!playing); notify(playing ? "音乐已暂停" : `正在播放：${tracks[track].title}`); }} aria-label={playing ? "暂停" : "播放"}>{playing ? "Ⅱ" : "▶"}</button><button onClick={() => moveTrack(1)} aria-label="下一首">⏭</button><button className="collapse-player" onClick={() => setPlayerOpen(false)} aria-label="收起播放器">−</button></>}</div>{chatOpen && <div className="kanban-chat" role="dialog" aria-label="看板娘对话"><button className="close-chat" aria-label="关闭看板娘对话" onClick={() => setChatOpen(false)}>×</button><div className="kanban-name">{raiment.shortName} · GUIDE</div><p>{chatReply?.theme === theme ? chatReply.text : raiment.kanban.greeting}</p><div className="quick-replies"><button onClick={() => sendChat("推荐文章")}>推荐文章</button><button onClick={() => sendChat("怎么切换主题")}>主题说明</button></div><form onSubmit={(e) => { e.preventDefault(); sendChat(); }}><input aria-label="发送给看板娘的消息" value={chatText} onChange={(e) => setChatText(e.target.value)} placeholder={`问问 ${raiment.kanban.displayName}…`} /><button>发送</button></form></div>}<div className="floating-tools">{showTop && <button className="back-top" onClick={() => window.scrollTo({ top: 0, behavior: "smooth" })} aria-label="返回顶部">▲</button>}<button className={chatOpen ? "active" : ""} onClick={() => setChatOpen(!chatOpen)} aria-label={chatOpen ? "关闭看板娘" : "打开看板娘"} aria-expanded={chatOpen}>♙</button><button className="floating-theme" onClick={toggleTheme} aria-label="切换灵衣">衣</button></div></>;
}

function Footer() { return <footer><Link href="/" className="brand">helt<span>.</span></Link><p>写代码、追番、折腾博客的个人小站。</p><span>© 2020—2026 helt. · POWERED BY REACT</span></footer>; }

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

const adminNav = [["/admin", "▦", "仪表盘"], ["/admin/articles", "▤", "文章管理"], ["/admin/comments", "◫", "评论审核"], ["/admin/assets", "▧", "素材库"], ["/admin/raiments", "♙", "灵衣"], ["/admin/llm", "✦", "LLM"], ["/admin/media", "♫", "音乐与语音"], ["/admin/settings", "⚙", "站点设置"]];

function AdminLayout({ pathname, theme, toggleTheme, notify, admin }: { pathname: string; theme: Theme; toggleTheme: () => void; notify: Notify; admin: AdminIdentity }) {
  const [commandOpen, setCommandOpen] = useState(false);
  const [accountOpen, setAccountOpen] = useState(false);
  const [currentAdmin, setCurrentAdmin] = useState(admin);
  const current = adminNav.find(([href]) => pathname === href)?.[2] || (pathname.includes("articles") ? "文章编辑器" : "仪表盘");
  let content: React.ReactNode;
  if (pathname === "/admin") content = <Dashboard />;
  else if (pathname === "/admin/articles") content = <ArticleManager notify={notify} />;
  else if (pathname.includes("/admin/articles/")) content = <ArticleEditor pathname={pathname} theme={theme} notify={notify} />;
  else if (pathname === "/admin/comments") content = <CommentManager />;
  else if (pathname === "/admin/assets") content = <AssetManager notify={notify} />;
  else if (pathname === "/admin/raiments" || pathname === "/admin/appearance") content = <RaimentSettings notify={notify} />;
  else if (pathname === "/admin/llm" || pathname === "/admin/kanban") content = <LlmSettings notify={notify} />;
  else if (pathname === "/admin/media") content = <MediaSettings notify={notify} />;
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
        <Link href="/admin" className="brand">helt<span>.</span> <small>ADMIN</small></Link>
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
            <ThemeSwitch theme={theme} onClick={toggleTheme} compact />
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

function Dashboard() {
  return <><AdminTitle title="仪表盘" sub="WELCOME BACK, MASTER · 2026.07.23" action={<Link href="/admin/articles/new" className="admin-primary">＋ 撰写新文章</Link>} /><div className="admin-stats">{[["128", "文章总数", "+3 本月"], ["187,203", "累计访客", "+12.4%"], ["Artalk", "评论系统", "独立审核控制台"], ["2,048", "运行天数", "99.98%"]].map(([n, l, d]) => <article key={l}><span>{l}</span><b>{n}</b><small>{d}</small></article>)}</div><div className="dashboard-grid"><section className="admin-panel"><h2>访问趋势 <small>LAST 14 DAYS</small></h2><div className="chart">{[35, 52, 43, 66, 58, 78, 72, 88, 60, 82, 76, 94, 86, 100].map((n, i) => <i key={i} style={{ height: `${n}%` }} />)}</div></section><section className="admin-panel recent-comments"><h2>评论系统 <Link href="/admin/comments">管理 →</Link></h2><div><span>A</span><p><b>Artalk 已接入</b>最新评论、审核队列与评论统计请在评论控制台查看。</p></div></section></div><section className="admin-panel quick"><h2>快速操作</h2><div><Link href="/admin/articles/new">✎<span>新建文章</span></Link><Link href="/admin/assets">▧<span>上传素材</span></Link><Link href="/admin/raiments">♙<span>管理灵衣</span></Link><Link href="/admin/settings">⚙<span>站点设置</span></Link></div></section></>;
}

function ArticleManager({ notify }: { notify: Notify }) {
  const [filter, setFilter] = useState("全部");
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const [rows, setRows] = useState<Post[]>([]);
  const [selected, setSelected] = useState<number[]>([]);
  const [deleteConfirmation, setDeleteConfirmation] = useState<{ ids: number[]; title?: string } | null>(null);
  const [deleting, setDeleting] = useState(false);
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
  const pageCount = Math.max(1, Math.ceil(total / perPage));
  return <><AdminTitle title="文章管理" sub={`ARTICLES · ${total} 条结果`} action={<Link href="/admin/articles/new" className="admin-primary">＋ 新建文章</Link>} /><div className="admin-toolbar"><div>{["全部", "已发布", "草稿", "置顶"].map((x) => <button key={x} className={filter === x ? "active" : ""} onClick={() => { setFilter(x); setPage(1); }}>{x}</button>)}</div><input aria-label="搜索文章标题" value={query} onChange={(e) => { setQuery(e.target.value); setPage(1); }} placeholder="搜索标题…" /></div>{selected.length > 0 && <div className="admin-toolbar"><span>已选择 {selected.length} 篇</span><button onClick={() => void batch("publish")}>发布</button><button onClick={() => void batch("unpublish")}>撤回</button><button onClick={() => void batch("pin")}>置顶</button><button onClick={() => void batch("delete")}>删除</button></div>}<div className="admin-table"><div className="table-head"><span>选择</span><span>标题</span><span>分类</span><span>状态</span><span>数据</span><span>日期</span><span>操作</span></div>{loading && <div className="empty-panel">正在加载文章…</div>}{!loading && rows.map((p) => <div className="table-row" key={p.id}><span><input type="checkbox" aria-label={`选择 ${p.title}`} checked={selected.includes(p.id)} onChange={(event) => setSelected((items) => event.target.checked ? [...items, p.id] : items.filter((id) => id !== p.id))} /></span><b>{p.is_pinned && <em>置顶</em>}{p.title}</b><span className="tag">{categoryName(p)}</span><span className={p.status === "published" ? "published" : "draft"}>{p.status === "published" ? "● 已发布" : p.status === "hidden" ? "◌ 隐藏" : "◐ 草稿"}</span><small>{p.view_count} 阅 · <span className="artalk-comment-count" data-page-key={articleCommentKey(p.slug)}>0</span> 评</small><small>{articleDate(p).slice(5)}</small><span className="row-actions"><Link href={`/admin/articles/${p.id}/edit`}>编辑</Link>{p.status === "published" && <Link href={`/posts/${p.slug}`}>预览</Link>}<button onClick={() => remove(p.id, p.title)}>删除</button></span></div>)}{!loading && !rows.length && <div className="empty-panel">没有符合当前筛选的文章。</div>}</div>{pageCount > 1 && <div className="pagination admin-article-pagination" aria-label="后台文章分页"><button disabled={page === 1 || loading} onClick={() => setPage((value) => Math.max(1, value - 1))} aria-label="上一页">◀</button><span>第 {page} / {pageCount} 页</span><button disabled={page === pageCount || loading} onClick={() => setPage((value) => Math.min(pageCount, value + 1))} aria-label="下一页">▶</button></div>}{deleteConfirmation && <div className="admin-account-dialog article-delete-overlay" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && !deleting && setDeleteConfirmation(null)}><section className="article-delete-dialog" role="dialog" aria-modal="true" aria-labelledby="article-delete-title"><header><div><span>ARTICLES / DELETE</span><h2 id="article-delete-title">确认删除文章</h2></div><button type="button" aria-label="关闭删除确认" disabled={deleting} onClick={() => setDeleteConfirmation(null)}>×</button></header><p>{deleteConfirmation.ids.length === 1 && deleteConfirmation.title ? `确定删除《${deleteConfirmation.title}》？` : `确定删除选中的 ${deleteConfirmation.ids.length} 篇文章？`}此操作不能撤销。</p><footer><button type="button" disabled={deleting} onClick={() => setDeleteConfirmation(null)}>取消</button><button className="danger" type="button" disabled={deleting} onClick={() => void confirmDelete()}>{deleting ? "正在删除…" : "确认删除"}</button></footer></section></div>}</>;
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

function buildEditorMergeRows(before: string, after: string): EditorMergeRow[] {
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
      const rows = buildEditorMergeRows(source, payload.text);
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
      ? `先填写${polishTargetLabel}草稿`
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
                : "已就绪；生成后先查看 Diff，再决定是否替换。";
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
            <div className="editor-writing-footer"><span>Markdown 编辑器插件 · 支持表格、任务列表与代码块</span><span><b>{wordCount.toLocaleString()}</b> 字 · 约 {readingMinutes} 分钟阅读</span></div>
          </section>
        </section>
        <aside className="editor-sidebar">
          <section className="editor-side-card editor-ai-card">
            <header><div><span>AI POLISH</span><h2>AI 润色</h2></div><small>本次文章专用</small></header>
            <p>生成后先查看 Diff，确认前不会替换原文。</p>
            <div className="editor-ai-target" role="group" aria-label="润色目标"><button type="button" className={aiTarget === "summary" ? "active" : ""} aria-pressed={aiTarget === "summary"} disabled={polishing} onClick={() => { setAiTarget("summary"); setLlmError(""); }}>摘要</button><button type="button" className={aiTarget === "content_md" ? "active" : ""} aria-pressed={aiTarget === "content_md"} disabled={polishing} onClick={() => { setAiTarget("content_md"); setLlmError(""); }}>正文</button></div>
            <div className="editor-ai-fields">
              <label>使用 Key<select value={aiConnectionId ?? ""} disabled={llmLoading || polishing} onChange={(event) => selectAiConnection(event.target.value)}><option value="">{llmLoading ? "正在读取 Key…" : "请选择已保存的 Key"}</option>{llmConnections.map((connection) => <option value={connection.id} key={connection.id}>{connection.display_name}</option>)}</select></label>
              <label>使用模型<span className="editor-ai-model-row"><select value={aiModel} disabled={!aiConnectionId || modelsLoading || polishing} onChange={(event) => setAiModel(event.target.value)}><option value="">{modelsLoading ? "正在获取模型…" : aiConnectionId ? "请选择模型" : "先选择 Key"}</option>{aiModels.map((model) => <option value={model.id} key={model.id}>{model.name === model.id ? model.id : `${model.name} · ${model.id}`}</option>)}</select><button type="button" aria-label="刷新模型列表" disabled={!aiConnectionId || modelsLoading || polishing} onClick={() => aiConnectionId && void loadAiModels(aiConnectionId)}>↻</button></span></label>
              <label className="editor-ai-prompt">润色提示词<textarea value={aiPrompt} maxLength={12000} disabled={polishing} onChange={(event) => setAiPrompts((current) => ({ ...current, [aiTarget]: event.target.value }))} placeholder="告诉 AI 希望保留什么、补充什么，以及想要的语气…" /></label>
            </div>
            {(!llmLoading && !llmConnections.length) && <div className="editor-ai-empty">还没有可用 Key。<Link href="/admin/llm">先去 LLM 管理新增并验证 →</Link></div>}
            {llmError && <div className="editor-ai-error" role="alert">{llmError}</div>}
            <footer><span role="status">{polishHint}</span><button className="admin-primary" type="button" aria-busy={polishing} onClick={() => void polishArticle()} disabled={polishing}>{polishing ? <><i className="editor-ai-wait-mark" aria-hidden="true">◆</i> 正在生成{polishTargetLabel}候选…</> : aiTarget === "summary" ? "✦ 生成摘要候选" : "✦ 生成正文候选"}</button></footer>
          </section>
          <section className="editor-side-card editor-publishing-card">
            <div className="editor-side-heading"><h2>发布设置</h2><span className={cx("editor-status-dot", data.status)}>{statusLabel}</span></div>
            <div className="editor-control"><span>分类 <small>选择已有或新增</small></span><div className="taxonomy-select-row"><select value={data.category_id ?? ""} onChange={(e) => update("category_id", e.target.value ? Number(e.target.value) : null)}><option value="">选择分类</option>{categories.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select><input value={newCategory} onChange={(e) => setNewCategory(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void createCategory()} placeholder="新增分类" aria-label="新增分类名称" /><button type="button" onClick={() => void createCategory()} disabled={!newCategory.trim() || creatingTaxonomy === "category"}>＋</button></div></div>
            <div className="editor-side-divider" />
            <div className="editor-control"><span>标签 <small>可多选、可新增</small></span><div className="editor-tag-picker">{tags.length ? tags.map((item) => <button type="button" key={item.id} className={data.tag_ids.includes(item.id) ? "selected" : ""} onClick={() => toggleTag(item.id)}>{item.name}<i>{data.tag_ids.includes(item.id) ? "✓" : "+"}</i></button>) : <em>暂无可用标签</em>}</div><div className="taxonomy-create-row"><input value={newTag} onChange={(e) => setNewTag(e.target.value)} onKeyDown={(e) => e.key === "Enter" && void createTag()} placeholder="新增标签" aria-label="新增标签名称" /><button type="button" onClick={() => void createTag()} disabled={!newTag.trim() || creatingTaxonomy === "tag"}>＋ 添加标签</button></div></div>
          </section>
          <section className="editor-side-card editor-article-options-card">
            <div className="editor-side-heading"><h2>文章选项</h2><span>OPTIONS</span></div>
            <label className="editor-toggle"><input type="checkbox" checked={data.is_pinned} onChange={(e) => update("is_pinned", e.target.checked)} /><span><b>置顶文章</b><small>在首页优先展示</small></span><i /></label>
            <label className="editor-toggle"><input type="checkbox" checked={data.allow_comment} onChange={(e) => update("allow_comment", e.target.checked)} /><span><b>允许评论</b><small>读者可以在文章下留言</small></span><i /></label>
            <label className="editor-toggle"><input type="checkbox" checked={data.kanban_ref} onChange={(e) => update("kanban_ref", e.target.checked)} /><span><b>看板娘参与</b><small>在文章页显示相关对话</small></span><i /></label>
          </section>
          <section className="editor-side-card editor-cover-card">
            <div className="editor-side-heading"><h2>封面素材</h2><span>OPTIONAL</span></div>
            <div className="editor-cover-picker-row">{cover ? <div className="editor-cover-preview"><Image src={cover.file.url} width={180} height={100} unoptimized alt={cover.name} /><button type="button" onClick={() => update("cover_asset_id", null)}>移除</button></div> : <div className="editor-cover-empty"><span>▧</span><p>还没有选择封面</p></div>}<button type="button" className="editor-cover-select" onClick={() => setCoverPickerOpen(true)}>从素材库选择</button></div>
            <div className="editor-cover-note"><span>▧</span><p>封面会显示在首页文章卡片。支持在弹窗中预览并选择图片素材。</p></div>
          </section>
          <div className="editor-tip"><span>✦</span><div><b>写作小贴士</b><p>先写标题和摘要，再放心地进入正文。草稿随时可以保存，发布前记得打开预览检查排版。</p></div></div>
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

function CommentManager() {
  return <>
    <AdminTitle title="评论审核" sub="COMMENTS · POWERED BY ARTALK" action={<a className="admin-primary" href="/artalk/" target="_blank" rel="noreferrer">打开评论控制台 ↗</a>} />
    <section className="admin-panel comment-admin-card">
      <div className="comment-admin-mark" aria-hidden="true">A</div>
      <div>
        <h2>Artalk 评论控制台</h2>
        <p>评论、嵌套回复、待审队列、垃圾内容、用户和页面统计统一由 Artalk 管理。控制台使用独立的评论管理员账户。</p>
        <p>评论管理员由部署环境中的 <code>ARTALK_ADMIN_NAME</code>、<code>ARTALK_ADMIN_EMAIL</code> 和 <code>ARTALK_ADMIN_PASSWORD</code> 配置，请使用这组独立凭据登录。</p>
      </div>
    </section>
  </>;
}

function MediaSettings({ notify }: { notify: Notify }) {
  const [tracks, setTracks] = useState(["THIS ILLUSION", "to the beginning", "oath sign", "花の唄"]);
  const [preview, setPreview] = useState("");
  const move = (index: number, delta: number) => { const next = index + delta; if (next < 0 || next >= tracks.length) return; setTracks((items) => { const copy = [...items]; [copy[index], copy[next]] = [copy[next], copy[index]]; return copy; }); };
  const voices = [
    { id: "day-intro", kind: "day", name: "日间 Saber · 登录前", file: "blue-saber.mp3 · 固定资源" },
    { id: "day-success", kind: "day", name: "日间 Saber · 契约成立", file: "blue-saber-success.mp3 · 固定资源" },
    { id: "night-intro", kind: "night", name: "夜间 Alter · 登录前", file: "alter-saber.mp3 · 固定资源" },
    { id: "night-success", kind: "night", name: "夜间 Alter · 契约成立", file: "alter-saber-success.mp3 · 固定资源" },
  ];
  return <><AdminTitle title="音乐与语音" sub={`AUDIO LIBRARY · ${tracks.length} BGM · 4 LOCKED VOICES`} action={<button className="admin-primary" onClick={() => notify("BGM 上传入口已打开（Mock）")}>＋ 上传 BGM</button>} /><div className="settings-grid media-settings"><section className="admin-panel"><h2>BGM 播放列表</h2>{tracks.map((t, i) => <div className="track" key={t}><span>0{i + 1}</span><div><b>{t}</b><small>Fate Series · Audio Track</small></div><button onClick={() => move(i, -1)} disabled={i === 0}>↑</button><button onClick={() => move(i, 1)} disabled={i === tracks.length - 1}>↓</button><button onClick={() => { setTracks((items) => items.filter((item) => item !== t)); notify(`已移除 ${t}`, "danger"); }}>×</button></div>)}</section><section className="admin-panel voice-cards"><h2>登录语音 <small>FIXED · MINIO</small></h2>{voices.map(({ id, kind, name, file }) => <div key={id}><i className={kind} /><b>{name}</b><span>{file}</span><button className={preview === id ? "active" : ""} onClick={() => { setPreview(preview === id ? "" : id); notify(preview === id ? "试听已暂停" : `正在试听 ${name}`); }}>{preview === id ? "Ⅱ 暂停" : "▶ 试听"}</button></div>)}</section></div></>;
}
