"use client";

import Link from "next/link";
import Image from "next/image";
import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { usePathname } from "next/navigation";

type Theme = "day" | "night";
type Notify = (message: string, tone?: "normal" | "success" | "danger") => void;

const posts = [
  { slug: "blog-rebuild", category: "技术", date: "2026-07-12", title: "重构博客的一些思考", excerpt: "从单薄的功能到一个完整的个人站点：开屏动画、日夜双主题、追番页与时间轴，这次重构想把「我喜欢的东西」都装进来。", time: "8 min", words: "3.2k 字", comments: 12, pinned: true },
  { slug: "spring-anime-2026", category: "追番", date: "2026-07-02", title: "2026 春季番剧总结：这季度我推的都完结了", excerpt: "照例的季度总结，聊聊这个季度追完的六部番，以及为什么我又把 Fate/Zero 重刷了一遍。", time: "6 min", words: "2.4k 字", comments: 8 },
  { slug: "wallpaper-engine", category: "折腾", date: "2026-06-20", title: "把 Wallpaper Engine 的动态壁纸搬到网页开屏", excerpt: "mp4 和 pkg 格式的壁纸怎么提取、压缩、再做成博客的开屏动画，顺便记录踩过的坑。", time: "10 min", words: "4.1k 字", comments: 23 },
  { slug: "june-notes", category: "生活", date: "2026-06-08", title: "六月随笔：梅雨、键盘和一杯冰美式", excerpt: "没什么主题的一篇，写写最近的生活。", time: "4 min", words: "1.6k 字", comments: 5 },
  { slug: "live2d-llm", category: "技术", date: "2026-05-24", title: "让看板娘真正听懂你：Live2D 与 LLM 的一次握手", excerpt: "从动作映射到上下文注入，记录一套不打扰阅读的角色交互方案。", time: "12 min", words: "5.2k 字", comments: 17 },
  { slug: "keyboard-notes", category: "生活", date: "2026-05-12", title: "深夜键盘声：一把客制化键盘的诞生", excerpt: "轴体、定位板与声音包的取舍，以及我为什么最终选了偏安静的配置。", time: "7 min", words: "2.8k 字", comments: 9 },
  { slug: "samurai-remnant", category: "游戏", date: "2026-04-28", title: "Fate/Samurai Remnant 二周目杂感", excerpt: "当结局已经知道，第二次走进江户反而看见了更多角色之间的微妙距离。", time: "9 min", words: "3.7k 字", comments: 14 },
  { slug: "vite-migration", category: "折腾", date: "2026-04-15", title: "把旧博客迁到 Vite：一次克制的性能重构", excerpt: "不追逐漂亮的跑分，只处理真正影响阅读体验的加载、缓存与切页问题。", time: "11 min", words: "4.6k 字", comments: 20 },
  { slug: "fate-zero-rewatch", category: "追番", date: "2026-03-30", title: "第七次重看 Fate/Zero，我开始理解切嗣了", excerpt: "同一部作品在不同年龄重看，会得到完全不同的答案。", time: "8 min", words: "3.4k 字", comments: 31 },
  { slug: "spring-walk", category: "生活", date: "2026-03-18", title: "春日散步：胶片、河堤与晚风", excerpt: "带着相机漫无目的地走了一下午，顺便重新认识了居住多年的城市。", time: "5 min", words: "1.9k 字", comments: 6 },
];

type Post = (typeof posts)[number];

const articleDetails: Record<string, { first: string; firstBody: string; second: string; secondBody: string; quote: string }> = {
  技术: {
    first: "问题与取舍",
    firstBody: "真正影响体验的往往不是功能数量，而是加载、层级和反馈是否足够清楚。这次记录会把方案拆开，说明哪些部分值得做，哪些复杂度应该留到以后。",
    second: "实现与复盘",
    secondBody: "实现过程中优先保证内容可读、交互可预期，再逐步加入主题、动效和细节。技术选择只是手段，最终仍要回到读者能否顺畅理解内容。",
    quote: "先把正确的反馈放在正确的位置，再考虑让它变得华丽。",
  },
  折腾: {
    first: "从想法到可用",
    firstBody: "这类折腾最有趣的部分，是把一个看似只适合桌面的效果变成稳定的网页体验。过程里需要同时照顾资源体积、兼容性和降级方案。",
    second: "踩坑与边界",
    secondBody: "不是每个效果都值得原样搬到浏览器。删掉高成本、低收益的部分，保留最能传达气氛的细节，反而更接近最终想要的体验。",
    quote: "折腾的终点不是复杂，而是让复杂在使用时消失。",
  },
  追番: {
    first: "这一季留下了什么",
    firstBody: "比起罗列剧情，我更想记录角色、音乐和某些瞬间为什么会在完结后继续留在记忆里。评分只是索引，感受才是正文。",
    second: "值得再次回看的片段",
    secondBody: "有些作品第一次看的是情节，重看时才会注意到人物之间没有说出口的距离。它们也是我愿意持续做追番记录的原因。",
    quote: "好的故事结束之后，仍会在观众心里继续生长。",
  },
  生活: {
    first: "最近的日常",
    firstBody: "这些片段没有明确的主题：天气、键盘声、路上的光，或者一杯慢慢融化的冰美式。写下来，是为了不让它们轻易从记忆里消失。",
    second: "留给之后的自己",
    secondBody: "日常文章不需要总结出结论。能在很久以后重新读到当时的气味和心情，就已经足够。",
    quote: "把普通的一天认真记住，本身就是一件浪漫的事。",
  },
  游戏: {
    first: "第二次进入这个世界",
    firstBody: "当结局已经知道，注意力会从任务目标转向人物、环境和那些容易错过的支线。第二次体验往往比第一次更接近作品真正想说的内容。",
    second: "系统之外的感受",
    secondBody: "数值和机制决定是否好玩，角色与氛围决定一段旅程能否被记住。这篇记录更关心后者。",
    quote: "通关不是结束，而是终于看清这段旅程的开始。",
  },
};

const navItems = [
  ["/", "首页"], ["/archives", "归档"], ["/moments", "时间轴"], ["/anime", "追番"], ["/about", "关于"], ["/friends", "友链"],
];

function cx(...items: Array<string | false | undefined>) { return items.filter(Boolean).join(" "); }

export function BlogApp() {
  const pathname = usePathname() || "/";
  const [theme, setTheme] = useState<Theme>("day");
  const [menuOpen, setMenuOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [playerOpen, setPlayerOpen] = useState(true);
  const [themeTransition, setThemeTransition] = useState(false);
  const [toast, setToast] = useState<{ message: string; tone: "normal" | "success" | "danger" } | null>(null);
  const [easterEgg, setEasterEgg] = useState(false);
  const toastTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const themeInitialized = useRef(false);

  useEffect(() => {
    const saved = localStorage.getItem("helt-theme") as Theme | null;
    const nextTheme = saved === "day" || saved === "night"
      ? saved
      : window.matchMedia("(prefers-color-scheme: dark)").matches ? "night" : "day";
    document.documentElement.dataset.theme = nextTheme;
    window.requestAnimationFrame(() => {
      themeInitialized.current = true;
      setTheme(nextTheme);
    });
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    if (themeInitialized.current) localStorage.setItem("helt-theme", theme);
  }, [theme]);

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
    window.setTimeout(() => setTheme((value) => value === "day" ? "night" : "day"), 310);
    window.setTimeout(() => setThemeTransition(false), 760);
  };
  const common = { theme, toggleTheme, pathname, menuOpen, setMenuOpen, searchOpen, setSearchOpen };

  if (pathname.startsWith("/admin")) return <><AdminRouter pathname={pathname} theme={theme} toggleTheme={toggleTheme} notify={notify} />{themeTransition && <ThemeBlade />}{toast && <Toast {...toast} />}</>;

  let page: React.ReactNode;
  if (pathname === "/") page = <HomePage theme={theme} toggleTheme={toggleTheme} notify={notify} onSearch={() => setSearchOpen(true)} />;
  else if (pathname.startsWith("/posts/")) {
    const slug = pathname.split("/").filter(Boolean)[1];
    const post = posts.find((item) => item.slug === slug);
    page = post ? <ArticlePage post={post} notify={notify} /> : <NotFound />;
  }
  else if (pathname === "/archives") page = <ArchivesPage />;
  else if (pathname === "/moments") page = <MomentsPage notify={notify} />;
  else if (pathname === "/anime") page = <AnimePage />;
  else if (pathname === "/about") page = <AboutPage notify={notify} />;
  else if (pathname === "/friends") page = <FriendsPage notify={notify} />;
  else page = <NotFound />;

  return (
    <div className="site-shell">
      {pathname !== "/" && <TopNav {...common} />}
      {page}
      {pathname !== "/" && <Footer />}
      <FloatingTools theme={theme} toggleTheme={toggleTheme} playerOpen={playerOpen} setPlayerOpen={setPlayerOpen} notify={notify} />
      {searchOpen && <SearchOverlay onClose={() => setSearchOpen(false)} />}
      {themeTransition && <ThemeBlade />}
      {toast && <Toast {...toast} />}
      {easterEgg && <EasterEgg theme={theme} onClose={() => setEasterEgg(false)} />}
    </div>
  );
}

