"use client";

import Image from "next/image";
import { FormEvent, useEffect, useRef, useState } from "react";

import {
  isJsonResponse,
  PublicRaiment,
  PublicRaimentPayload,
  responseMessage,
  scheduledRaimentId,
} from "./shared";

export function AdminLogin() {
  const [show, setShow] = useState(false);
  const [username, setUsername] = useState("helt");
  const [password, setPassword] = useState("");
  const [remember, setRemember] = useState(true);
  const [busy, setBusy] = useState(false);
  const [feedback, setFeedback] = useState<{ tone: "error" | "success"; message: string } | null>(null);
  const [catalog, setCatalog] = useState<PublicRaimentPayload | null>(null);
  const [activeRaimentId, setActiveRaimentId] = useState("");
  const [voicePlaying, setVoicePlaying] = useState(false);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const playAfterSwitch = useRef(false);
  const manualSelection = useRef(false);
  const scene: PublicRaiment | null = catalog?.items.find((item) => item.id === activeRaimentId)
    || catalog?.items.find((item) => item.id === catalog.default_raiment_id)
    || catalog?.items[0]
    || null;
  const sceneIndex = scene && catalog ? catalog.items.findIndex((item) => item.id === scene.id) : -1;
  const nextScene = catalog?.items[(sceneIndex + 1) % catalog.items.length] || null;
  const loginTheme = scene?.color_scheme || "night";

  useEffect(() => {
    const controller = new AbortController();
    void fetch("/api/v1/raiments", { signal: controller.signal, cache: "no-store" })
      .then(async (response) => {
        if (!response.ok || !isJsonResponse(response)) throw new Error("灵衣目录加载失败");
        return response.json() as Promise<PublicRaimentPayload>;
      })
      .then((payload) => {
        if (!payload.items.length) throw new Error("没有可用的灵衣");
        setCatalog(payload);
        setActiveRaimentId(scheduledRaimentId(payload));
      })
      .catch((error) => {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          console.warn("Admin login raiment catalog unavailable", error);
        }
      });
    return () => controller.abort();
  }, []);

  useEffect(() => {
    if (!catalog || manualSelection.current) return;
    const syncSchedule = () => setActiveRaimentId(scheduledRaimentId(catalog));
    const timer = window.setInterval(syncSchedule, 60_000);
    return () => window.clearInterval(timer);
  }, [catalog]);

  useEffect(() => {
    const audio = audioRef.current;
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

  useEffect(() => {
    const audio = audioRef.current;
    if (!audio) return;
    audio.pause();
    audio.currentTime = 0;
    audio.load();
    if (playAfterSwitch.current && scene?.cover_voice_url) {
      playAfterSwitch.current = false;
      void audio.play().catch(() => setVoicePlaying(false));
    }
  }, [scene?.id, scene?.cover_voice_url]);

  useEffect(() => {
    const message = sessionStorage.getItem("helt-auth-message");
    if (!message) return;
    sessionStorage.removeItem("helt-auth-message");
    const timer = window.setTimeout(() => setFeedback({ tone: "success", message }), 0);
    return () => window.clearTimeout(timer);
  }, []);

  const stopLoginVoice = () => {
    const audio = audioRef.current;
    if (audio) {
      audio.pause();
      audio.currentTime = 0;
    }
  };

  const playLoginVoice = async () => {
    const audio = audioRef.current;
    if (!audio || !scene?.cover_voice_url) return;
    audio.currentTime = 0;
    try {
      await audio.play();
    } catch {
      setVoicePlaying(false);
    }
  };

  const toggleLoginRaiment = () => {
    if (!nextScene || !catalog || catalog.items.length < 2) return;
    stopLoginVoice();
    manualSelection.current = true;
    playAfterSwitch.current = Boolean(nextScene.cover_voice_url);
    setActiveRaimentId(nextScene.id);
  };

  const toggleVoice = async () => {
    const audio = audioRef.current;
    if (!audio) return;
    if (voicePlaying || !audio.paused) {
      stopLoginVoice();
      return;
    }
    await playLoginVoice();
  };

  const submitLogin = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!username.trim() || !password) {
      setFeedback({ tone: "error", message: "请输入账号和密码。" });
      return;
    }
    setBusy(true);
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

      if (scene?.login_success_voice_url) {
        sessionStorage.setItem("helt-login-success-voice", scene.login_success_voice_url);
      } else {
        sessionStorage.removeItem("helt-login-success-voice");
      }
      window.location.replace("/admin");
    } catch (error) {
      setFeedback({
        tone: "error",
        message: error instanceof Error ? error.message : "认证失败，请稍后重试。",
      });
      setBusy(false);
    }
  };

  return (
    <main
      className={`admin-login login-theme-${loginTheme}`}
      style={scene ? {
        "--login-accent": scene.theme.primary,
        "--login-gold": scene.theme.secondary,
        backgroundColor: scene.theme.background,
      } as React.CSSProperties : undefined}
    >
      {scene && <Image key={scene.id} className="admin-login-cover" src={scene.cover_url} fill sizes="100vw" priority unoptimized alt="" />}
      <div className="admin-login-shade" aria-hidden="true" />
      <button
        className="login-theme-switch"
        type="button"
        onClick={toggleLoginRaiment}
        disabled={!catalog || catalog.items.length < 2}
        aria-label={nextScene ? `切换至${nextScene.name}灵衣` : "没有其他可用灵衣"}
      >
        <b>{nextScene ? `切换至 ${nextScene.name}` : scene?.name || "灵衣加载中"}</b>
      </button>
      <form className="login-card" onSubmit={submitLogin}>
        <div className="login-brand">
          <span>ADMIN ACCESS</span>
          <h1>helt<i>.</i></h1>
          <p>以令咒为证，建立管理契约。</p>
        </div>
        <label>账号<input name="username" value={username} onChange={(event) => setUsername(event.target.value)} autoComplete="username" maxLength={64} /></label>
        <label>
          密码
          <span className="login-password">
            <input name="password" type={show ? "text" : "password"} value={password} onChange={(event) => setPassword(event.target.value)} autoComplete="current-password" />
            <button type="button" onClick={() => setShow((value) => !value)} aria-label={show ? "隐藏密码" : "显示密码"}>{show ? "隐藏" : "显示"}</button>
          </span>
        </label>
        <label className="login-remember"><input type="checkbox" checked={remember} onChange={(event) => setRemember(event.target.checked)} />七日内保持契约</label>
        {feedback && <div className={`login-feedback ${feedback.tone}`} role={feedback.tone === "error" ? "alert" : "status"}><span aria-hidden="true">{feedback.tone === "error" ? "!" : "✓"}</span>{feedback.message}</div>}
        <button className="login-submit" disabled={busy}>{busy ? "仪 式 进 行 中…" : "契 约 · 成 立"}</button>
      </form>
      {scene && <section className="login-scene-copy" aria-live="polite">
        <div>
          <span className="login-scene-name">{scene.cover_character_name || scene.name}</span>
          <blockquote>
            <p>{scene.cover_title}</p>
            <p>{scene.cover_subtitle || scene.cover_dialogue}</p>
          </blockquote>
        </div>
        {scene.cover_voice_url && <div className="login-scene-actions">
          <button className={`login-voice-button${voicePlaying ? " is-playing" : ""}`} type="button" onClick={() => void toggleVoice()} aria-pressed={voicePlaying}>
            <span className="login-voice-glyph" aria-hidden="true"><i /><i /><i /><i /></span>
            {voicePlaying ? "停止播放" : scene.cover_voice_label || "语音放送"}
          </button>
        </div>}
      </section>}
      <audio id="admin-login-voice" ref={audioRef} src={scene?.cover_voice_url || undefined} preload="metadata" />
    </main>
  );
}
