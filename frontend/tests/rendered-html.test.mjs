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

test("keeps project assets and mock front-end routes in place", async () => {
  const [app, packageJson, viteConfig, styles] = await Promise.all([
    readFile(new URL("app/BlogApp.tsx", root), "utf8"),
    readFile(new URL("package.json", root), "utf8"),
    readFile(new URL("vite.config.ts", root), "utf8"),
    readFile(new URL("app/globals.css", root), "utf8"),
    access(new URL("public/saber-day.png", root)),
    access(new URL("public/saber-night.png", root)),
    access(new URL("public/og.png", root)),
  ]);
  assert.match(app, /const posts = \[/);
  assert.match(app, /function AdminLayout/);
  assert.match(app, /function FriendsPage/);
  assert.match(app, /問おう。貴方が私のマスターか？/);
  assert.match(app, /试问。你是我的御主吗？/);
  assert.match(app, /召喚に応じ参上した。貴様が私のマスターという奴か？/);
  assert.match(app, /应召唤前来。你这家伙就是我的御主吗？/);
  assert.match(app, /\/storage\/voice\/login\/blue-saber\.mp3/);
  assert.match(app, /\/storage\/voice\/login\/alter-saber\.mp3/);
  assert.match(app, /\/storage\/voice\/login\/blue-saber-success\.mp3/);
  assert.match(app, /\/storage\/voice\/login\/alter-saber-success\.mp3/);
  assert.match(app, /await playLoginSuccessVoice\(\)/);
  assert.match(app, /停止播放/);
  assert.match(app, /requestAnimationFrame\(syncVoiceState\)/);
  assert.match(app, /!audio\.paused && !audio\.ended/);
  assert.match(app, /修改密码/);
  assert.match(app, /保存 Passkey/);
  assert.match(app, /注销登录/);
  assert.match(app, /编辑个人资料/);
  assert.match(app, /B 站 UID/);
  assert.doesNotMatch(app, /个人中心|账户安全|会清除此设备上的后台会话/);
  assert.match(app, /\/api\/v1\/admin\/auth\/change-password/);
  assert.match(app, /\/api\/v1\/admin\/auth\/profile/);
  assert.match(app, /\/api\/v1\/admin\/auth\/avatar/);
  assert.match(app, /\/api\/v1\/profile/);
  assert.match(app, /URL\.createObjectURL/);
  assert.match(app, /renderCroppedAvatar/);
  assert.match(app, /imageSmoothingQuality = "high"/);
  assert.match(app, /MAX_AVATAR_SOURCE_BYTES = 10 \* 1024 \* 1024/);
  assert.match(app, /type="range"/);
  assert.match(app, /admin-avatar-crop-dialog/);
  assert.match(app, /头像裁剪/);
  assert.match(app, />确定<\/button>/);
  assert.doesNotMatch(app, /框定头像区域|使用此区域/);
  assert.match(app, /DEFAULT_PROFILE_AVATAR_URL = "\/storage\/avatars\/default\/admin-avatar\.webp"/);
  assert.match(app, /admin\.avatar_url \|\| DEFAULT_PROFILE_AVATAR_URL/);
  assert.match(app, /profile\.avatar_url \|\| DEFAULT_PROFILE_AVATAR_URL/);
  assert.match(app, /dialog === "profile" && !avatarCropOpen/);
  const cropDialog = app.match(/\{avatarCropOpen && avatarSource && \([\s\S]*?\{dialog === "password"/)?.[0] ?? "";
  assert.match(cropDialog, /头像裁剪/);
  assert.doesNotMatch(cropDialog, /profileEmail|profileBilibiliUid|邮箱地址|B 站 UID/);
  const sidebarUser = app.match(/<div className="admin-user">[\s\S]*?<\/aside>/)?.[0] ?? "";
  assert.match(sidebarUser, /<small>\{currentAdmin\.email \|\| "ADMINISTRATOR"\}<\/small>/);
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
  assert.match(html, /契约仪式/);
  assert.match(html, /契 约 · 成 立/);
  assert.match(html, /语音放送/);
  assert.match(html, /灵衣切换/);
  assert.doesNotMatch(html, /☀|☾/);
  assert.match(html, /問おう。貴方が私のマスターか？|召喚に応じ参上した。貴様が私のマスターという奴か？/);
  assert.match(html, /试问。你是我的御主吗？|应召唤前来。你这家伙就是我的御主吗？/);
  assert.match(html, /\/storage\/voice\/login\/(?:blue-saber|alter-saber)\.mp3/);
  assert.match(html, /autoplay|autoPlay/);
  assert.match(html, /name="username"/);
  assert.match(html, /name="password"/);
  assert.doesNotMatch(html, /忘记密码|通行密钥|Passkey|MASTER AUTHENTICATION|SECURE ADMIN GATEWAY|NIGHT CONTRACT|SYSTEM TIME|恢复自动/);
  assert.doesNotMatch(html, /value="excalibur"/);
});

test("renders the selected article and rejects unknown article slugs", async () => {
  const articleResponse = await render("/posts/spring-anime-2026");
  assert.equal(articleResponse.status, 200);
  const articleHtml = await articleResponse.text();
  assert.match(articleHtml, /2026 春季番剧总结：这季度我推的都完结了/);
  assert.match(articleHtml, /这一季留下了什么/);
  assert.doesNotMatch(articleHtml, /<h1>重构博客的一些思考<\/h1>/);

  const missingResponse = await render("/posts/not-a-real-post");
  assert.equal(missingResponse.status, 200);
  const missingHtml = await missingResponse.text();
  assert.match(missingHtml, /前方并非约定之地/);
  assert.doesNotMatch(missingHtml, /<h1>重构博客的一些思考<\/h1>/);
});