function ThemeBlade() { return <div className="theme-blade" aria-hidden="true"><i /><i /><i /></div>; }

function Toast({ message, tone }: { message: string; tone: "normal" | "success" | "danger" }) {
  return <div className={cx("toast", `toast-${tone}`)} role="status"><span>{tone === "success" ? "✓" : tone === "danger" ? "!" : "◆"}</span>{message}</div>;
}

function EasterEgg({ theme, onClose }: { theme: Theme; onClose: () => void }) {
  useEffect(() => {
    const close = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [onClose]);
  return <div className="easter-egg" role="dialog" aria-modal="true" aria-label="隐藏彩蛋" onClick={onClose}><div onClick={(e) => e.stopPropagation()}><Image src={theme === "day" ? "/saber-day.png" : "/saber-night.png"} width={5120} height={2160} sizes="(max-width: 760px) 100vw, 760px" alt="" /><div className="dialog-box"><b>{theme === "day" ? "Saber" : "Alter"}</b><p>能抵达这里，说明你的意志相当坚定，Master。今晚的晚餐，就由胜者决定吧。</p></div><button onClick={onClose}>收起令咒</button></div></div>;
}

function ThemeSwitch({ theme, onClick, compact = false }: { theme: Theme; onClick: () => void; compact?: boolean }) {
  return (
    <button className={cx("theme-switch", compact && "compact")} onClick={onClick} aria-label={`切换到${theme === "day" ? "夜间" : "日间"}主题`} aria-pressed={theme === "night"}>
      <span className="theme-label active">{theme === "day" ? "☀ SABER" : "☾ ALTER"}</span>
      <span className="switch-track"><i /></span>
      {!compact && <span className="theme-label muted">{theme === "day" ? "☾ ALTER" : "☀ SABER"}</span>}
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
  const [page, setPage] = useState(1);
  const [menuOpen, setMenuOpen] = useState(false);
  const [navElevated, setNavElevated] = useState(false);
  const [voicePlaying, setVoicePlaying] = useState(false);
  const [voiceSeconds, setVoiceSeconds] = useState(0);
  const pageCount = Math.ceil(posts.length / 4);
  const visiblePosts = posts.slice((page - 1) * 4, page * 4);
  const enter = () => document.getElementById("articles")?.scrollIntoView({ behavior: "smooth" });
  useEffect(() => {
    const updateNav = () => setNavElevated(window.scrollY > 56);
    updateNav();
    window.addEventListener("scroll", updateNav, { passive: true });
    return () => window.removeEventListener("scroll", updateNav);
  }, []);
  useEffect(() => {
    if (!voicePlaying) return;
    const timer = window.setInterval(() => setVoiceSeconds((value) => {
      if (value >= 11) { setVoicePlaying(false); notify("语音播放完成", "success"); return 0; }
      return value + 1;
    }), 1000);
    return () => window.clearInterval(timer);
  }, [voicePlaying, notify]);
  const changePage = (next: number) => {
    const target = Math.min(pageCount, Math.max(1, next));
    if (target === page) return;
    setPage(target);
    document.querySelector(".section-intro")?.scrollIntoView({ behavior: "smooth", block: "start" });
  };
  return (
    <>
      <TopNav pathname="/" theme={theme} toggleTheme={toggleTheme} menuOpen={menuOpen} setMenuOpen={setMenuOpen} searchOpen={false} setSearchOpen={() => onSearch()} floating elevated={navElevated} />
      <section className="hero">
        <div className="hero-stripe stripe-one" /><div className="hero-stripe stripe-two" />
        <Image className="hero-art" src={theme === "day" ? "/saber-day.png" : "/saber-night.png"} width={5120} height={2160} sizes="(max-width: 768px) 100vw, 64vw" priority alt={theme === "day" ? "Saber" : "Saber Alter"} />
        <div className="hero-copy">
          <div className="eyebrow"><i /> SINCE 2020 · HELT&apos;S BLOG</div>
          <h1>「問おう。<br />貴方が私の<span>マスター</span>か？」</h1>
          <p>—— 我问你，你就是我的 Master 吗？</p>
          <div className="hero-actions">
            <button className={cx("voice-button", voicePlaying && "is-playing")} aria-pressed={voicePlaying} onClick={() => { setVoicePlaying((value) => !value); notify(voicePlaying ? "语音已暂停" : "开始播放开屏语音"); }}><b>{voicePlaying ? "Ⅱ" : "▶"}</b><span className="wave"><i /><i /><i /><i /><i /><i /></span><span>{voicePlaying ? `播放中 00:${String(voiceSeconds).padStart(2, "0")} / 00:12` : `音声を再生 · ${theme === "day" ? "川澄綾子" : "Alter"}`}</span></button>
            <button className="primary-button" onClick={enter}>ENTER · 进入博客 ▾</button>
          </div>
        </div>
        <div className="dialog-box hero-dialog"><b>{theme === "day" ? "Saber" : "Alter"}</b><p>{theme === "day" ? "今日もいい天気ですね。最近更新：《重构博客的一些思考》 · 2026-07-12" : "夜已深，Master。仍要继续前行吗？最近更新：《重构博客的一些思考》"}</p><span>▼</span></div>
        <button className="scroll-cue" onClick={enter}>SCROLL ▼</button>
      </section>
      <section id="articles" className="home-content">
        <Stats />
        <div className="section-intro reveal"><div><span>RECENT WRITING</span><h2>最近写下的东西</h2></div><p>技术、生活与热爱的作品。每一页都尽量留下真实的温度。</p></div>
        <div className="post-list">
          <div className="post-page" key={page}>{visiblePosts.map((post) => <PostCard key={post.slug} post={post} />)}</div>
          <div className="pagination" aria-label="文章分页"><button onClick={() => changePage(page - 1)} disabled={page === 1} aria-label="上一页">◀</button>{Array.from({ length: pageCount }, (_, i) => i + 1).map((item) => <button key={item} className={page === item ? "current" : ""} onClick={() => changePage(item)} aria-current={page === item ? "page" : undefined}>{item}</button>)}<button onClick={() => changePage(page + 1)} disabled={page === pageCount} aria-label="下一页">▶</button></div>
        </div>
      </section>
    </>
  );
}

function Stats() {
  return <div className="stats">{[["128", "文章"], ["42.6w", "总字数"], ["187,203", "访客"], ["2,048", "运行天数"]].map(([n, l]) => <div key={l}><b>{n}</b><span>{l}</span></div>)}</div>;
}

function PostCard({ post }: { post: typeof posts[number] }) {
  return (
    <Link href={`/posts/${post.slug}`} className={cx("post-card", post.pinned && "pinned")}>
      {post.pinned && <span className="pin">置顶 PINNED</span>}
      <div className="post-main"><div className="post-meta"><span className={`tag tag-${post.category}`}>{post.category}</span><span>{post.date}</span></div><h2>{post.title}</h2><p>{post.excerpt}</p><div className="post-stats"><span>{post.time}</span><span>{post.words}</span><span>评论 {post.comments}</span></div></div>
      {post.pinned && <Image src="/saber-day.png" width={5120} height={2160} sizes="240px" alt="文章封面" />}
    </Link>
  );
}

function PageHeading({ title, subtitle }: { title: string; subtitle: string }) { return <div className="page-heading"><h1>{title}</h1><span>{subtitle}</span></div>; }

function ArticlePage({ post, notify }: { post: Post; notify: Notify }) {
  const [progress, setProgress] = useState(0);
  const [liked, setLiked] = useState(false);
  const detail = articleDetails[post.category] ?? articleDetails.技术;
  const currentIndex = posts.findIndex((item) => item.slug === post.slug);
  const previousPost = currentIndex < posts.length - 1 ? posts[currentIndex + 1] : null;
  const nextPost = currentIndex > 0 ? posts[currentIndex - 1] : null;
  const related = posts.filter((item) => item.slug !== post.slug && item.category === post.category).slice(0, 2);
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
  return (
    <><div className="article-reading-bar"><i style={{ width: `${progress}%` }} /></div><main className="page-wrap article-layout page-enter">
      <article className="article-card">
        <div className="breadcrumbs"><Link href="/">首页</Link> / <Link href="/archives">{post.category}</Link></div>
        <h1>{post.title}</h1>
        <div className="article-meta">{post.date} · {post.words} · {post.time} · 阅读 {1_284 - currentIndex * 73}</div>
        <p>{post.excerpt}</p>
        <SectionTitle id="first-section" index="一" title={detail.first} />
        <p>{detail.firstBody}</p>
        <div className="dialog-box quote"><b>{post.category === "追番" || post.category === "游戏" ? "helt" : "Saber"}</b><p>「{detail.quote}」</p></div>
        {post.slug === "blog-rebuild" && <Image className="article-image" src="/saber-day.png" width={5120} height={2160} sizes="(max-width: 768px) 100vw, 680px" alt="Saber 日间主题封面" />}
        <SectionTitle id="second-section" index="二" title={detail.second} />
        <p>{detail.secondBody}</p>
        {(post.category === "技术" || post.category === "折腾") && <pre><code>{`:root {\n  --accent: #2b5fb8;\n  --gold: #b99a3e;\n}`}</code></pre>}
        <SectionTitle id="closing" index="三" title="写在最后" />
        <p>界面可以有鲜明的性格，但每一次动作都应当回答用户的期待。装饰服务于方向感，动画服务于因果关系。</p>
        <div className="article-actions"><button className={liked ? "liked" : ""} onClick={() => { setLiked(!liked); notify(liked ? "已取消喜欢" : "感谢你的喜欢", "success"); }}>{liked ? "♥ 已喜欢 · 129" : "♡ 喜欢 · 128"}</button><button onClick={shareArticle}>⌁ 分享文章</button></div>
        <div className="article-nav">{previousPost ? <Link href={`/posts/${previousPost.slug}`}>← 上一篇<br /><b>{previousPost.title}</b></Link> : <span />}{nextPost ? <Link href={`/posts/${nextPost.slug}`}>下一篇 →<br /><b>{nextPost.title}</b></Link> : <span />}</div>
        <Comments count={post.comments} notify={notify} />
      </article>
      <aside className="article-aside"><div className="toc"><b>目录 CONTENTS <small>{Math.round(progress)}%</small></b><Link href="#first-section" className="active">一、{detail.first}</Link><Link href="#second-section">二、{detail.second}</Link><Link href="#closing">三、写在最后</Link></div><div className="recommend"><b>相关文章</b>{related.length ? related.map((item) => <Link key={item.slug} href={`/posts/${item.slug}`}>{item.title}</Link>) : <Link href="/archives">浏览全部文章</Link>}</div></aside>
    </main></>
  );
}

function SectionTitle({ index, title, id }: { index: string; title: string; id?: string }) { return <h2 id={id} className="section-title"><i />{index}、{title}</h2>; }

function Comments({ count, notify }: { count: number; notify: Notify }) {
  const [sent, setSent] = useState(false);
  const [replyTo, setReplyTo] = useState("");
  return <section className="comments"><h2>评论 · {count}</h2><div className="comment"><span>凛</span><div><b>Rin <small>· 3 小时前</small></b><p>开屏语音这个想法太棒了，期待夜间 Alter 的低音版本（笑）。</p><button className="text-action" onClick={() => setReplyTo("@Rin ")}>回复</button></div></div><div className="comment reply"><span>h</span><div><b>helt <em>博主</em> <small>· 2 小时前</small></b><p>@Rin 已经在录了，音量会默认静音防止吓到人（认真）。</p><button className="text-action" onClick={() => setReplyTo("@helt ")}>回复</button></div></div><form className="comment-form" onSubmit={(e) => { e.preventDefault(); setSent(true); setReplyTo(""); notify("评论已进入审核队列", "success"); }}><b>留下你的回应</b><div><input aria-label="昵称" placeholder="昵称" required /><input aria-label="邮箱" placeholder="邮箱" type="email" required /></div><textarea aria-label="评论内容" key={replyTo} defaultValue={replyTo} placeholder="写下想说的话……" required /><button className="primary-button">发送评论</button>{sent && <span className="success" role="status">已进入审核队列（Mock）</span>}</form></section>;
}

function ArchivesPage() {
  const [view, setView] = useState("time");
  const [selected, setSelected] = useState("");
  const choices = view === "category" ? ["技术 45", "生活 32", "折腾 24", "追番 18", "游戏 9"] : ["React", "TypeScript", "Fate", "Live2D", "日常", "Vite", "动画", "游戏"];
  const matchedPosts = selected ? posts.filter((post) => post.category === selected || `${post.title} ${post.excerpt}`.includes(selected)) : [];
  return <main className="page-wrap page-enter"><PageHeading title="归档" subtitle="ARCHIVE · 128 篇" /><div className="view-tabs" role="tablist" aria-label="归档浏览方式"><button role="tab" aria-selected={view === "time"} className={view === "time" ? "active" : ""} onClick={() => { setView("time"); setSelected(""); }}>时间</button><button role="tab" aria-selected={view === "category"} className={view === "category" ? "active" : ""} onClick={() => { setView("category"); setSelected(""); }}>分类</button><button role="tab" aria-selected={view === "tag"} className={view === "tag" ? "active" : ""} onClick={() => { setView("tag"); setSelected(""); }}>标签</button></div>{view === "time" ? <div className="archive-grid"><div className="timeline-list"><h2>2026</h2>{posts.map((p) => <Link key={p.slug} href={`/posts/${p.slug}`}><time>{p.date.slice(5)}</time><b>{p.title}</b><span className={`tag tag-${p.category}`}>{p.category}</span></Link>)}</div><aside className="archive-side"><b>分类</b>{[["技术", 45], ["生活", 32], ["折腾", 24], ["追番", 18], ["游戏", 9]].map(([n, c]) => <button key={n} onClick={() => { setView("category"); setSelected(String(n)); }}><span>{n}</span><span>{c}</span></button>)}</aside></div> : <><div className="cloud-panel">{choices.map((item, i) => <button className={selected === item.split(" ")[0] ? "active" : ""} key={item} style={{ fontSize: `${14 + (i % 4) * 4}px` }} onClick={() => setSelected(item.split(" ")[0])}>{item}</button>)}</div>{selected && <div className="archive-selection"><span>正在浏览</span><b>{selected}</b><p>共找到 {matchedPosts.length} 篇相关内容</p><button onClick={() => setSelected("")}>清除筛选 ×</button></div>}{selected && <div className="archive-results">{matchedPosts.length ? matchedPosts.map((post) => <Link key={post.slug} href={`/posts/${post.slug}`}><span className={`tag tag-${post.category}`}>{post.category}</span><b>{post.title}</b><small>{post.date}</small></Link>) : <div className="empty-panel">当前演示内容中没有匹配文章。</div>}</div>}</>}</main>;
}

function MomentsPage({ notify }: { notify: Notify }) {
  const [liked, setLiked] = useState<number[]>([]);
  const moments = [{ date: "07.21", text: "新博客的开屏动画调了一晚上，语音淡入的时机终于对了。就是这个感觉。", mood: "开发日志" }, { date: "07.14", text: "周末去了漫展，战利品合影。", mood: "日常" }, { date: "07.02", text: "博客运行满 2000 天了。谢谢每一个来过的人。", mood: "纪念" }];
  return <main className="page-wrap narrow page-enter"><PageHeading title="时间轴" subtitle="MOMENTS · 碎碎念" /><div className="moments">{moments.map((m, i) => <article key={m.date}><div className="moment-date"><b>{m.date}</b><span>2026</span></div><div className="moment-card"><span className="tag">{m.mood}</span><p>{m.text}</p>{i === 1 && <div className="photo-placeholder"><span>COMIC MARKET</span><b>MEMORY / 07.14</b></div>}<div className="moment-actions"><button className={liked.includes(i) ? "liked" : ""} onClick={() => setLiked((items) => items.includes(i) ? items.filter((x) => x !== i) : [...items, i])}>{liked.includes(i) ? "♥" : "♡"} {18 - i * 3 + (liked.includes(i) ? 1 : 0)}</button><button onClick={() => notify(`已展开 ${m.date} 的 ${4 - i} 条 Mock 评论`)}>评论 {4 - i}</button></div></div></article>)}</div></main>;
}

function AnimePage() {
  const [tab, setTab] = useState("anime");
  const [selected, setSelected] = useState<string | null>(null);
  const anime = ["Fate/strange Fake", "葬送的芙莉莲 第二季", "赛马娘 灰发灰姑娘", "胆大党 第二季"];
  const games = ["Fate/Samurai Remnant", "艾尔登法环：黑夜君临", "死亡搁浅 2", "明日方舟：终末地"];
  const items = tab === "anime" ? anime : games;
  useEffect(() => {
    if (!selected) return;
    const close = (event: KeyboardEvent) => event.key === "Escape" && setSelected(null);
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [selected]);
  return <main className="page-wrap page-enter"><PageHeading title="追番 · 游戏" subtitle="在看 4 · 看完 87 · 在玩 2" /><div className="view-tabs" role="tablist" aria-label="记录类型"><button role="tab" aria-selected={tab === "anime"} className={tab === "anime" ? "active" : ""} onClick={() => { setTab("anime"); setSelected(null); }}>追番记录</button><button role="tab" aria-selected={tab === "games"} className={tab === "games" ? "active" : ""} onClick={() => { setTab("games"); setSelected(null); }}>游戏记录</button></div><div className="media-grid">{items.map((name, i) => <button className="media-card" key={name} onClick={() => setSelected(name)}><span className={`media-cover cover-${i}`}><span>{tab === "anime" ? "ANIME" : "GAME"}</span></span><span className="media-copy"><span className="status">{i < 2 ? "● 进行中" : "✓ 已完成"}</span><h2>{name}</h2><p>{i % 2 ? "画面、音乐与叙事都相当惊喜，值得慢慢体验。" : "本季度最期待的作品，角色塑造依旧让人着迷。"}</p><span className="score">{["★★★★★", "★★★★☆", "★★★★★", "★★★★☆"][i]}</span><small>点击查看记录 →</small></span></button>)}</div>{selected && <div className="detail-overlay" role="dialog" aria-modal="true" aria-labelledby="media-detail-title" onClick={() => setSelected(null)}><article onClick={(e) => e.stopPropagation()}><button className="close-detail" aria-label="关闭详情" onClick={() => setSelected(null)}>×</button><span className="eyebrow"><i /> {tab === "anime" ? "WATCH LOG" : "PLAY LOG"}</span><h2 id="media-detail-title">{selected}</h2><div className="score">★★★★★</div><p>这里保留了观看进度、短评和最喜欢的片段。相比把所有信息都堆在卡片上，详情在需要时再展开。</p><dl><div><dt>状态</dt><dd>进行中</dd></div><div><dt>进度</dt><dd>{tab === "anime" ? "08 / 12 话" : "36 小时"}</dd></div><div><dt>最近更新</dt><dd>2026.07.20</dd></div></dl></article></div>}</main>;
}

function AboutPage({ notify }: { notify: Notify }) {
  const copyEmail = async () => {
    try {
      if (!navigator.clipboard) throw new Error("Clipboard API unavailable");
      await navigator.clipboard.writeText("hello@helt.example.com");
      notify("邮箱已复制", "success");
    } catch {
      notify("邮箱：hello@helt.example.com", "normal");
    }
  };
  return <main className="page-wrap about-layout page-enter"><aside className="profile-card"><div className="avatar">h</div><h1>helt.</h1><p>写代码 / 追番 / 折腾博客<br />骑士王的头号 Master</p><div className="profile-stats"><span><b>128</b>文章</span><span><b>2,048</b>天</span></div><div className="socials"><button aria-label="GitHub" onClick={() => notify("GitHub 主页为演示链接")}>GH</button><button aria-label="哔哩哔哩" onClick={() => notify("哔哩哔哩主页为演示链接")}>BL</button><button aria-label="复制联系邮箱" onClick={copyEmail}>✉</button></div></aside><div className="about-content"><PageHeading title="关于我" subtitle="ABOUT · MASTER PROFILE" /><div className="dialog-box"><b>helt</b><p>你好，欢迎来到我的小站。白天是普通的程序员，晚上是熬夜追番的 Master。这个博客从 2020 年写到现在，记录技术、生活，和那些让我热血的作品。</p></div><SectionTitle index="01" title="技能与兴趣" /><div className="skill-grid">{["React / TypeScript", "Node.js", "UI Engineering", "Fate Series", "摄影与键盘", "动画与游戏"].map((s) => <span key={s}>{s}</span>)}</div><SectionTitle index="02" title="关于本站" /><p>本站从设计到代码都在持续重构中。按下 Konami 秘技（↑↑↓↓←→←→BA）会触发隐藏彩蛋；日夜切换时，Saber 与 Alter 的视觉也会一起切换。</p></div></main>;
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

function NotFound() { return <main className="empty-state"><b>404</b><h1>前方并非约定之地</h1><p>Master，这条路径似乎不存在。</p><Link href="/" className="primary-button">返回首页</Link></main>; }

function SearchOverlay({ onClose }: { onClose: () => void }) {
  const [query, setQuery] = useState("");
  const result = useMemo(() => posts.filter((p) => `${p.title} ${p.category} ${p.excerpt}`.toLowerCase().includes(query.toLowerCase())), [query]);
  useEffect(() => { const close = (e: KeyboardEvent) => e.key === "Escape" && onClose(); window.addEventListener("keydown", close); return () => window.removeEventListener("keydown", close); }, [onClose]);
  return <div className="search-overlay" role="dialog" aria-modal="true" aria-label="搜索文章" onClick={onClose}><div className="search-panel" onClick={(e) => e.stopPropagation()}><div><span aria-hidden="true">⌕</span><input aria-label="搜索关键词" autoFocus value={query} onChange={(e) => setQuery(e.target.value)} placeholder="搜索标题、分类或关键词…" /><button onClick={onClose} aria-label="关闭搜索">×</button></div>{query ? <section aria-live="polite">{result.length ? result.slice(0, 6).map((p) => <Link key={p.slug} href={`/posts/${p.slug}`}><span className="tag">{p.category}</span><b>{p.title}</b><small>{p.date}</small></Link>) : <p>没有找到相关内容，换个关键词试试。</p>}</section> : <div className="search-suggestions"><small>热门关键词</small><div>{["React", "Fate", "Live2D", "博客重构"].map((item) => <button key={item} onClick={() => setQuery(item)}>{item}</button>)}</div></div>}<small>ESC 关闭 · 输入关键词实时检索 Mock 数据</small></div></div>;
}

function FloatingTools({ theme, toggleTheme, playerOpen, setPlayerOpen, notify }: { theme: Theme; toggleTheme: () => void; playerOpen: boolean; setPlayerOpen: (v: boolean) => void; notify: Notify }) {
  const tracks = [{ title: "THIS ILLUSION", artist: "LiSA · Fate OST" }, { title: "to the beginning", artist: "Kalafina" }, { title: "花の唄", artist: "Aimer" }];
  const [track, setTrack] = useState(0);
  const [playing, setPlaying] = useState(false);
  const [showTop, setShowTop] = useState(false);
  const [chatOpen, setChatOpen] = useState(false);
  const [chatText, setChatText] = useState("");
  const [chatReply, setChatReply] = useState("Master，需要我为你介绍今天的内容吗？");
  useEffect(() => { const watch = () => setShowTop(window.scrollY > 500); watch(); window.addEventListener("scroll", watch, { passive: true }); return () => window.removeEventListener("scroll", watch); }, []);
  const moveTrack = (delta: number) => { const next = (track + delta + tracks.length) % tracks.length; setTrack(next); setPlaying(true); notify(`正在播放：${tracks[next].title}`); };
  const sendChat = (text = chatText) => { if (!text.trim()) return; setChatReply("我明白了，Master。这个演示暂时由 Mock 数据回应，但交互链路已经完整。"); setChatText(""); };
  return <><div className={cx("music-player", !playerOpen && "collapsed", playing && "is-playing")}><button className="disc" onClick={() => playerOpen ? setPlaying(!playing) : setPlayerOpen(true)} aria-label={playerOpen ? (playing ? "暂停音乐" : "播放音乐") : "展开播放器"}><i /></button>{playerOpen && <><div><b>{tracks[track].title}</b><span>{tracks[track].artist}</span><i className="progress"><i /></i></div><button onClick={() => moveTrack(-1)} aria-label="上一首">⏮</button><button onClick={() => { setPlaying(!playing); notify(playing ? "音乐已暂停" : `正在播放：${tracks[track].title}`); }} aria-label={playing ? "暂停" : "播放"}>{playing ? "Ⅱ" : "▶"}</button><button onClick={() => moveTrack(1)} aria-label="下一首">⏭</button><button className="collapse-player" onClick={() => setPlayerOpen(false)} aria-label="收起播放器">−</button></>}</div>{chatOpen && <div className="kanban-chat" role="dialog" aria-label="看板娘对话"><button className="close-chat" aria-label="关闭看板娘对话" onClick={() => setChatOpen(false)}>×</button><div className="kanban-name">SABER · GUIDE</div><p>{chatReply}</p><div className="quick-replies"><button onClick={() => sendChat("推荐文章")}>推荐文章</button><button onClick={() => sendChat("怎么切换主题")}>主题说明</button></div><form onSubmit={(e) => { e.preventDefault(); sendChat(); }}><input aria-label="发送给看板娘的消息" value={chatText} onChange={(e) => setChatText(e.target.value)} placeholder="问问看板娘…" /><button>发送</button></form></div>}<div className="floating-tools">{showTop && <button className="back-top" onClick={() => window.scrollTo({ top: 0, behavior: "smooth" })} aria-label="返回顶部">▲</button>}<button className={chatOpen ? "active" : ""} onClick={() => setChatOpen(!chatOpen)} aria-label={chatOpen ? "关闭看板娘" : "打开看板娘"} aria-expanded={chatOpen}>♙</button><button className="floating-theme" onClick={toggleTheme} aria-label={`切换到${theme === "day" ? "夜间" : "日间"}主题`}>{theme === "day" ? "☾" : "☀"}</button></div></>;
}

function Footer() { return <footer><Link href="/" className="brand">helt<span>.</span></Link><p>写代码、追番、折腾博客的个人小站。</p><span>© 2020—2026 helt. · POWERED BY REACT</span></footer>; }

function AdminRouter({ pathname, theme, toggleTheme, notify }: { pathname: string; theme: Theme; toggleTheme: () => void; notify: Notify }) {
  if (pathname === "/admin/login") return <AdminLogin />;
  return <AdminSessionGate><AdminLayout pathname={pathname} theme={theme} toggleTheme={toggleTheme} notify={notify} /></AdminSessionGate>;
}

type ApiErrorPayload = { error?: { code?: string; message?: string }; message?: string };
type PasskeyRequestOptions = {
  challenge: string;
  rp_id?: string;
  timeout?: number;
  user_verification?: UserVerificationRequirement;
  allow_credentials?: Array<{ id: string; type?: PublicKeyCredentialType; transports?: AuthenticatorTransport[] }>;
};

async function responseMessage(response: Response, fallback: string) {
  const payload = await response.json().catch(() => null) as ApiErrorPayload | null;
  return payload?.error?.message || payload?.message || fallback;
}

function isJsonResponse(response: Response) {
  return response.headers.get("content-type")?.includes("application/json") ?? false;
}

function decodeBase64Url(value: string) {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, "=");
  const binary = atob(padded);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0)).buffer;
}

function encodeBase64Url(value: ArrayBuffer) {
  const bytes = new Uint8Array(value);
  let binary = "";
  bytes.forEach((byte) => { binary += String.fromCharCode(byte); });
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function AdminSessionGate({ children }: { children: React.ReactNode }) {
  const [authenticated, setAuthenticated] = useState(false);

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
          setAuthenticated(true);
          return;
        }
        const refreshed = await fetch("/api/v1/admin/auth/refresh", {
          method: "POST",
          credentials: "include",
          headers: { accept: "application/json" },
          signal: controller.signal,
        });
        if (refreshed.ok && isJsonResponse(refreshed)) {
          setAuthenticated(true);
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

  if (!authenticated) {
    return <main className="admin-session-loading" role="status"><span aria-hidden="true">◆</span><p>正在验证令咒…</p></main>;
  }
  return children;
}

const loginScenes = {
  day: {
    Japanese: "問おう。貴方が私のマスターか？",
    Chinese: "试问。你是我的御主吗？",
    voice: "/storage/voice/login/blue-saber.mp3",
  },
  night: {
    Japanese: "召喚に応じ参上した。貴様が私のマスターという奴か？",
    Chinese: "应召唤前来。你这家伙就是我的御主吗？",
    voice: "/storage/voice/login/alter-saber.mp3",
  },
} as const satisfies Record<Theme, {
  Japanese: string;
  Chinese: string;
  voice: string;
}>;

function loginThemeForCurrentTime(date = new Date()): Theme {
  const hour = date.getHours();
  return hour >= 7 && hour < 19 ? "day" : "night";
}

function AdminLogin() {
  const [show, setShow] = useState(false);
  const [username, setUsername] = useState("helt");
  const [password, setPassword] = useState("");
  const [remember, setRemember] = useState(true);
  const [busy, setBusy] = useState<"password" | "passkey" | "forgot" | null>(null);
  const [feedback, setFeedback] = useState<{ tone: "error" | "success"; message: string } | null>(null);
  const [loginTheme, setLoginTheme] = useState<Theme>(() => loginThemeForCurrentTime());
  const [voicePlaying, setVoicePlaying] = useState(false);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const scene = loginScenes[loginTheme];

  useEffect(() => {
    const audio = document.querySelector<HTMLAudioElement>("#admin-login-voice");
    if (!audio) return;
    const syncVoiceState = () => setVoicePlaying(!audio.paused && !audio.ended);
    const frame = window.requestAnimationFrame(syncVoiceState);
    audio.addEventListener("play", syncVoiceState);
    audio.addEventListener("pause", syncVoiceState);
    audio.addEventListener("ended", syncVoiceState);
    audio.addEventListener("error", syncVoiceState);
    return () => {
      window.cancelAnimationFrame(frame);
      audio.removeEventListener("play", syncVoiceState);
      audio.removeEventListener("pause", syncVoiceState);
      audio.removeEventListener("ended", syncVoiceState);
      audio.removeEventListener("error", syncVoiceState);
    };
  }, []);

  const stopLoginVoice = () => {
    const audio = audioRef.current;
    if (audio) {
      audio.pause();
      audio.currentTime = 0;
    }
    setVoicePlaying(false);
  };

  const playLoginVoice = async (audio = audioRef.current) => {
    if (!audio) return;
    audio.currentTime = 0;
    try {
      await audio.play();
      setVoicePlaying(true);
    } catch {
      setVoicePlaying(false);
    }
  };

  const toggleLoginTheme = () => {
    const nextTheme = loginTheme === "day" ? "night" : "day";
    stopLoginVoice();
    setLoginTheme(nextTheme);
    const audio = audioRef.current;
    if (audio) {
      audio.src = loginScenes[nextTheme].voice;
      audio.load();
      void playLoginVoice(audio);
    }
  };

  const toggleVoice = async () => {
    const audio = audioRef.current;
    if (!audio) return;
    if (voicePlaying || !audio.paused) {
      stopLoginVoice();
      return;
    }
    await playLoginVoice(audio);
  };

  const submitLogin = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!username.trim() || !password) {
      setFeedback({ tone: "error", message: "请输入账号和密码。" });
      return;
    }
    setBusy("password");
    setFeedback(null);
    try {
      const response = await fetch("/api/v1/admin/auth/login", {
        method: "POST",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ username: username.trim(), password, remember }),
      });
      if (!response.ok) {
        throw new Error(await responseMessage(response, "认证失败，请稍后重试。"));
      }
      if (!isJsonResponse(response)) {
        throw new Error("认证接口尚未连接，请确认本地后端正在运行。");
      }
      setFeedback({ tone: "success", message: "认证通过，正在进入控制室…" });
      window.setTimeout(() => { window.location.href = "/admin"; }, 260);
    } catch (error) {
      setFeedback({ tone: "error", message: error instanceof Error ? error.message : "认证失败，请稍后重试。" });
    } finally {
      setBusy(null);
    }
  };

  const requestPasswordReset = async () => {
    if (!username.trim()) {
      setFeedback({ tone: "error", message: "请先填写需要重置的账号。" });
      return;
    }
    setBusy("forgot");
    setFeedback(null);
    try {
      const response = await fetch("/api/v1/admin/auth/forgot-password", {
        method: "POST",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ username: username.trim() }),
      });
      const message = await responseMessage(response, "请联系服务器管理员重置密码。");
      if (!response.ok) throw new Error(message);
      if (!isJsonResponse(response)) throw new Error("密码重置接口尚未连接。");
      setFeedback({ tone: "success", message });
    } catch (error) {
      setFeedback({ tone: "error", message: error instanceof Error ? error.message : "暂时无法提交重置请求。" });
    } finally {
      setBusy(null);
    }
  };

  const loginWithPasskey = async () => {
    if (!window.PublicKeyCredential || !navigator.credentials) {
      setFeedback({ tone: "error", message: "当前浏览器不支持通行密钥 Passkey。" });
      return;
    }
    setBusy("passkey");
    setFeedback(null);
    try {
      const optionsResponse = await fetch("/api/v1/admin/auth/passkey/options", {
        method: "POST",
        credentials: "include",
        headers: { "content-type": "application/json" },
      });
      if (!optionsResponse.ok) {
        const payload = await optionsResponse.json().catch(() => null) as ApiErrorPayload | null;
        if (payload?.error?.code === "not_implemented") throw new Error("通行密钥尚未配置，请使用账号密码登录。");
        throw new Error(payload?.error?.message || "无法获取通行密钥认证请求。");
      }
      if (!isJsonResponse(optionsResponse)) throw new Error("通行密钥接口尚未连接。");
      const options = await optionsResponse.json() as PasskeyRequestOptions;
      const credential = await navigator.credentials.get({
        publicKey: {
          challenge: decodeBase64Url(options.challenge),
          rpId: options.rp_id,
          timeout: options.timeout ?? 60_000,
          userVerification: options.user_verification ?? "preferred",
          allowCredentials: options.allow_credentials?.map((item) => ({
            id: decodeBase64Url(item.id),
            type: item.type ?? "public-key",
            transports: item.transports,
          })),
        },
      }) as PublicKeyCredential | null;
      if (!credential) throw new Error("未选择通行密钥。");
      const assertion = credential.response as AuthenticatorAssertionResponse;
      const verifyResponse = await fetch("/api/v1/admin/auth/passkey/verify", {
        method: "POST",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          id: credential.id,
          raw_id: encodeBase64Url(credential.rawId),
          type: credential.type,
          response: {
            authenticator_data: encodeBase64Url(assertion.authenticatorData),
            client_data_json: encodeBase64Url(assertion.clientDataJSON),
            signature: encodeBase64Url(assertion.signature),
            user_handle: assertion.userHandle ? encodeBase64Url(assertion.userHandle) : null,
          },
        }),
      });
      if (!verifyResponse.ok) throw new Error(await responseMessage(verifyResponse, "通行密钥认证失败。"));
      setFeedback({ tone: "success", message: "通行密钥认证通过，正在进入控制室…" });
      window.setTimeout(() => { window.location.href = "/admin"; }, 260);
    } catch (error) {
      const cancelled = error instanceof DOMException && error.name === "NotAllowedError";
      setFeedback({ tone: "error", message: cancelled ? "通行密钥认证已取消。" : error instanceof Error ? error.message : "通行密钥认证失败。" });
    } finally {
      setBusy(null);
    }
  };

  return (
    <main className={`admin-login login-theme-${loginTheme}`}>
      <div className="admin-login-cover login-cover-day" aria-hidden="true" />
      <div className="admin-login-cover login-cover-night" aria-hidden="true" />
      <div className="admin-login-shade" aria-hidden="true" />
      <button className="login-theme-switch" type="button" onClick={toggleLoginTheme} aria-label={`切换至${loginTheme === "day" ? "夜间" : "日间"}灵衣`}>
        <i aria-hidden="true" />
        <span>灵衣切换</span>
      </button>
      <form onSubmit={submitLogin} aria-busy={busy !== null}>
        <span className="auth-tag">契约仪式</span>
        <div className="login-brand">
          <Link href="/" className="brand" aria-label="返回 helt 博客首页">helt<span>.</span> <small>ADMIN</small></Link>
        </div>
        <label htmlFor="admin-username">账号
          <input id="admin-username" name="username" autoComplete="username" value={username} onChange={(event) => setUsername(event.target.value)} disabled={busy !== null} />
        </label>
        <label htmlFor="admin-password">密码
          <span className="password-control">
            <input id="admin-password" name="password" autoComplete="current-password" type={show ? "text" : "password"} value={password} onChange={(event) => setPassword(event.target.value)} disabled={busy !== null} autoFocus />
            <button type="button" aria-label={show ? "隐藏密码" : "显示密码"} aria-pressed={show} onClick={() => setShow(!show)} disabled={busy !== null}>{show ? "◉" : "◎"}</button>
          </span>
        </label>
        <div className="login-options">
          <label className="remember">
            <input type="checkbox" checked={remember} onChange={(event) => setRemember(event.target.checked)} disabled={busy !== null} />
            <span aria-hidden="true">{remember ? "✓" : ""}</span>
            七日内免认证
          </label>
          <button className="forgot-link" type="button" onClick={requestPasswordReset} disabled={busy !== null}>{busy === "forgot" ? "提交中…" : "忘记密码？"}</button>
        </div>
        {feedback && <div className={`login-feedback ${feedback.tone}`} role={feedback.tone === "error" ? "alert" : "status"}><span aria-hidden="true">{feedback.tone === "error" ? "!" : "✓"}</span>{feedback.message}</div>}
        <button className="login-submit" disabled={busy !== null}>{busy === "password" ? "仪 式 进 行 中…" : "契 约 · 成 立"}</button>
        <div className="login-divider" aria-hidden="true"><span /></div>
        <button className="passkey-button" type="button" onClick={loginWithPasskey} disabled={busy !== null}>{busy === "passkey" ? "正在唤起通行密钥…" : "通行密钥 Passkey 登录"}</button>
      </form>
      <section className="login-scene-copy" aria-live="polite">
        <blockquote>
          <p lang="ja">{scene.Japanese}</p>
          <p lang="zh-CN">{scene.Chinese}</p>
        </blockquote>
        <div className="login-scene-actions">
          <button
            className={`login-voice-button${voicePlaying ? " is-playing" : ""}`}
            type="button"
            onClick={toggleVoice}
            aria-pressed={voicePlaying}
          >
            <span className="login-voice-glyph" aria-hidden="true"><i /><i /><i /><i /></span>
            {voicePlaying ? "停止播放" : "语音放送"}
          </button>
        </div>
        <audio
          id="admin-login-voice"
          ref={audioRef}
          src={scene.voice}
          autoPlay
          preload="auto"
        />
      </section>
    </main>
  );
}

