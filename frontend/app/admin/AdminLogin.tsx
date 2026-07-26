"use client";

import { FormEvent, useEffect, useRef, useState } from "react";

import { isJsonResponse, responseMessage, Theme } from "./shared";

const loginScenes = {
  day: {
    Japanese: "問おう。貴方が私のマスターか？",
    Chinese: "试问。你是我的御主吗？",
    voice: "/storage/voice/login/blue-saber.mp3",
    successVoice: "/storage/voice/login/blue-saber-success.mp3",
  },
  night: {
    Japanese: "召喚に応じ参上した。貴様が私のマスターという奴か？",
    Chinese: "应召唤前来。你这家伙就是我的御主吗？",
    voice: "/storage/voice/login/alter-saber.mp3",
    successVoice: "/storage/voice/login/alter-saber-success.mp3",
  },
} as const satisfies Record<Theme, {
  Japanese: string;
  Chinese: string;
  voice: string;
  successVoice: string;
}>;

function loginThemeForCurrentTime(date = new Date()): Theme {
  const hour = date.getHours();
  return hour >= 7 && hour < 19 ? "day" : "night";
}

export function AdminLogin() {
  const [show, setShow] = useState(false);
  const [username, setUsername] = useState("helt");
  const [password, setPassword] = useState("");
  const [remember, setRemember] = useState(true);
  const [busy, setBusy] = useState(false);
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

  const playLoginVoice = async (audio = audioRef.current) => {
    if (!audio) return;
    audio.currentTime = 0;
    try {
      await audio.play();
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

      // Navigation is immediate. The destination page consumes this marker and
      // starts the success voice independently from authentication.
      sessionStorage.setItem("helt-login-success-voice", scene.successVoice);
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
    <main className={`admin-login login-theme-${loginTheme}`}>
      <div className="admin-login-cover login-cover-day" aria-hidden="true" />
      <div className="admin-login-cover login-cover-night" aria-hidden="true" />
      <div className="admin-login-shade" aria-hidden="true" />
      <button className="login-theme-switch" type="button" onClick={toggleLoginTheme} aria-label={`切换至${loginTheme === "day" ? "夜间" : "日间"}灵衣`}>
        <b>{loginTheme === "day" ? "夜间模式" : "日间模式"}</b>
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
      <section className="login-scene-copy" aria-live="polite">
        <div>
          <span>{loginTheme === "day" ? "SABER / BLUE" : "SABER / ALTER"}</span>
          <h2>{scene.Japanese}</h2>
          <p>{scene.Chinese}</p>
        </div>
        <div className="login-scene-actions">
          <button className={`login-voice-button${voicePlaying ? " is-playing" : ""}`} type="button" onClick={() => void toggleVoice()} aria-pressed={voicePlaying}>
            <span className="login-voice-glyph" aria-hidden="true"><i /><i /><i /><i /></span>
            {voicePlaying ? "停止播放" : "语音放送"}
          </button>
        </div>
        <audio id="admin-login-voice" ref={audioRef} src={scene.voice} preload="metadata" />
      </section>
    </main>
  );
}
