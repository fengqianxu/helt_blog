import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";
import { buildArticleToc, getActiveTocId } from "../app/article-toc.mjs";
import { expectedTocItems, multiHeadingArticle } from "./fixtures/multi-heading-article.mjs";

const root = new URL("../", import.meta.url);

async function render(path = "/") {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("test", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);
  return worker.fetch(
    new Request(`http://localhost${path}`, { headers: { accept: "text/html" } }),
    { ASSETS: { fetch: async () => new Response("Not found", { status: 404 }) } },
    { waitUntil() {}, passThroughOnException() {} },
  );
}

function headingsFromMarkdown(markdown) {
  return markdown
    .split("\n")
    .map((line) => line.match(/^(#{2,4})\s+(.+)$/))
    .filter(Boolean)
    .map((match) => {
      const classes = new Set();
      return {
        tagName: `H${match[1].length}`,
        textContent: match[2],
        id: "",
        classList: { add: (name) => classes.add(name), contains: (name) => classes.has(name) },
      };
    });
}

test("builds and scroll-tracks the table of contents for a multi-heading article", async () => {
  const headings = headingsFromMarkdown(multiHeadingArticle.content);
  const items = buildArticleToc(headings);

  assert.deepEqual(items, expectedTocItems);
  assert.deepEqual(headings.map((heading) => heading.id), expectedTocItems.map((item) => item.id));
  assert.ok(headings.every((heading) => heading.classList.contains("article-heading")));

  const beforeFirstHeading = new Map(items.map((item, index) => [item.id, 240 + index * 180]));
  assert.equal(getActiveTocId(items, (id) => beforeFirstHeading.get(id)), items[0].id);

  const readingNestedSection = new Map(items.map((item, index) => [item.id, (index - 2) * 180 + 120]));
  assert.equal(getActiveTocId(items, (id) => readingNestedSection.get(id)), items[2].id);

  const atArticleEnd = new Map(items.map((item, index) => [item.id, -900 + index * 120]));
  assert.equal(getActiveTocId(items, (id) => atArticleEnd.get(id)), items.at(-1).id);

  const styles = await readFile(new URL("app/globals.css", root), "utf8");
  assert.match(styles, /\.article-aside\s*\{[^}]*position:\s*sticky[^}]*top:\s*94px/);
  assert.match(styles, /\.toc \.toc-link\.active\s*\{[^}]*border-left-color:\s*var\(--accent\)/);
  assert.match(styles, /@media \(max-width:\s*768px\)[\s\S]*?\.article-aside\s*\{\s*display:\s*none/);
});

test("server-renders the finished blog", async () => {
  const response = await render();
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type") ?? "", /^text\/html\b/i);
  const html = await response.text();
  assert.match(html, /<title>helt\. \| 写代码、追番、折腾博客<\/title>/i);
  assert.match(html, /貴方が私の/);
  assert.match(html, /saber-day\.png/);
  assert.doesNotMatch(html, /codex-preview|Your site is taking shape|react-loading-skeleton/i);
});

test("keeps project assets and API-backed front-end routes in place", async () => {
  const [shell, layout, login, account, assets, llm, raiments, siteSettings, playlistSettings, review, shared, mediaPage, animePage, gamesPage, mediaApi, mediaHooks, pagination, packageJson, viteConfig, styles] = await Promise.all([
    readFile(new URL("app/BlogApp.tsx", root), "utf8"),
    readFile(new URL("app/layout.tsx", root), "utf8"),
    readFile(new URL("app/admin/AdminLogin.tsx", root), "utf8"),
    readFile(new URL("app/admin/AdminAccountCenter.tsx", root), "utf8"),
    readFile(new URL("app/admin/AssetManager.tsx", root), "utf8"),
    readFile(new URL("app/admin/LlmSettings.tsx", root), "utf8"),
    readFile(new URL("app/admin/RaimentSettings.tsx", root), "utf8"),
    readFile(new URL("app/admin/SiteSettings.tsx", root), "utf8"),
    readFile(new URL("app/admin/PlaylistSettings.tsx", root), "utf8"),
    readFile(new URL("app/admin/ReviewManager.tsx", root), "utf8"),
    readFile(new URL("app/admin/shared.ts", root), "utf8"),
    readFile(new URL("app/media/MediaPage.tsx", root), "utf8"),
    readFile(new URL("app/media/AnimePage.tsx", root), "utf8"),
    readFile(new URL("app/media/GamesPage.tsx", root), "utf8"),
    readFile(new URL("app/media/api.ts", root), "utf8"),
    readFile(new URL("app/media/hooks.ts", root), "utf8"),
    readFile(new URL("app/media/Pagination.tsx", root), "utf8"),
    readFile(new URL("package.json", root), "utf8"),
    readFile(new URL("vite.config.ts", root), "utf8"),
    readFile(new URL("app/globals.css", root), "utf8"),
    access(new URL("public/saber-day.png", root)),
    access(new URL("public/saber-night.png", root)),
    access(new URL("public/og.png", root)),
  ]);
  const media = [mediaPage, animePage, gamesPage, mediaApi, mediaHooks, pagination].join("\n");
  const app = [shell, login, account, assets, llm, raiments, siteSettings, playlistSettings, review, shared, media].join("\n");
  assert.doesNotMatch(app, /const posts = \[/);
  assert.match(app, /\/api\/v1\/articles/);
  assert.match(app, /\/api\/v1\/admin\/articles/);
  assert.match(app, /\/api\/v1\/admin\/articles\/batch/);
  assert.match(shell, /aria-label="归档索引"/);
  assert.match(shell, /archive-category-index-title/);
  assert.match(shell, /archive-tag-index-title/);
  assert.match(shell, /className="archive-side-section"/);
  assert.match(shell, /toggleSelection\("category", item\.id\)/);
  assert.match(shell, /toggleSelection\("tag", item\.id\)/);
  assert.match(shell, /post\.category\?\.id === selectedChoice\.id/);
  assert.match(shell, /post\.tags\.some\(\(tag\) => tag\.id === selectedChoice\.id\)/);
  assert.doesNotMatch(shell, /aria-label="归档浏览方式"|archive-taxonomy|archive-category-grid|archive-tag-cloud/);
  assert.doesNotMatch(shell, /item\.split\(" "\)\[0\]/);
  assert.match(styles, /\.archive-side button\.active\s*\{[^}]*box-shadow:\s*inset 3px 0 var\(--taxonomy-color\)/);
  assert.match(styles, /\.archive-side button\.archive-side-tag\s*\{[^}]*grid-template-columns:\s*18px minmax\(0,1fr\)/);
  assert.match(styles, /\.archive-side\s*\{[^}]*flex-direction:\s*column[^}]*gap:\s*18px/);
  assert.match(styles, /\.archive-side-section\s*\{[^}]*background:\s*var\(--surface\)[^}]*border:\s*1px solid var\(--line\)/);
  assert.match(styles, /@media \(max-width:\s*768px\)[\s\S]*?\.archive-side\s*\{[^}]*position:\s*static[^}]*order:\s*-1/);
  assert.match(shell, /\/api\/v1\/raiments/);
  assert.match(layout, /process\.env\.PUBLIC_ORIGIN/);
  assert.doesNotMatch(layout, /x-forwarded-host|headers\(\)/);
  assert.match(layout, /helt-color-scheme/);
  assert.match(shell, /persistColorScheme/);
  assert.match(shell, /raimentCatalog\.order\.length < 2/);
  assert.match(raiments, /method: "POST"/);
  assert.match(raiments, /method: "DELETE"/);
  assert.match(raiments, /method: "DELETE"[\s\S]*?body: JSON\.stringify\(\{ revision: selected\.revision \}\)/);
  assert.match(raiments, /\/api\/v1\/admin\/raiments\/\$\{encodeURIComponent\(selected\.id\)\}/);
  assert.match(raiments, /revision:\s*selected\.revision/);
  assert.doesNotMatch(shell, /<HomePage key=\{activeRaimentId\}/);
  assert.match(raiments, /cover_asset_id:\s*selected\.cover_asset_id/);
  assert.doesNotMatch(raiments, /switch_at|启用时间|raiment-add-tab/);
  assert.doesNotMatch(raiments, /外观基调|明亮外观|深色外观|ID ·|BUILT-IN|CUSTOM PROFILE|☀|☾/);
  assert.doesNotMatch(siteSettings, /☀|☾/);
  assert.match(raiments, /inferColorScheme\(selected\.theme\.background/);
  assert.match(raiments, /cover_title:\s*selected\.cover_title/);
  assert.match(raiments, /cover_voice_asset_id:\s*selected\.cover_voice_asset_id/);
  assert.match(raiments, /fetchAllAssets\("raiment_cover"/);
  assert.match(raiments, /fetchAllAssets\("raiment_voice"/);
  assert.match(raiments, /items\.length >= payload\.total/);
  assert.match(raiments, /kanban_asset_id:\s*selected\.kanban_asset_id/);
  assert.match(siteSettings, /\/api\/v1\/admin\/site\/raiment-schedule/);
  assert.match(siteSettings, /\/api\/v1\/admin\/playlists/);
  assert.match(siteSettings, /playlist_id/);
  assert.match(siteSettings, /背景音乐/);
  assert.match(siteSettings, /type="time"/);
  assert.match(siteSettings, /新增时间段/);
  assert.doesNotMatch(raiments, /Mock|角色展示|默认招呼语/);
  assert.match(mediaApi, /const BANGUMI_PAGE_SIZE = 8/);
  assert.doesNotMatch(media, /animeStatus|追番状态筛选/);
  assert.match(pagination, /className="pagination bangumi-pagination"/);
  assert.match(mediaPage, /bangumi\.meta\.counts\.watching/);
  assert.match(animePage, /href=\{item\.url\} target="_blank" rel="noreferrer"/);
  assert.match(animePage, /前往 Bilibili 官网/);
  assert.doesNotMatch(media, /查看追番详情|BILIBILI WATCH LOG|className="detail-overlay"/);
  assert.match(animePage, /item\.status === "watching" \? "● 在看" : "◆ 看完"/);
  assert.match(animePage, /观看进度未公开/);
  assert.match(mediaPage, /role="tabpanel"/);
  assert.match(mediaPage, /\["ArrowLeft", "ArrowRight", "Home", "End"\]/);
  assert.doesNotMatch(media, /<dt>最近同步<\/dt>/);
  assert.doesNotMatch(media, /const anime = \["Fate\/strange Fake"/);
  assert.match(shell, /dynamic\(\(\) => import\("\.\/media\/MediaPage"\)/);
  assert.match(packageJson, /"artalk": "2\.10\.0"/);
  assert.match(shell, /Artalk\.init/);
  assert.match(shell, /Artalk\.loadCountWidget/);
  assert.doesNotMatch(shell, /\.comment_count/);
  assert.match(shell, /pageKey: articleCommentKey\(slug\)/);
  assert.match(shell, /payload\.allow_comment/);
  assert.match(shell, /网站（选填）/);
  assert.doesNotMatch(shell, /aria-label="关闭评论"|>展开评论<|comments-collapsed|setExpanded/);
  assert.match(shell, /aria-label="关闭文章详情"/);
  assert.match(shell, /className="article-close"/);
  assert.match(shell, /event\.target === event\.currentTarget && closeArticle\(\)/);
  assert.doesNotMatch(shell, /评论由 Artalk 提供；提交时会处理昵称、邮箱、IP 地址与浏览器信息/);
  assert.match(shell, /querySelectorAll<HTMLHeadingElement>\("h2, h3, h4"\)/);
  assert.match(shell, /data-toc-id/);
  assert.match(review, /\/api\/v1\/admin\/comments\?\$\{params\}/);
  assert.match(review, /fetch\(`\/api\/v1\/admin\/comments\/\$\{item\.id\}`/);
  assert.match(review, /updateCommentStatus\(item, "approved"\)/);
  assert.match(review, />删除评论<\/button>/);
  assert.doesNotMatch(review, /<iframe|\/artalk\/sidebar\/|>进入评论审核 →/);
  assert.doesNotMatch(review, /Artalk 评论控制台|ARTALK_ADMIN_NAME|ARTALK_ADMIN_EMAIL|ARTALK_ADMIN_PASSWORD|嵌套回复|待审队列/);
  assert.doesNotMatch(shell, /评论已进入审核队列（Mock）|恢复演示数据/);
  assert.match(shell, /\["\/admin\/comments",\s*"◫",\s*"审核"\]/);
  assert.doesNotMatch(shell, /评论审核/);
  assert.doesNotMatch(shell, /<h2>评论系统/);
  assert.match(shell, /审核系统/);
  assert.match(shell, /\/api\/v1\/admin\/comments\?status=pending&page=1&per_page=2/);
  assert.match(shell, /\/api\/v1\/admin\/friends\?status=pending&page=1&per_page=2/);
  assert.match(shell, /approveDashboardComment/);
  assert.match(review, /useSearchParams/);
  assert.match(review, /requestedSection === "friends" \? "friends" : "comments"/);
  assert.match(shell, /\/api\/v1\/friends\?per_page=50/);
  assert.match(shell, /contact_email/);
  assert.doesNotMatch(shell, /申请已提交（Mock）|这是 Mock 友链/);
  assert.match(review, /\/api\/v1\/admin\/friends/);
  assert.match(review, /usable_for=friend_avatar/);
  assert.match(review, /updateStatus\(item,\s*"approved"\)/);
  assert.match(review, /updateStatus\(item,\s*"rejected"\)/);
  assert.match(review, />友链申请</);
  assert.match(app, /\/api\/v1\/admin\/categories/);
  assert.match(app, /\/api\/v1\/admin\/tags/);
  assert.match(app, /function AdminLayout/);
  assert.match(shell, /\["\/admin\/llm",\s*"✦",\s*"LLM"\]/);
  assert.match(app, /\/api\/v1\/admin\/llm/);
  assert.match(app, /\/api\/v1\/admin\/llm\/connections/);
  assert.match(app, /\/api\/v1\/admin\/llm\/models/);
  assert.match(app, /\/api\/v1\/admin\/llm\/test/);
  assert.match(shell, /\/api\/v1\/admin\/llm\/polish/);
  assert.match(shell, /AI 润色/);
  assert.match(shell, /生成摘要候选/);
  assert.match(shell, /生成正文候选/);
  assert.match(shell, /还差一步：请选择模型/);
  assert.match(shell, /editor-ai-wait-mark/);
  assert.match(shell, /disabled=\{polishing\}/);
  assert.doesNotMatch(shell, /disabled=\{polishing \|\| !polishSource\.trim\(\)/);
  assert.match(shell, /不得超过 120 个字符/);
  assert.match(shell, /不要扩写成正文/);
  assert.match(shell, /DEFAULT_POLISH_PROMPTS/);
  assert.match(shell, /saveInFlightRef/);
  assert.match(shell, /data\.status === "published" \? "published" : "draft"/);
  assert.match(shell, /admin-article-pagination/);
  assert.match(shell, /for \(let page = 2; page <= pageCount; page \+= 1\)/);
  assert.match(shell, /应用合并结果/);
  assert.match(shell, /diffLines/);
  assert.match(shell, /buildEditorMergeRows/);
  assert.match(shell, /全部保留原文/);
  assert.match(shell, /全部采用润色/);
  assert.match(shell, /最终保留/);
  assert.match(shell, /choosePolishLine/);
  assert.match(shell, /createPortal/);
  assert.match(shell, /document\.body/);
  assert.match(shell, /localeCompare\(right\.id, "en"/);
  assert.match(styles, /editor-merge-grid[^}]*grid-template-columns:\s*minmax\(0, \.92fr\)[^}]*minmax\(0, 1\.16fr\)/);
  assert.match(styles, /editor-diff-modal > section[^}]*width:\s*100vw/);
  assert.match(styles, /editor-diff-modal[^}]*z-index:\s*10000/);
  assert.doesNotMatch(styles, /editor-diff-modal[^}]*backdrop-filter/);
  assert.match(styles, /editor-merge-cell\.changed\.selected/);
  assert.ok(shell.indexOf('editor-side-card editor-ai-card') > shell.indexOf('<aside className="editor-sidebar">'));
  assert.doesNotMatch(shell, /update\("content_md", payload/);
  assert.equal(JSON.parse(packageJson).dependencies.diff, "9.0.0");
  assert.match(llm, /已保存的 Key/);
  assert.match(llm, /测试并保存/);
  assert.match(llm, /<select value=\{useCase\.model\}/);
  assert.doesNotMatch(llm, /updateConnection\(connection\.id, "model"/);
  assert.match(app, /kanban_chat/);
  assert.doesNotMatch(app, /comment_review|评论预审/);
  assert.doesNotMatch(llm, /label: "文章助手"/);
  assert.doesNotMatch(app, /供应商、模型、密钥和提示词只在这里维护|llm-source-banner|llm-policy-panel|服务商/);
  assert.doesNotMatch(shell, /raiment-model-panel|人格提示词|api\.example\.com|ai-reference-note/);
  assert.match(shell, /<SiteBrand href="\/admin" suffix=\{<small>ADMIN<\/small>\} \/>/);
  assert.match(shell, /\/api\/v1\/site/);
  assert.match(shell, /site\.basic\.favicon_url/);
  assert.match(siteSettings, /站点 Logo/);
  assert.match(siteSettings, /浏览器图标/);
  assert.match(siteSettings, /\/api\/v1\/admin\/site\/settings/);
  assert.match(siteSettings, /function BrandingAssetPicker/);
  assert.match(siteSettings, /createPortal/);
  assert.match(siteSettings, /media_type: "image"/);
  assert.match(siteSettings, /从素材库选择/);
  assert.match(siteSettings, /新增素材请统一前往素材库上传/);
  assert.doesNotMatch(siteSettings, /上传新图片|type="file"|new FormData\(\)/);
  assert.doesNotMatch(siteSettings, /fetch\("\/api\/v1\/admin\/assets",\s*\{\s*method: "POST"/);
  assert.match(styles, /\.branding-asset-dialog \.branding-asset-search input:focus-visible\s*\{[^}]*outline:\s*0/);
  assert.doesNotMatch(shell, /\["\/admin\/articles\/new",\s*"✎",\s*"撰写文章"\]/);
  assert.doesNotMatch(shell, /AdminTitle title="仪表盘"[^>]*撰写新文章/);
  assert.match(shell, /className="admin-article-preview-overlay"/);
  assert.match(shell, /aria-label="关闭文章预览"/);
  assert.match(shell, /event\.target === event\.currentTarget && closePreview\(\)/);
  assert.match(shell, /fetch\(`\/api\/v1\/admin\/articles\/\$\{previewTarget\.id\}`/);
  assert.doesNotMatch(shell, /href=\{`\/posts\/\$\{p\.slug\}`\}>预览/);
  assert.match(shell, /data-color-mode=\{theme === "night" \? "dark" : "light"\}/);
  assert.doesNotMatch(shell, /发布地址|固定链接|系统稳定地址|editor-slug-row|editor-slug-input/);
  assert.doesNotMatch(styles, /editor-slug-row|editor-slug-input/);
  assert.match(app, /function FriendsPage/);
  assert.match(login, /scene\.cover_title/);
  assert.match(login, /scene\.cover_subtitle \|\| scene\.cover_dialogue/);
  assert.match(login, /fetch\("\/api\/v1\/raiments"/);
  assert.match(login, /scheduledRaimentId\(payload\)/);
  assert.match(login, /scene\.cover_url/);
  assert.match(login, /scene\.cover_title/);
  assert.match(login, /scene\.cover_voice_url/);
  assert.match(login, /scene\.login_success_voice_url/);
  assert.doesNotMatch(login, /loginScenes|\/storage\/voice\/login|saber-day\.png|saber-night\.png/);
  assert.doesNotMatch(app, /playLoginSuccessVoice|15_000/);
  assert.match(app, /sessionStorage\.setItem\("helt-login-success-voice"/);
  assert.match(app, /window\.location\.replace\("\/admin"\)/);
  assert.match(app, /停止播放/);
  assert.match(app, /requestAnimationFrame\(syncVoiceState\)/);
  assert.match(app, /!audio\.paused && !audio\.ended/);
  assert.match(app, /音频预览/);
  assert.match(app, /asset-reference-tag/);
  assert.match(assets, /aria-label="素材排列方式"/);
  assert.match(assets, /params\.set\("sort", sortField\)/);
  assert.match(assets, /params\.set\("order", sortOrder\)/);
  assert.doesNotMatch(app, /点击素材查看详情 · 选择模式可批量下载 \/ 删除/);
  assert.match(app, /修改密码/);
  assert.match(app, /保存 Passkey/);
  assert.match(app, /注销登录/);
  assert.match(app, /编辑个人资料/);
  assert.match(app, /B 站 UID/);
  assert.match(app, /Steam Web API Key/);
  assert.match(app, /SteamID64/);
  assert.match(app, /steam_web_api_key/);
  assert.match(app, /steam_id64/);
  assert.match(account, /required=\{steamWebApiKeyPresent\}/);
  assert.match(account, /update_sync:\s*profileTab === "sync"/);
  assert.match(account, /disabled=\{busy === "profile" \|\| \(profileTab === "sync" && !steamPairComplete\)\}/);
  assert.match(account, /留空并保存即可移除/);
  assert.doesNotMatch(account, /clear_steam_web_api_key|清除已保存的 Steam 凭据|只有勾选此项才会删除 Key|留空保留已保存的 Key/);
  assert.match(app, /\/api\/v1\/games/);
  assert.match(app, /playtime_forever_minutes/);
  assert.match(app, /最近两周/);
  assert.match(mediaHooks, /fetchGamePage\(page, sort, range/);
  assert.match(gamesPage, /<select aria-label="游戏排序" value=\{sort\}/);
  assert.match(gamesPage, /<option value="playtime">累计时长<\/option>/);
  assert.match(gamesPage, /href=\{item\.steam_url\} target="_blank" rel="noreferrer"/);
  assert.match(gamesPage, /前往 Steam 官网/);
  assert.doesNotMatch(media, /STEAM PLAY LOG|查看游戏进程|className="steam-link"/);
  assert.match(styles, /\.media-cover\.steam-cover\s*\{[^}]*padding:\s*0/);
  assert.match(styles, /\.steam-cover img\s*\{[^}]*width:\s*100%\s*!important[^}]*height:\s*100%\s*!important/);
  assert.match(styles, /\.game-card \.media-copy p\s*\{[^}]*display:\s*block/);
  assert.match(mediaApi, /range === "recent"\) query\.set\("recent", "true"\)/);
  assert.match(gamesPage, />游戏库 \{meta\.counts\.total\}<\/button>/);
  assert.match(gamesPage, />最近两周 \{meta\.counts\.recent\}<\/button>/);
  assert.doesNotMatch(app, /Fate\/Samurai Remnant|死亡搁浅 2/);
  assert.doesNotMatch(app, /个人中心|账户安全|会清除此设备上的后台会话/);
  assert.match(app, /\/api\/v1\/admin\/auth\/change-password/);
  assert.match(app, /\/api\/v1\/admin\/auth\/profile/);
  assert.match(app, /\/api\/v1\/admin\/assets\?media_type=image/);
  assert.match(playlistSettings, /media_type:\s*"audio"/);
  assert.match(playlistSettings, /\/api\/v1\/admin\/assets\?\$\{params\}/);
  assert.match(playlistSettings, /\/api\/v1\/admin\/playlists/);
  assert.match(playlistSettings, /source_kind:\s*sourceKind/);
  assert.match(playlistSettings, /网易云音乐/);
  assert.match(playlistSettings, /QQ 音乐/);
  assert.match(playlistSettings, /重命名/);
  assert.match(playlistSettings, /createPortal/);
  assert.match(playlistSettings, /params\.set\("search"/);
  assert.match(playlistSettings, /TRACK_PAGE_SIZE = 10/);
  assert.match(playlistSettings, /\/tracks\?\$\{params\}/);
  assert.match(playlistSettings, /playlist-track-pagination/);
  assert.doesNotMatch(playlistSettings, /<select|<audio|播放器设置|默认音量/);
  assert.doesNotMatch(playlistSettings, /统一管理曲目来源与展示顺序|每页 \{TRACK_PAGE_SIZE\} 首/);
  assert.match(shell, /function BackgroundMusic/);
  assert.match(shell, /siteFeaturesReady && site\.features\.music && <BackgroundMusic/);
  assert.doesNotMatch(shell, /FloatingTools|floating-theme|打开看板娘/);
  assert.match(shell, /<Footer \/>/);
  assert.match(shell, /site\.basic\.footer_text/);
  assert.match(siteSettings, /updateBasic\("footer_text"/);
  assert.match(shell, /site\.basic\.hero_eyebrow/);
  assert.match(siteSettings, /updateBasic\("hero_eyebrow"/);
  assert.match(raiments, /封面左下角对白/);
  assert.match(raiments, /addDialogue/);
  assert.match(shell, /window\.setInterval\(nextDialogue, 6000\)/);
  assert.match(shell, /void audio\.play\(\)\.catch/);
  assert.match(shell, /onPointerDown=\{\(event\) => event\.stopPropagation\(\)\}/);
  assert.match(shell, /onClick=\{\(event\) => \{ event\.stopPropagation\(\); nextDialogue\(\); \}\}/);
  assert.match(shell, /!siteFeaturesReady \|\| pathname\.startsWith\("\/admin"\) \|\| !site\.features\.stats/);
  assert.match(shell, /!siteFeaturesReady \|\| pathname\.startsWith\("\/admin"\) \|\| !site\.features\.easter_egg/);
  assert.match(shell, /setEasterEggPath\(null\)/);
  assert.match(shell, /scheduledPeriod\(schedule\)\?\.playlist_id/);
  assert.match(shell, /\/api\/v1\/playlists/);
  assert.match(shell, /<audio/);
  assert.match(styles, /\.background-music-player/);
  assert.doesNotMatch(shell, /THIS ILLUSION|to the beginning|BGM 播放列表|音乐与语音/);
  assert.match(app, /avatar_asset_id/);
  assert.match(app, /更换头像/);
  assert.match(account, /保存后立即联动关于页/);
  assert.match(account, /about:\s*normalizedAbout/);
  assert.match(account, /个人介绍（支持 Markdown）/);
  assert.match(account, /updateAbout\("skills"/);
  assert.match(account, /updateAbout\("socials"/);
  assert.doesNotMatch(app, /头像从素材库引用，替换素材版本后会自动同步/);
  assert.match(app, /\/api\/v1\/profile/);
  assert.match(shell, /profile\.stats\.article_count/);
  assert.match(shell, /profile\.stats\.uptime_days/);
  assert.match(shell, /profile\.about\.intro_md/);
  assert.match(shell, /profile\.avatar_crop_zoom/);
  assert.doesNotMatch(shell, /GitHub 主页为演示链接|哔哩哔哩主页为演示链接/);
  assert.doesNotMatch(app, /\/api\/v1\/admin\/auth\/avatar/);
  assert.match(app, /admin-avatar-crop-dialog/);
  assert.match(app, /框定头像范围/);
  assert.match(app, /DEFAULT_PROFILE_AVATAR_URL = "\/storage\/avatars\/default\/admin-avatar\.webp"/);
  assert.match(app, /admin\.avatar_url \|\| DEFAULT_PROFILE_AVATAR_URL/);
  assert.match(app, /profile\.avatar_url \|\| DEFAULT_PROFILE_AVATAR_URL/);
  assert.match(app, /dialog === "profile"/);
  assert.match(app, /className="admin-avatar-library"/);
  assert.match(app, />替换素材<\/button>/);
  assert.match(app, /asset-detail-image/);
  assert.doesNotMatch(app, /当前版本|历史版本|替换版本|素材 ID|素材ID|版本回滚|current_version/);
  assert.doesNotMatch(app, /生成后先查看 Diff|本次文章专用|先填写正文草稿|选择已有或新增|可多选、可新增|在首页优先展示|读者可以在文章下留言|在文章页显示相关对话|封面会显示在首页文章卡片|写作小贴士|Markdown 编辑器插件/);
  assert.doesNotMatch(raiments, /统一管理每套灵衣|每种颜色都可以独立修改|排序决定前台手动切换顺序/);
  assert.doesNotMatch(llm, /新增时只验证并保存连接凭据|长期运行的场景在这里绑定/);
  assert.doesNotMatch(siteSettings, /关闭后首页直接显示文章流|关闭后所有文章隐藏 Artalk 评论区|控制右下角看板娘入口与对话|控制按时间段联动的站内播放器|控制首页统计条与匿名 PV\/UV 上报|控制键盘隐藏彩蛋|>当日<|>跨午夜<|歌单可在“歌单”页面维护/);
  assert.match(siteSettings, /Konami 彩蛋（↑ ↑ ↓ ↓ ← → ← → B A）/);
  const sidebarUser = app.match(/<div className="admin-user">[\s\S]*?<\/aside>/)?.[0] ?? "";
  assert.match(sidebarUser, /<small>\{currentAdmin\.email \|\| "唯一管理员"\}<\/small>/);
  assert.doesNotMatch(app, /\brole:\s*string\b|Administrator|ADMINISTRATOR/);
  assert.doesNotMatch(app, /这组资料用于后台身份展示|仅用于后台资料展示|留空会停止使用当前账号同步追番数据/);
  assert.match(app, /\/api\/v1\/admin\/auth\/passkeys\/options/);
  assert.match(app, /navigator\.credentials\.create/);
  assert.doesNotMatch(app, /admin-user-menu-button|•••/);
  assert.doesNotMatch(app, /语音暂时无法播放|onCanPlay/);
  assert.doesNotMatch(packageJson, /react-loading-skeleton/);
  assert.match(viteConfig, /127\.0\.0\.1:3001/);
  assert.match(viteConfig, /"\/api"/);
  assert.match(viteConfig, /127\.0\.0\.1:3000/);
  assert.match(viteConfig, /"\/storage"/);
  assert.match(styles, /\.login-theme-switch\s*\{[\s\S]*?right:\s*32px/);
  assert.match(styles, /\.login-theme-switch i\s*\{/);
  assert.match(styles, /--login-switch-text:\s*rgba\(255,246,250,\.84\)/);
  assert.match(styles, /--login-switch-text:\s*rgba\(21,55,96,\.88\)/);
  assert.match(styles, /\.admin-account-menu\s*\{/);
  assert.match(styles, /\.admin-account-menu\s*\{[^}]*right:\s*24px/);
  assert.match(styles, /:root\[data-theme="night"\] \.admin-account-hero\s*\{/);
  assert.doesNotMatch(styles, /\.admin-account-section|\.admin-account-menu > footer/);
  assert.match(styles, /\.admin-profile-trigger\s*\{/);
  assert.match(styles, /\.admin-avatar-editor\s*\{/);
  assert.match(styles, /\.admin-profile-tabs\s*\{/);
  assert.match(styles, /\.about-intro-card\s*\{/);
  assert.match(styles, /\.profile-links\s*\{/);
  assert.match(styles, /\.admin-avatar-crop-dialog\s*\{/);
  assert.match(styles, /\.admin-avatar-crop-dialog\s*\{[^}]*width:\s*min\(420px,\s*100%\)/);
  assert.match(styles, /\.admin-avatar-crop-dialog\s*\{[^}]*height:\s*fit-content/);
  assert.match(styles, /\.admin-account-dialog form > footer,[^}]*padding:\s*17px 0 0/);
  assert.match(styles, /\.admin-avatar-crop-dialog > footer\s*\{[^}]*padding:\s*14px 0 0/);
  assert.match(styles, /\.admin-avatar-crop-stage\s*\{/);
  assert.match(styles, /\.admin-user\s*\{[^}]*margin:\s*0 12px 14px/);
  assert.match(styles, /\.admin-user\s*\{[^}]*border-left:\s*3px solid #5f8bd2/);
  assert.match(styles, /\.admin-user div\s*\{[^}]*flex-direction:\s*column/);
  assert.match(styles, /\.admin-user small\s*\{/);
  assert.match(styles, /\.admin-account-dialog\s*\{/);
});

test("server-renders the admin login shell and real authentication form", async () => {
  const response = await render("/admin/login");
  assert.equal(response.status, 200);
  const html = await response.text();
  assert.match(html, /ADMIN ACCESS/);
  assert.match(html, /契 约 · 成 立/);
  assert.match(html, /灵衣加载中/);
  assert.doesNotMatch(html, /\/storage\/voice\/login/);
  assert.match(html, /name="username"/);
  assert.match(html, /name="password"/);
  assert.doesNotMatch(html, /忘记密码|通行密钥|Passkey|MASTER AUTHENTICATION|SECURE ADMIN GATEWAY|NIGHT CONTRACT|SYSTEM TIME|恢复自动/);
  assert.doesNotMatch(html, /value="excalibur"/);
});

test("renders the article loading boundary and delegates slug resolution to the API", async () => {
  const articleResponse = await render("/posts/spring-anime-2026");
  assert.equal(articleResponse.status, 200);
  const articleHtml = await articleResponse.text();
  assert.match(articleHtml, /正在读取文章/);

  const missingResponse = await render("/posts/not-a-real-post");
  assert.equal(missingResponse.status, 200);
  const missingHtml = await missingResponse.text();
  assert.match(missingHtml, /正在读取文章/);
});

test("wires every account-security request to a credentialed front-end flow", async () => {
  const app = (await Promise.all([
    readFile(new URL("app/BlogApp.tsx", root), "utf8"),
    readFile(new URL("app/admin/AdminLogin.tsx", root), "utf8"),
    readFile(new URL("app/admin/AdminAccountCenter.tsx", root), "utf8"),
  ])).join("\n");
  const expected = [
    ["/api/v1/admin/auth/me", /credentials:\s*"include"/],
    ["/api/v1/admin/auth/refresh", /method:\s*"POST"/],
    ["/api/v1/admin/auth/login", /method:\s*"POST"/],
    ["/api/v1/admin/auth/profile", /method:\s*"PATCH"/],
    ["/api/v1/admin/auth/change-password", /method:\s*"POST"/],
    ["/api/v1/admin/auth/passkeys", /credentials:\s*"include"/],
    ["/api/v1/admin/auth/passkeys/options", /method:\s*"POST"/],
    ["/api/v1/admin/auth/logout", /method:\s*"POST"/],
  ];
  for (const [path] of expected) assert.ok(app.includes(path), `missing front-end flow for ${path}`);
  assert.match(app, /navigator\.credentials\.create/);
  assert.match(app, /window\.location\.replace\("\/admin\/login"\)/);
  assert.match(app, /newPassword !== confirmPassword/);
  assert.match(app, /newPassword\.length < 12/);
  assert.match(app, /revoke_other_sessions:\s*revokeOtherSessions/);
  assert.match(app, /撤销其他设备会话/);
});

test("wires the complete asset-library API and destructive-action guards", async () => {
  const app = await readFile(new URL("app/admin/AssetManager.tsx", root), "utf8");
  for (const endpoint of [
    "/api/v1/admin/assets?",
    "/api/v1/admin/assets/${id}",
    "/api/v1/admin/assets/${detail.asset.id}",
    "/api/v1/admin/assets/${detail.asset.id}/replace",
    "/api/v1/admin/assets/${path}",
  ]) {
    assert.ok(app.includes(endpoint), `missing asset front-end flow for ${endpoint}`);
  }
  assert.match(app, /path:\s*"batch-delete"\s*\|\s*"batch-download"/);
  assert.match(app, /method:\s*"PATCH"/);
  assert.match(app, /method:\s*"DELETE"/);
  assert.match(app, /detail\.references\.length > 0/);
  assert.match(app, /file\.size <= MAX_FILE_BYTES/);
  assert.match(app, /URL\.revokeObjectURL/);
  assert.match(app, /MAX_CONCURRENT_UPLOADS = 3/);
  assert.match(app, /new XMLHttpRequest\(\)/);
  assert.match(app, /request\.upload\.addEventListener\("progress"/);
  assert.match(app, /cancelUpload/);
  assert.match(app, /retryUpload/);
  assert.match(app, /确认批量删除/);
  assert.doesNotMatch(app, /window\.prompt/);
});