function AdminLogoutButton() {
  const [busy, setBusy] = useState(false);
  const logout = async () => {
    if (busy) return;
    setBusy(true);
    try {
      await fetch("/api/v1/admin/auth/logout", { method: "POST", credentials: "include" });
    } finally {
      window.location.replace("/admin/login");
    }
  };
  return <button className="admin-logout" type="button" title="退出登录" aria-label="退出登录" onClick={logout} disabled={busy}>{busy ? "…" : "↪"}</button>;
}

const adminNav = [["/admin", "▦", "仪表盘"], ["/admin/articles", "▤", "文章管理"], ["/admin/articles/new", "✎", "撰写文章"], ["/admin/comments", "◫", "评论审核"], ["/admin/kanban", "♙", "看板娘 · LLM"], ["/admin/media", "♫", "音乐与语音"], ["/admin/appearance", "◐", "封面与主题"], ["/admin/settings", "⚙", "站点设置"]];

function AdminLayout({ pathname, theme, toggleTheme, notify }: { pathname: string; theme: Theme; toggleTheme: () => void; notify: Notify }) {
  const [commandOpen, setCommandOpen] = useState(false);
  const current = adminNav.find(([href]) => pathname === href)?.[2] || (pathname.includes("articles") ? "文章编辑器" : "仪表盘");
  let content: React.ReactNode;
  if (pathname === "/admin") content = <Dashboard notify={notify} />;
  else if (pathname === "/admin/articles") content = <ArticleManager notify={notify} />;
  else if (pathname.includes("/admin/articles/")) content = <ArticleEditor notify={notify} />;
  else if (pathname === "/admin/comments") content = <CommentManager notify={notify} />;
  else if (pathname === "/admin/kanban") content = <KanbanSettings notify={notify} />;
  else if (pathname === "/admin/media") content = <MediaSettings notify={notify} />;
  else if (pathname === "/admin/appearance") content = <AppearanceSettings notify={notify} />;
  else content = <SiteSettings notify={notify} />;
  useEffect(() => {
    if (!commandOpen) return;
    const close = (event: KeyboardEvent) => event.key === "Escape" && setCommandOpen(false);
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [commandOpen]);
  return <div className="admin-shell"><aside className="admin-sidebar"><Link href="/" className="brand">helt<span>.</span> <small>ADMIN</small></Link><nav aria-label="后台主导航">{adminNav.map(([href, icon, label]) => <Link key={href} href={href} aria-current={pathname === href ? "page" : undefined} className={pathname === href ? "active" : ""}><i>{icon}</i>{label}</Link>)}</nav><div className="admin-user"><span>h</span><div><b>helt</b><small>Administrator</small></div><AdminLogoutButton /></div></aside><main className="admin-main"><header><div><span>CONTROL ROOM /</span><b>{current}</b></div><div><ThemeSwitch theme={theme} onClick={toggleTheme} compact /><button onClick={() => setCommandOpen(true)} aria-label="打开快捷导航" aria-expanded={commandOpen}>⌕</button><span className="admin-avatar">h</span></div></header><nav className="admin-mobile-nav" aria-label="后台移动导航">{adminNav.map(([href, icon, label]) => <Link key={href} href={href} aria-current={pathname === href ? "page" : undefined} className={pathname === href ? "active" : ""}><i>{icon}</i><span>{label}</span></Link>)}</nav><div className="admin-content page-enter">{content}</div></main>{commandOpen && <div className="admin-command" role="dialog" aria-modal="true" aria-label="快速导航" onClick={() => setCommandOpen(false)}><div onClick={(e) => e.stopPropagation()}><header><b>快速前往</b><button aria-label="关闭快捷导航" onClick={() => setCommandOpen(false)}>×</button></header>{adminNav.map(([href, icon, label]) => <Link key={href} href={href}><i>{icon}</i><span>{label}</span><small>→</small></Link>)}</div></div>}</div>;
}

function AdminTitle({ title, sub, action }: { title: string; sub: string; action?: React.ReactNode }) { return <div className="admin-title"><div><h1>{title}</h1><p>{sub}</p></div>{action}</div>; }

function Dashboard({ notify }: { notify: Notify }) {
  return <><AdminTitle title="仪表盘" sub="WELCOME BACK, MASTER · 2026.07.23" action={<Link href="/admin/articles/new" className="admin-primary">＋ 撰写新文章</Link>} /><div className="admin-stats">{[["128", "文章总数", "+3 本月"], ["187,203", "累计访客", "+12.4%"], ["1,842", "评论总数", "17 待审核"], ["2,048", "运行天数", "99.98%"]].map(([n, l, d]) => <article key={l}><span>{l}</span><b>{n}</b><small>{d}</small></article>)}</div><div className="dashboard-grid"><section className="admin-panel"><h2>访问趋势 <small>LAST 14 DAYS</small></h2><div className="chart">{[35, 52, 43, 66, 58, 78, 72, 88, 60, 82, 76, 94, 86, 100].map((n, i) => <i key={i} style={{ height: `${n}%` }} />)}</div></section><section className="admin-panel recent-comments"><h2>最新评论 <Link href="/admin/comments">全部 →</Link></h2>{["Rin", "Aki", "Kumo"].map((n, i) => <div key={n}><span>{n[0]}</span><p><b>{n} · {i + 1} 小时前</b>开屏语音这个想法太棒了，期待夜间 Alter…</p><span className="mini-actions"><button onClick={() => notify(`已通过 ${n} 的评论`, "success")}>通过</button><button onClick={() => notify(`已拒绝 ${n} 的评论`, "danger")}>拒绝</button><button onClick={() => notify(`正在回复 ${n}`)}>回复</button></span></div>)}</section></div><section className="admin-panel quick"><h2>快速操作</h2><div><Link href="/admin/articles/new">✎<span>新建文章</span></Link><Link href="/admin/comments">◫<span>审核评论</span></Link><Link href="/admin/appearance">◐<span>切换封面</span></Link><Link href="/admin/settings">⚙<span>站点设置</span></Link></div></section></>;
}

function ArticleManager({ notify }: { notify: Notify }) {
  const [filter, setFilter] = useState("全部");
  const [query, setQuery] = useState("");
  const [rows, setRows] = useState(() => [...posts, { ...posts[0], slug: "llm-live2d-draft", title: "看板娘接入 LLM 实战：从 Live2D 到 Claude", category: "技术", date: "2026-07-16", comments: 0, pinned: false }]);
  const visible = rows.filter((row, index) => row.title.toLowerCase().includes(query.toLowerCase()) && (filter === "全部" || filter === "草稿" && row.slug.endsWith("draft") || filter === "置顶" && index === 0 || filter === "已发布" && !row.slug.endsWith("draft")));
  return <><AdminTitle title="文章管理" sub={`ARTICLES · ${visible.length} 条结果`} action={<Link href="/admin/articles/new" className="admin-primary">＋ 新建文章</Link>} /><div className="admin-toolbar"><div>{["全部", "已发布", "草稿", "置顶"].map((x) => <button key={x} className={filter === x ? "active" : ""} onClick={() => setFilter(x)}>{x}</button>)}</div><input aria-label="搜索文章标题" value={query} onChange={(e) => setQuery(e.target.value)} placeholder="搜索标题…" /></div><div className="admin-table"><div className="table-head"><span>选择</span><span>标题</span><span>分类</span><span>状态</span><span>数据</span><span>日期</span><span>操作</span></div>{visible.map((p) => { const originalIndex = rows.findIndex((row) => row.slug === p.slug); const draft = p.slug.endsWith("draft"); return <div className="table-row" key={p.slug}><span><input type="checkbox" aria-label={`选择 ${p.title}`} /></span><b>{originalIndex === 0 && <em>置顶</em>}{p.title}</b><span className="tag">{p.category}</span><span className={draft ? "draft" : "published"}>{draft ? "◐ 草稿" : "● 已发布"}</span><small>{draft ? "—" : `${1200 - originalIndex * 75} 阅 · ${p.comments} 评`}</small><small>{p.date.slice(5)}</small><span className="row-actions"><Link href={`/admin/articles/${p.slug}/edit`}>编辑</Link><Link href={`/posts/${p.slug}`}>预览</Link><button onClick={() => { setRows((items) => items.filter((row) => row.slug !== p.slug)); notify(`已删除《${p.title}》`, "danger"); }}>删除</button></span></div>; })}{!visible.length && <div className="empty-panel">没有符合当前筛选的文章。</div>}</div></>;
}

function ArticleEditor({ notify }: { notify: Notify }) {
  const [preview, setPreview] = useState(false);
  const [body, setBody] = useState("# 重构博客的一些思考\n\n旧博客最大的问题是「功能单薄」……\n\n## 一、开屏与主题系统\n\n日间是蓝 Saber，夜间切换为 Alter。\n\n> [Saber]\n> 主题切换不是换一层皮，而是换一个人格。");
  const insert = (snippet: string) => setBody((value) => `${value}\n\n${snippet}`);
  const tools = [["B", "**粗体文字**"], ["I", "*斜体文字*"], ["H", "## 新标题"], ["❝", "> [Saber]\n> 对话内容"], ["⌁", "[链接文字](https://)"], ["</>", "```ts\n// code\n```"], ["▧", "![图片说明](/image.png)"]];
  return <><AdminTitle title="文章编辑器" sub="EDITOR · 内容变更会保存在本地 Mock 状态" action={<div className="editor-actions"><button onClick={() => notify("草稿已保存", "success")}>保存草稿</button><button className="admin-primary" onClick={() => notify("文章已发布（Mock）", "success")}>发布文章</button></div>} /><div className="editor-meta"><input defaultValue="重构博客的一些思考" /><select defaultValue="技术"><option>技术</option><option>生活</option><option>追番</option></select><input placeholder="标签：React, UI" defaultValue="React, UI, 博客" /></div><div className="editor-toolbar">{tools.map(([label, snippet]) => <button key={label} onClick={() => insert(snippet)} title={`插入 ${label}`}>{label}</button>)}<span /><button className={preview ? "active" : ""} onClick={() => setPreview(!preview)}>{preview ? "关闭预览" : "分屏预览"}</button></div><div className={cx("editor-area", preview && "split")}><textarea value={body} onChange={(e) => setBody(e.target.value)} />{preview && <article><h1>重构博客的一些思考</h1><p>旧博客最大的问题是「功能单薄」……</p><h2>一、开屏与主题系统</h2><p>日间是蓝 Saber，夜间切换为 Alter。</p><div className="dialog-box"><b>Saber</b><p>主题切换不是换一层皮，而是换一个人格。</p></div></article>}</div></>;
}

function CommentManager({ notify }: { notify: Notify }) {
  const [done, setDone] = useState<string[]>([]);
  const items = [{ n: "Rin", text: "开屏语音这个想法太棒了，期待夜间 Alter 的低音版本（笑）。", post: "重构博客的一些思考" }, { n: "Kumo", text: "pkg 解包那段能再详细一点吗？我卡在 RePKG 提取贴图这步了。", post: "把 Wallpaper Engine 的动态壁纸搬到网页开屏" }, { n: "Aki", text: "新的移动端布局读起来很舒服！", post: "重构博客的一些思考" }];
  const handle = (name: string, action: string) => { setDone((itemsDone) => [...itemsDone, name]); notify(`已${action} ${name} 的评论`, action === "拒绝" ? "danger" : "success"); };
  return <><AdminTitle title="评论审核" sub={`COMMENTS · ${items.length - done.length} 待处理`} /><div className="moderation-list">{items.filter((x) => !done.includes(x.n)).map((x) => <article key={x.n}><span>{x.n[0]}</span><div><b>{x.n} <small>· 刚刚</small></b><p>{x.text}</p><small>评论于：<span>《{x.post}》</span></small></div><div><button onClick={() => handle(x.n, "通过")}>✓ 通过</button><button onClick={() => handle(x.n, "拒绝")}>× 拒绝</button><button onClick={() => notify(`已打开对 ${x.n} 的回复框`)}>↩ 回复</button></div></article>)}{done.length === items.length && <div className="empty-panel"><b>✓</b><p>全部评论已处理完毕。</p><button onClick={() => setDone([])}>恢复演示数据</button></div>}</div></>;
}

function KanbanSettings({ notify }: { notify: Notify }) {
  const [temperature, setTemperature] = useState(7);
  const [testing, setTesting] = useState(false);
  const test = () => { setTesting(true); window.setTimeout(() => { setTesting(false); notify("模型连通测试完成", "success"); }, 900); };
  return <><AdminTitle title="看板娘 · LLM" sub="KANBAN GIRL · MODEL CONFIGURATION" action={<button className="admin-primary" onClick={() => notify("看板娘配置已保存", "success")}>保存配置</button>} /><div className="settings-grid"><section className="admin-panel"><h2>模型连接 <span className="live-dot">ONLINE</span></h2><label>服务商<select><option>OpenAI Compatible</option><option>Anthropic</option></select></label><label>模型<input defaultValue="gpt-4.1-mini" /></label><label>API 地址<input defaultValue="https://api.example.com/v1" /></label><label>Temperature <b>{temperature / 10}</b><input type="range" min="0" max="10" value={temperature} onChange={(e) => setTemperature(Number(e.target.value))} /></label></section><section className="admin-panel"><h2>人格提示词</h2><textarea defaultValue="你是 helt 博客的看板娘，人格原型为骑士王。称呼访客为「Master」，语气端正温和、略带古风骑士腔，偶尔提到晚餐。回答不超过三句话……" /><button className={testing ? "loading" : ""} onClick={test} disabled={testing}>{testing ? "正在召唤…" : "测试对话"}</button><div className={cx("test-chat", testing && "thinking")}><b>Saber</b><p>{testing ? "正在读取当前文章上下文……" : "Master，今日更新了《重构博客的一些思考》。关于开屏与主题系统的部分，我认为值得一读。"}</p></div></section></div></>;
}

function MediaSettings({ notify }: { notify: Notify }) {
  const [tracks, setTracks] = useState(["THIS ILLUSION", "to the beginning", "oath sign", "花の唄"]);
  const [preview, setPreview] = useState("");
  const move = (index: number, delta: number) => { const next = index + delta; if (next < 0 || next >= tracks.length) return; setTracks((items) => { const copy = [...items]; [copy[index], copy[next]] = [copy[next], copy[index]]; return copy; }); };
  return <><AdminTitle title="音乐与语音" sub={`AUDIO LIBRARY · ${tracks.length} BGM · 2 LOCKED VOICES`} action={<button className="admin-primary" onClick={() => notify("BGM 上传入口已打开（Mock）")}>＋ 上传 BGM</button>} /><div className="settings-grid media-settings"><section className="admin-panel"><h2>BGM 播放列表</h2>{tracks.map((t, i) => <div className="track" key={t}><span>0{i + 1}</span><div><b>{t}</b><small>Fate Series · Audio Track</small></div><button onClick={() => move(i, -1)} disabled={i === 0}>↑</button><button onClick={() => move(i, 1)} disabled={i === tracks.length - 1}>↓</button><button onClick={() => { setTracks((items) => items.filter((item) => item !== t)); notify(`已移除 ${t}`, "danger"); }}>×</button></div>)}</section><section className="admin-panel voice-cards"><h2>登录语音 <small>FIXED · MINIO</small></h2>{[["day", "日间 Saber", "blue-saber.mp3 · 固定资源"], ["night", "夜间 Alter", "alter-saber.mp3 · 固定资源"]].map(([kind, name, file]) => <div key={kind}><i className={kind} /><b>{name}</b><span>{file}</span><button className={preview === kind ? "active" : ""} onClick={() => { setPreview(preview === kind ? "" : kind); notify(preview === kind ? "试听已暂停" : `正在试听 ${name}`); }}>{preview === kind ? "Ⅱ 暂停" : "▶ 试听"}</button></div>)}</section></div></>;
}

function AppearanceSettings({ notify }: { notify: Notify }) {
  const [rule, setRule] = useState("system");
  return <><AdminTitle title="封面与主题" sub="APPEARANCE · DAY / NIGHT" action={<button className="admin-primary" onClick={() => notify("主题策略已保存并应用", "success")}>保存并应用</button>} /><div className="appearance-grid"><article><Image src="/saber-day.png" width={5120} height={2160} sizes="(max-width: 900px) 100vw, 45vw" alt="日间 Saber 封面" /><div><b>日间主题 · SABER</b><span>#2B5FB8 · #B99A3E</span><button onClick={() => notify("请选择新的日间封面文件（Mock）")}>更换封面</button></div></article><article><Image src="/saber-night.png" width={5120} height={2160} sizes="(max-width: 900px) 100vw, 45vw" alt="夜间 Alter 封面" /><div><b>夜间主题 · ALTER</b><span>#D84358 · #0E0B16</span><button onClick={() => notify("请选择新的夜间封面文件（Mock）")}>更换封面</button></div></article></div><section className="admin-panel schedule"><h2>主题切换规则 <small>当前：{rule === "system" ? "跟随系统" : rule === "schedule" ? "定时切换" : "始终日间"}</small></h2><label><input type="radio" name="theme-rule" checked={rule === "system"} onChange={() => setRule("system")} /> 跟随系统</label><label><input type="radio" name="theme-rule" checked={rule === "schedule"} onChange={() => setRule("schedule")} /> 定时切换 <input type="time" defaultValue="07:00" disabled={rule !== "schedule"} /> — <input type="time" defaultValue="19:00" disabled={rule !== "schedule"} /></label><label><input type="radio" name="theme-rule" checked={rule === "day"} onChange={() => setRule("day")} /> 始终使用日间主题</label></section></>;
}

function SiteSettings({ notify }: { notify: Notify }) {
  const [saved, setSaved] = useState(false);
  return <><AdminTitle title="站点设置" sub="SITE CONFIGURATION" action={<button className="admin-primary" onClick={() => { setSaved(true); notify("站点设置已保存", "success"); }}>保存设置</button>} />{saved && <div className="save-toast">✓ 设置已保存（Mock）</div>}<div className="settings-grid"><section className="admin-panel form-panel"><h2>基本信息</h2><label>站点名称<input defaultValue="helt." /></label><label>站点描述<textarea defaultValue="写代码、追番、折腾博客的个人小站。" /></label><label>站点地址<input defaultValue="https://helt.example.com" /></label><label>联系邮箱<input defaultValue="hello@helt.example.com" /></label></section><section className="admin-panel toggles"><h2>功能开关</h2>{[["开屏页", "关闭后直接进入文章流"], ["评论系统", "允许访客在文章下留言"], ["看板娘", "显示 Live2D 角色与对话"], ["背景音乐", "显示全局音乐播放器"], ["Konami 彩蛋", "启用键盘隐藏彩蛋"]].map(([a, b], i) => <div key={a}><span><b>{a}</b><small>{b}</small></span><label className="toggle"><input type="checkbox" defaultChecked={i !== 4} onChange={(e) => notify(`${a}已${e.target.checked ? "开启" : "关闭"}`)} /><i /></label></div>)}</section></div></>;
}
