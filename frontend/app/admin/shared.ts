export type Theme = "day" | "night";
export type Notify = (message: string, tone?: "normal" | "success" | "danger") => void;

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
