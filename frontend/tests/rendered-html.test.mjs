import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import test from "node:test";

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
  const [shell, login, account, assets, llm, shared, packageJson, viteConfig, styles] = await Promise.all([
    readFile(new URL("app/BlogApp.tsx", root), "utf8"),
    readFile(new URL("app/admin/AdminLogin.tsx", root), "utf8"),
    readFile(new URL("app/admin/AdminAccountCenter.tsx", root), "utf8"),
    readFile(new URL("app/admin/AssetManager.tsx", root), "utf8"),
    readFile(new URL("app/admin/LlmSettings.tsx", root), "utf8"),
    readFile(new URL("app/admin/shared.ts", root), "utf8"),
    readFile(new URL("package.json", root), "utf8"),
    readFile(new URL("vite.config.ts", root), "utf8"),
    readFile(new URL("app/globals.css", root), "utf8"),
    access(new URL("public/saber-day.png", root)),
    access(new URL("public/saber-night.png", root)),
    access(new URL("public/og.png", root)),
  ]);
  const app = [shell, login, account, assets, llm, shared].join("\n");
  assert.doesNotMatch(app, /const posts = \[/);
  assert.match(app, /\/api\/v1\/articles/);
  assert.match(app, /\/api\/v1\/admin\/articles/);
  assert.match(app, /\/api\/v1\/admin\/articles\/batch/);
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
  assert.match(app, /comment_review/);
  assert.doesNotMatch(llm, /label: "文章助手"/);
  assert.doesNotMatch(app, /供应商、模型、密钥和提示词只在这里维护|llm-source-banner|llm-policy-panel|服务商/);
  assert.doesNotMatch(shell, /raiment-model-panel|人格提示词|api\.example\.com|ai-reference-note/);
  assert.ok(shell.includes('<Link href="/admin" className="brand">helt<span>.</span> <small>ADMIN</small></Link>'));
  assert.doesNotMatch(shell, /\["\/admin\/articles\/new",\s*"✎",\s*"撰写文章"\]/);
  assert.match(shell, /data-color-mode=\{theme === "night" \? "dark" : "light"\}/);
  assert.doesNotMatch(shell, /发布地址|固定链接|系统稳定地址|editor-slug-row|editor-slug-input/);
  assert.doesNotMatch(styles, /editor-slug-row|editor-slug-input/);
  assert.match(app, /function FriendsPage/);
  assert.match(app, /問おう。貴方が私のマスターか？/);
  assert.match(app, /试问。你是我的御主吗？/);
  assert.match(app, /召喚に応じ参上した。貴様が私のマスターという奴か？/);
  assert.match(app, /应召唤前来。你这家伙就是我的御主吗？/);
  assert.match(app, /\/storage\/voice\/login\/blue-saber\.mp3/);
  assert.match(app, /\/storage\/voice\/login\/alter-saber\.mp3/);
  assert.match(app, /\/storage\/voice\/login\/blue-saber-success\.mp3/);
  assert.match(app, /\/storage\/voice\/login\/alter-saber-success\.mp3/);
  assert.doesNotMatch(app, /playLoginSuccessVoice|15_000/);
  assert.match(app, /sessionStorage\.setItem\("helt-login-success-voice"/);
  assert.match(app, /window\.location\.replace\("\/admin"\)/);
  assert.match(app, /停止播放/);
  assert.match(app, /requestAnimationFrame\(syncVoiceState\)/);
  assert.match(app, /!audio\.paused && !audio\.ended/);
  assert.match(app, /音频预览/);
  assert.match(app, /asset-reference-tag/);
  assert.doesNotMatch(app, /点击素材查看详情 · 选择模式可批量下载 \/ 删除/);
  assert.match(app, /修改密码/);
  assert.match(app, /保存 Passkey/);
  assert.match(app, /注销登录/);
  assert.match(app, /编辑个人资料/);
  assert.match(app, /B 站 UID/);
  assert.doesNotMatch(app, /个人中心|账户安全|会清除此设备上的后台会话/);
  assert.match(app, /\/api\/v1\/admin\/auth\/change-password/);
  assert.match(app, /\/api\/v1\/admin\/auth\/profile/);
  assert.match(app, /\/api\/v1\/admin\/assets\?media_type=image/);
  assert.match(app, /avatar_asset_id/);
  assert.match(app, /更换头像/);
  assert.doesNotMatch(app, /头像从素材库引用，替换素材版本后会自动同步/);
  assert.match(app, /\/api\/v1\/profile/);
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

test("server-renders the admin login design and real authentication form", async () => {
  const response = await render("/admin/login");
  assert.equal(response.status, 200);
  const html = await response.text();
  assert.match(html, /ADMIN ACCESS/);
  assert.match(html, /契 约 · 成 立/);
  assert.match(html, /语音放送/);
  assert.match(html, /切换至(?:夜间|日间)灵衣/);
  assert.match(html, /問おう。貴方が私のマスターか？|召喚に応じ参上した。貴様が私のマスターという奴か？/);
  assert.match(html, /试问。你是我的御主吗？|应召唤前来。你这家伙就是我的御主吗？/);
  assert.match(html, /\/storage\/voice\/login\/(?:blue-saber|alter-saber)\.mp3/);
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
