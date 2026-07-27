export type Theme = "day" | "night";
export type Notify = (message: string, tone?: "normal" | "success" | "danger") => void;

export type ThemeTokens = {
  primary: string;
  secondary: string;
  background: string;
  surface: string;
  surface_alt: string;
  text: string;
  text_secondary: string;
  muted: string;
  faint: string;
  border: string;
  danger: string;
  success: string;
};

export type RaimentSchedulePeriod = {
  id: string;
  start_at: string;
  end_at: string;
  raiment_id: string;
  playlist_id: number | null;
};

export type RaimentSchedule = {
  revision: number;
  periods: RaimentSchedulePeriod[];
};

export type PublicRaiment = {
  id: string;
  name: string;
  cover_url: string;
  theme: ThemeTokens;
  color_scheme: Theme;
  cover_title: string;
  cover_subtitle: string;
  cover_character_name: string;
  cover_dialogue: string;
  cover_voice_label: string;
  cover_voice_url: string | null;
  login_success_voice_url: string | null;
  kanban_configured: boolean;
};

export type PublicRaimentPayload = {
  items: PublicRaiment[];
  schedule: RaimentSchedule;
  default_raiment_id: string;
};

export function scheduledRaimentId(payload: PublicRaimentPayload, date = new Date()) {
  const available = new Set(payload.items.map((item) => item.id));
  const active = scheduledPeriod(payload.schedule, date);
  if (active && available.has(active.raiment_id)) return active.raiment_id;
  if (available.has(payload.default_raiment_id)) return payload.default_raiment_id;
  return payload.items[0]?.id || "";
}

export const DEFAULT_PROFILE_AVATAR_URL = "/storage/avatars/default/admin-avatar.webp";

export type AdminIdentity = {
  username: string;
  email: string;
  avatar_url: string | null;
  avatar_asset_id: number | null;
  avatar_crop_x: number;
  avatar_crop_y: number;
  avatar_crop_zoom: number;
  bilibili_uid: string;
  steam_web_api_key_configured: boolean;
  steam_web_api_key_masked: string;
  steam_id64: string;
};

export type PublicProfile = Pick<AdminIdentity, "username" | "email" | "avatar_url">;

export type SitePayload = {
  basic: {
    name: string;
    tagline: string;
    domain: string;
    icp: string;
    founded_at: string;
    logo_asset_id: number | null;
    logo_url: string | null;
    favicon_asset_id: number | null;
    favicon_url: string | null;
  };
  features: {
    splash: boolean;
    comments: boolean;
    kanban: boolean;
    music: boolean;
    stats: boolean;
    easter_egg: boolean;
  };
  stats: {
    article_count: number;
    total_words: number;
    total_visits: number;
    uptime_days: number;
  };
  updated_at: string;
};

export type AdminAsset = {
  id: number;
  name: string;
  media_type: "image" | "audio" | "video" | "live2d" | "font" | "other";
  created_at: string;
  reference_count: number;
  file: {
    url: string;
    mime: string;
    size_bytes: number;
    original_filename?: string;
  };
};

export type AssetDetailPayload = {
  asset: AdminAsset;
  references: Array<{
    source_type: string;
    source_id: string;
    source_label: string;
    admin_path: string;
  }>;
};

export type PlaylistTrack = {
  id: string;
  title: string;
  artist: string;
  url: string;
  cover_url: string | null;
  source_kind: "local" | "netease" | "qq";
  duration_s: number;
  sort_order: number;
  asset_id: number | null;
};

export type AdminPlaylist = {
  id: number;
  name: string;
  description: string;
  source_kind: "local" | "netease" | "qq";
  external_id: string | null;
  external_url: string | null;
  enabled: boolean;
  sort_order: number;
  status: "ready" | "unavailable";
  status_message: string | null;
  track_count: number | null;
  tracks?: PlaylistTrack[];
  created_at: string;
  updated_at: string;
};

export type PlaylistPayload = {
  items: AdminPlaylist[];
};

export type PlaylistTracksPayload = {
  page: number;
  per_page: number;
  total: number;
  items: PlaylistTrack[];
  status: "ready" | "unavailable";
  status_message: string | null;
};

export function scheduledPeriod(schedule: RaimentSchedule, date = new Date()) {
  const minutes = date.getHours() * 60 + date.getMinutes();
  return schedule.periods.find((period) => {
    const [startHour, startMinute] = period.start_at.split(":").map(Number);
    const [endHour, endMinute] = period.end_at.split(":").map(Number);
    const start = startHour * 60 + startMinute;
    const end = endHour * 60 + endMinute;
    return start < end
      ? minutes >= start && minutes < end
      : minutes >= start || minutes < end;
  }) || null;
}

type ApiErrorPayload = {
  error?: { code?: string; message?: string };
  message?: string;
};

export function cx(...items: Array<string | false | undefined>) {
  return items.filter(Boolean).join(" ");
}

export async function responseMessage(response: Response, fallback: string) {
  const payload = await response.json().catch(() => null) as ApiErrorPayload | null;
  return payload?.error?.message || payload?.message || fallback;
}

export function isJsonResponse(response: Response) {
  return response.headers.get("content-type")?.includes("application/json") ?? false;
}

export const assetLabels: Record<AdminAsset["media_type"], string> = {
  image: "图片",
  audio: "音频",
  video: "视频",
  live2d: "Live2D",
  font: "字体",
  other: "其他",
};

export function formatAssetSize(bytes: number) {
  return bytes >= 1024 * 1024
    ? `${(bytes / 1024 / 1024).toFixed(bytes >= 10 * 1024 * 1024 ? 0 : 1)} MB`
    : `${Math.max(1, Math.round(bytes / 1024))} KB`;
}
