"use client";

import { useCallback, useEffect, useState } from "react";

import {
  Notify,
  RaimentSchedule,
  RaimentSchedulePeriod,
  responseMessage,
} from "./shared";

type RaimentOption = {
  id: string;
  name: string;
  color_scheme: "day" | "night";
  enabled: boolean;
  is_default: boolean;
};

type RaimentPayload = {
  items: RaimentOption[];
};

const newPeriodId = () => `period-${typeof crypto !== "undefined" && "randomUUID" in crypto ? crypto.randomUUID() : Date.now()}`;

export function SiteSettings({ notify }: { notify: Notify }) {
  const [schedule, setSchedule] = useState<RaimentSchedule | null>(null);
  const [raiments, setRaiments] = useState<RaimentOption[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  const load = useCallback(async (signal?: AbortSignal) => {
    setLoading(true);
    try {
      const [scheduleResponse, raimentResponse] = await Promise.all([
        fetch("/api/v1/admin/site/raiment-schedule", { credentials: "include", signal }),
        fetch("/api/v1/admin/raiments", { credentials: "include", signal }),
      ]);
      if (!scheduleResponse.ok) {
        throw new Error(await responseMessage(scheduleResponse, "灵衣时间段加载失败"));
      }
      if (!raimentResponse.ok) {
        throw new Error(await responseMessage(raimentResponse, "灵衣列表加载失败"));
      }
      const [nextSchedule, nextRaiments] = await Promise.all([
        scheduleResponse.json() as Promise<RaimentSchedule>,
        raimentResponse.json() as Promise<RaimentPayload>,
      ]);
      setSchedule(nextSchedule);
      setRaiments(nextRaiments.items.filter((item) => item.enabled));
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") return;
      notify(error instanceof Error ? error.message : "站点设置加载失败", "danger");
    } finally {
      if (!signal?.aborted) setLoading(false);
    }
  }, [notify]);

  useEffect(() => {
    const controller = new AbortController();
    const timer = window.setTimeout(() => void load(controller.signal), 0);
    return () => {
      window.clearTimeout(timer);
      controller.abort();
    };
  }, [load]);

  const updatePeriod = (id: string, patch: Partial<RaimentSchedulePeriod>) => {
    setSchedule((current) => current ? {
      ...current,
      periods: current.periods.map((period) => period.id === id ? { ...period, ...patch } : period),
    } : current);
  };

  const addPeriod = () => {
    if (!raiments.length) {
      notify("请先添加至少一套灵衣", "danger");
      return;
    }
    setSchedule((current) => current ? {
      ...current,
      periods: [...current.periods, {
        id: newPeriodId(),
        start_at: "08:00",
        end_at: "18:00",
        raiment_id: raiments[0].id,
      }],
    } : current);
  };

  const removePeriod = (id: string) => {
    setSchedule((current) => current ? {
      ...current,
      periods: current.periods.filter((period) => period.id !== id),
    } : current);
  };

  const save = async () => {
    if (!schedule || saving) return;
    setSaving(true);
    try {
      const response = await fetch("/api/v1/admin/site/raiment-schedule", {
        method: "PUT",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(schedule),
      });
      if (!response.ok) {
        throw new Error(await responseMessage(response, "灵衣时间段保存失败"));
      }
      const saved = await response.json() as RaimentSchedule;
      setSchedule(saved);
      window.dispatchEvent(new Event("helt:raiments-updated"));
      notify("站点灵衣时间段已保存", "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "灵衣时间段保存失败", "danger");
    } finally {
      setSaving(false);
    }
  };

  return <>
    <div className="admin-title">
      <div><h1>站点设置</h1><p>SITE CONFIGURATION · 24-HOUR RAIMENT SCHEDULE</p></div>
      <button className="admin-primary" type="button" disabled={!schedule || saving} onClick={() => void save()}>{saving ? "保存中…" : "保存设置"}</button>
    </div>
    <div className="settings-grid">
      <section className="admin-panel form-panel">
        <h2>基本信息</h2>
        <label>站点名称<input defaultValue="helt." /></label>
        <label>站点描述<textarea defaultValue="写代码、追番、折腾博客的个人小站。" /></label>
        <label>站点地址<input defaultValue="https://helt.example.com" /></label>
      </section>
      <section className="admin-panel toggles">
        <h2>功能开关</h2>
        {[["开屏页", "关闭后直接进入文章流"], ["看板娘", "显示 Live2D 角色与对话"], ["背景音乐", "显示全局音乐播放器"], ["Konami 彩蛋", "启用键盘隐藏彩蛋"]].map(([label, description], index) => <div key={label}>
          <span><b>{label}</b><small>{description}</small></span>
          <label className="toggle"><input type="checkbox" defaultChecked={index !== 3} onChange={(event) => notify(`${label}已${event.target.checked ? "开启" : "关闭"}`)} /><i /></label>
        </div>)}
      </section>
    </div>

    <section className="admin-panel raiment-schedule-panel">
      <header>
        <div><span>AUTOMATION</span><h2>灵衣时间段</h2><p>使用访客设备的本地时间，以 24 小时制匹配。跨午夜可直接填写，例如 19:00—07:00。</p></div>
        <button type="button" onClick={addPeriod} disabled={loading || !schedule}>＋ 新增时间段</button>
      </header>
      {loading && <div className="raiment-schedule-empty">正在读取时间段…</div>}
      {!loading && schedule?.periods.map((period, index) => {
        const wraps = period.start_at >= period.end_at;
        return <div className="raiment-schedule-row" key={period.id}>
          <b>{String(index + 1).padStart(2, "0")}</b>
          <label>开始<input type="time" step={60} value={period.start_at} onChange={(event) => updatePeriod(period.id, { start_at: event.target.value })} /></label>
          <span>→</span>
          <label>结束<input type="time" step={60} value={period.end_at} onChange={(event) => updatePeriod(period.id, { end_at: event.target.value })} /></label>
          <label>使用灵衣<select value={period.raiment_id} onChange={(event) => updatePeriod(period.id, { raiment_id: event.target.value })}>{raiments.map((raiment) => <option value={raiment.id} key={raiment.id}>{raiment.name}</option>)}</select></label>
          <small>{wraps ? "跨午夜" : "当日"}</small>
          <button type="button" aria-label={`删除第 ${index + 1} 个时间段`} onClick={() => removePeriod(period.id)}>×</button>
        </div>;
      })}
      {!loading && schedule && !schedule.periods.length && <div className="raiment-schedule-empty">尚未设置自动时间段；前台将使用默认灵衣，访客仍可手动切换。</div>}
      <footer>时间段不可重叠；开始时间包含在内，结束时间不包含在内。未覆盖的时刻使用默认灵衣。</footer>
    </section>
  </>;
}
