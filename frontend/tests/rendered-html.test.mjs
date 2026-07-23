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
  assert.match(app, /停止播放/);
  assert.match(app, /requestAnimationFrame\(syncVoiceState\)/);
  assert.match(app, /!audio\.paused && !audio\.ended/);
  assert.doesNotMatch(app, /语音暂时无法播放|onCanPlay/);
  assert.doesNotMatch(packageJson, /react-loading-skeleton/);
  assert.match(viteConfig, /127\.0\.0\.1:3001/);
  assert.match(viteConfig, /"\/api"/);
  assert.match(viteConfig, /127\.0\.0\.1:8080/);
  assert.match(viteConfig, /"\/storage"/);
  assert.match(styles, /\.login-theme-switch\s*\{[\s\S]*?right:\s*32px/);
  assert.match(styles, /\.login-theme-switch i\s*\{/);
});

test("server-renders the admin login design and real authentication form", async () => {
  const response = await render("/admin/login");
  assert.equal(response.status, 200);
  const html = await response.text();
  assert.match(html, /契约仪式/);
  assert.match(html, /契 约 · 成 立/);
  assert.match(html, /通行密钥 Passkey 登录/);
  assert.match(html, /语音放送/);
  assert.match(html, /灵衣切换/);
  assert.doesNotMatch(html, /☀|☾/);
  assert.match(html, /問おう。貴方が私のマスターか？|召喚に応じ参上した。貴様が私のマスターという奴か？/);
  assert.match(html, /试问。你是我的御主吗？|应召唤前来。你这家伙就是我的御主吗？/);
  assert.match(html, /\/storage\/voice\/login\/(?:blue-saber|alter-saber)\.mp3/);
  assert.match(html, /autoplay|autoPlay/);
  assert.match(html, /name="username"/);
  assert.match(html, /name="password"/);
  assert.doesNotMatch(html, /MASTER AUTHENTICATION|SECURE ADMIN GATEWAY|NIGHT CONTRACT|SYSTEM TIME|恢复自动/);
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
