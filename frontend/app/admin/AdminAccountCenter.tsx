"use client";

import Image from "next/image";
import { FormEvent, useCallback, useEffect, useRef, useState } from "react";

import {
  AboutProfile,
  AdminAsset,
  AdminIdentity,
  cx,
  DEFAULT_PROFILE_AVATAR_URL,
  Notify,
  responseMessage,
} from "./shared";

type PasskeyItem = { id: number; label: string; created_at: string };
const SOCIAL_ICON_ASSET_NAMES: Record<string, string> = {
  bilibili: "Bilibili 社交图标",
  "b站": "Bilibili 社交图标",
  哔哩哔哩: "Bilibili 社交图标",
  steam: "Steam 社交图标",
  github: "GitHub 社交图标",
  email: "Email 社交图标",
  "e-mail": "Email 社交图标",
  邮箱: "Email 社交图标",
  邮件: "Email 社交图标",
};
type PasskeyCreationOptionsJSON = {
  publicKey: Omit<PublicKeyCredentialCreationOptions, "challenge" | "user" | "excludeCredentials"> & {
    challenge: string;
    user: Omit<PublicKeyCredentialUserEntity, "id"> & { id: string };
    excludeCredentials?: Array<Omit<PublicKeyCredentialDescriptor, "id"> & { id: string }>;
  };
};

function decodeBase64Url(value: string) {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, "=");
  const binary = window.atob(padded);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

function encodeBase64Url(value: ArrayBuffer) {
  let binary = "";
  for (const byte of new Uint8Array(value)) binary += String.fromCharCode(byte);
  return window.btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function browserCreationOptions(payload: PasskeyCreationOptionsJSON): PublicKeyCredentialCreationOptions {
  return {
    ...payload.publicKey,
    challenge: decodeBase64Url(payload.publicKey.challenge),
    user: { ...payload.publicKey.user, id: decodeBase64Url(payload.publicKey.user.id) },
    excludeCredentials: payload.publicKey.excludeCredentials?.map((credential) => ({
      ...credential,
      id: decodeBase64Url(credential.id),
    })),
  };
}

function serializedPasskeyCredential(credential: PublicKeyCredential) {
  const response = credential.response as AuthenticatorAttestationResponse;
  return {
    id: credential.id,
    rawId: encodeBase64Url(credential.rawId),
    type: credential.type,
    response: {
      attestationObject: encodeBase64Url(response.attestationObject),
      clientDataJSON: encodeBase64Url(response.clientDataJSON),
      transports: typeof response.getTransports === "function" ? response.getTransports() : undefined,
    },
    extensions: credential.getClientExtensionResults(),
  };
}

export function AdminProfileAvatar({ admin, className }: { admin: AdminIdentity; className?: string }) {
  const avatarUrl = admin.avatar_url || DEFAULT_PROFILE_AVATAR_URL;
  return (
    <span className={cx("admin-profile-avatar", className)}>
      <Image
        src={avatarUrl}
        width={128}
        height={128}
        sizes="96px"
        unoptimized
        alt=""
        style={{
          objectPosition: `${50 + (admin.avatar_crop_x || 0) * 50}% ${50 + (admin.avatar_crop_y || 0) * 50}%`,
          transform: `scale(${admin.avatar_crop_zoom || 1})`,
        }}
      />
    </span>
  );
}

export function AdminAccountCenter({
  open,
  admin,
  onClose,
  onAdminChange,
  notify,
}: {
  open: boolean;
  admin: AdminIdentity;
  onClose: () => void;
  onAdminChange: (admin: AdminIdentity) => void;
  notify: Notify;
}) {
  const [dialog, setDialog] = useState<"profile" | "password" | "passkey" | null>(null);
  const [busy, setBusy] = useState<"profile" | "password" | "passkey" | "logout" | null>(null);
  const [currentPassword, setCurrentPassword] = useState("");
  const [newPassword, setNewPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [revokeOtherSessions, setRevokeOtherSessions] = useState(true);
  const [passwordError, setPasswordError] = useState("");
  const [profileEmail, setProfileEmail] = useState(admin.email);
  const [profileAbout, setProfileAbout] = useState<AboutProfile>(admin.about);
  const [profileTab, setProfileTab] = useState<"public" | "sync">("public");
  const [profileBilibiliUid, setProfileBilibiliUid] = useState(admin.bilibili_uid);
  const [profileSteamWebApiKey, setProfileSteamWebApiKey] = useState("");
  const [profileSteamId64, setProfileSteamId64] = useState(admin.steam_id64 ?? "");
  const [profileError, setProfileError] = useState("");
  const [avatarAssetId, setAvatarAssetId] = useState<number | null>(admin.avatar_asset_id);
  const [avatarAssets, setAvatarAssets] = useState<AdminAsset[]>([]);
  const [avatarAssetsLoading, setAvatarAssetsLoading] = useState(false);
  const [avatarPickerOpen, setAvatarPickerOpen] = useState(false);
  const [socialIconPicker, setSocialIconPicker] = useState<number | null>(null);
  const [cropAsset, setCropAsset] = useState<AdminAsset | null>(null);
  const [avatarCrop, setAvatarCrop] = useState({
    x: admin.avatar_crop_x || 0,
    y: admin.avatar_crop_y || 0,
    zoom: admin.avatar_crop_zoom || 1,
  });
  const cropDrag = useRef<{ pointerId: number; x: number; y: number; cropX: number; cropY: number } | null>(null);
  const [passkeyLabel, setPasskeyLabel] = useState("");
  const [passkeys, setPasskeys] = useState<PasskeyItem[]>([]);
  const [passkeysLoading, setPasskeysLoading] = useState(false);
  const [removingId, setRemovingId] = useState<number | null>(null);
  const profileOriginal = useRef<AdminIdentity | null>(null);
  const passkeySupported = typeof window !== "undefined"
    && "PublicKeyCredential" in window
    && Boolean(navigator.credentials);
  const steamWebApiKeyPresent = Boolean(profileSteamWebApiKey.trim());
  const steamId64Present = Boolean(profileSteamId64.trim());
  const steamPairComplete = !steamWebApiKeyPresent || steamId64Present;

  const closeDialog = useCallback(() => {
    if (busy) return;
    if (dialog === "profile" && profileOriginal.current) {
      onAdminChange(profileOriginal.current);
    }
    setDialog(null);
    setCropAsset(null);
    setSocialIconPicker(null);
    setCurrentPassword("");
    setNewPassword("");
    setConfirmPassword("");
    setPasswordError("");
    setProfileError("");
  }, [busy, dialog, onAdminChange]);

  useEffect(() => {
    if (!open && !dialog && !cropAsset) return;
    const close = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeDialog();
    };
    window.addEventListener("keydown", close);
    return () => window.removeEventListener("keydown", close);
  }, [closeDialog, cropAsset, dialog, open]);

  useEffect(() => {
    if (dialog !== "profile") return;
    const controller = new AbortController();
    void fetch("/api/v1/admin/assets?media_type=image&per_page=100&sort=uploaded_at&order=desc", {
      credentials: "include",
      headers: { accept: "application/json" },
      signal: controller.signal,
    })
      .then(async (response) => {
        if (!response.ok) throw new Error(await responseMessage(response, "无法读取图片素材。"));
        return response.json() as Promise<{ items: AdminAsset[] }>;
      })
      .then((payload) => setAvatarAssets(payload.items))
      .catch((error) => {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          setProfileError(error instanceof Error ? error.message : "无法读取图片素材。");
        }
      })
      .finally(() => setAvatarAssetsLoading(false));
    return () => controller.abort();
  }, [dialog]);

  useEffect(() => {
    if (dialog !== "passkey") return;
    const controller = new AbortController();
    void fetch("/api/v1/admin/auth/passkeys", {
      credentials: "include",
      headers: { accept: "application/json" },
      signal: controller.signal,
    })
      .then(async (response) => {
        if (!response.ok) throw new Error(await responseMessage(response, "无法读取已保存的 Passkey。"));
        return response.json() as Promise<{ items: PasskeyItem[] }>;
      })
      .then((payload) => setPasskeys(payload.items))
      .catch((error) => {
        if (!(error instanceof DOMException && error.name === "AbortError")) {
          notify(error instanceof Error ? error.message : "无法读取已保存的 Passkey。", "danger");
        }
      })
      .finally(() => setPasskeysLoading(false));
    return () => controller.abort();
  }, [dialog, notify]);

  const openDialog = (next: "profile" | "password" | "passkey") => {
    onClose();
    setPasswordError("");
    setProfileError("");
    if (next === "profile") {
      setAvatarAssetsLoading(true);
      profileOriginal.current = admin;
      setProfileEmail(admin.email);
      setProfileAbout(admin.about);
      setProfileTab("public");
      setProfileBilibiliUid(admin.bilibili_uid);
      setProfileSteamWebApiKey("");
      setProfileSteamId64(admin.steam_id64 ?? "");
      setAvatarAssetId(admin.avatar_asset_id);
      setAvatarCrop({
        x: admin.avatar_crop_x || 0,
        y: admin.avatar_crop_y || 0,
        zoom: admin.avatar_crop_zoom || 1,
      });
      setAvatarPickerOpen(false);
      setSocialIconPicker(null);
    }
    if (next === "passkey") setPasskeysLoading(true);
    setDialog(next);
  };

  const updateAbout = <Key extends keyof AboutProfile>(key: Key, value: AboutProfile[Key]) => {
    setProfileAbout((current) => ({ ...current, [key]: value }));
  };

  const updateSkill = (index: number, value: string) => {
    updateAbout("skills", profileAbout.skills.map((skill, candidate) => candidate === index ? value : skill));
  };

  const updateSocial = (index: number, key: "label" | "url", value: string) => {
    setProfileAbout((current) => ({
      ...current,
      socials: current.socials.map((social, candidate) => {
        if (candidate !== index) return social;
        if (key !== "label" || social.icon_asset_id) return { ...social, [key]: value };
        const assetName = SOCIAL_ICON_ASSET_NAMES[value.trim().toLowerCase()];
        const suggestedIcon = avatarAssets.find((asset) => asset.name === assetName);
        return {
          ...social,
          [key]: value,
          icon_asset_id: suggestedIcon?.id ?? null,
          icon_url: suggestedIcon?.file.url ?? null,
        };
      }),
    }));
  };

  const updateSocialIcon = (index: number, asset: AdminAsset | null) => {
    setProfileAbout((current) => ({
      ...current,
      socials: current.socials.map((social, candidate) => candidate === index
        ? { ...social, icon_asset_id: asset?.id ?? null, icon_url: asset?.file.url ?? null }
        : social),
    }));
    setSocialIconPicker(null);
  };

  const saveProfile = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (profileBilibiliUid && !/^\d{1,20}$/.test(profileBilibiliUid.trim())) {
      setProfileError("B 站 UID 只能包含数字，且不能超过 20 位。");
      return;
    }
    const steamWebApiKey = profileSteamWebApiKey.trim();
    const steamId64 = profileSteamId64.trim();
    if (steamWebApiKey && !/^[a-f\d]{32}$/i.test(steamWebApiKey)) {
      setProfileError("Steam Web API Key 应为 32 位十六进制字符。");
      return;
    }
    if (steamId64 && !/^7656119\d{10}$/.test(steamId64)) {
      setProfileError("请输入有效的 17 位 SteamID64。");
      return;
    }
    if (profileTab === "sync" && !steamPairComplete) {
      setProfileError("填写 Steam Web API Key 时必须同时填写 SteamID64。");
      return;
    }
    const incompleteSocial = profileAbout.socials.find((social) => Boolean(social.label.trim()) !== Boolean(social.url.trim()));
    if (incompleteSocial) {
      setProfileTab("public");
      setProfileError("每个社交链接都需要同时填写平台名称和完整地址。");
      return;
    }
    const normalizedAbout: AboutProfile = {
      ...profileAbout,
      version: 1,
      display_name: profileAbout.display_name.trim(),
      bio: profileAbout.bio.trim(),
      intro_md: profileAbout.intro_md.trim(),
      location: profileAbout.location.trim(),
      status: profileAbout.status.trim(),
      site_note: profileAbout.site_note.trim(),
      skills: profileAbout.skills.map((skill) => skill.trim()).filter(Boolean),
      socials: profileAbout.socials
        .map((social) => ({
          label: social.label.trim(),
          url: social.url.trim(),
          icon_asset_id: social.icon_asset_id ?? null,
        }))
        .filter((social) => social.label && social.url),
    };
    setBusy("profile");
    setProfileError("");
    try {
      const response = await fetch("/api/v1/admin/auth/profile", {
        method: "PATCH",
        credentials: "include",
        headers: { "content-type": "application/json", accept: "application/json" },
        body: JSON.stringify({
          email: profileEmail,
          bilibili_uid: profileBilibiliUid,
          steam_web_api_key: steamWebApiKey,
          steam_id64: steamId64,
          update_sync: profileTab === "sync",
          avatar_asset_id: avatarAssetId,
          avatar_crop_x: avatarCrop.x,
          avatar_crop_y: avatarCrop.y,
          avatar_crop_zoom: avatarCrop.zoom,
          about: normalizedAbout,
        }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "个人资料保存失败，请稍后重试。"));
      const updated = await response.json() as AdminIdentity;
      profileOriginal.current = updated;
      onAdminChange(updated);
      setProfileSteamWebApiKey("");
      setDialog(null);
      window.dispatchEvent(new Event("helt:profile-updated"));
      notify("个人资料已更新", "success");
    } catch (error) {
      setProfileError(error instanceof Error ? error.message : "个人资料保存失败，请稍后重试。");
    } finally {
      setBusy(null);
    }
  };

  const changePassword = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (newPassword.length < 12 || newPassword.length > 128) {
      setPasswordError("新密码长度须为 12–128 个字符。");
      return;
    }
    if (newPassword !== confirmPassword) {
      setPasswordError("两次输入的新密码不一致。");
      return;
    }
    setBusy("password");
    setPasswordError("");
    try {
      const response = await fetch("/api/v1/admin/auth/change-password", {
        method: "POST",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          current_password: currentPassword,
          new_password: newPassword,
          revoke_other_sessions: revokeOtherSessions,
        }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "密码修改失败，请稍后重试。"));
      sessionStorage.setItem(
        "helt-auth-message",
        revokeOtherSessions
          ? "密码已更新，所有设备需要重新登录。"
          : "密码已更新，请使用新密码重新登录。",
      );
      window.location.replace("/admin/login");
    } catch (error) {
      setPasswordError(error instanceof Error ? error.message : "密码修改失败，请稍后重试。");
      setBusy(null);
    }
  };

  const savePasskey = async () => {
    if (!passkeySupported || busy) return;
    setBusy("passkey");
    try {
      const optionsResponse = await fetch("/api/v1/admin/auth/passkeys/options", {
        method: "POST",
        credentials: "include",
        headers: { accept: "application/json" },
      });
      if (!optionsResponse.ok) throw new Error(await responseMessage(optionsResponse, "无法创建 Passkey 验证。"));
      const payload = await optionsResponse.json() as PasskeyCreationOptionsJSON;
      const credential = await navigator.credentials.create({
        publicKey: browserCreationOptions(payload),
      }) as PublicKeyCredential | null;
      if (!credential) throw new Error("浏览器没有返回有效的 Passkey。");

      const saveResponse = await fetch("/api/v1/admin/auth/passkeys", {
        method: "POST",
        credentials: "include",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          ...serializedPasskeyCredential(credential),
          label: passkeyLabel.trim() || "当前设备",
        }),
      });
      if (!saveResponse.ok) throw new Error(await responseMessage(saveResponse, "Passkey 保存失败。"));
      const item = await saveResponse.json() as PasskeyItem;
      setPasskeys((items) => [item, ...items]);
      setPasskeyLabel("");
      notify("Passkey 已安全保存", "success");
    } catch (error) {
      if (error instanceof DOMException && (error.name === "AbortError" || error.name === "NotAllowedError")) {
        notify("已取消保存 Passkey");
      } else {
        notify(error instanceof Error ? error.message : "Passkey 保存失败。", "danger");
      }
    } finally {
      setBusy(null);
    }
  };

  const removePasskey = async (item: PasskeyItem) => {
    if (!window.confirm(`移除“${item.label}”？移除后将无法再用它完成认证。`)) return;
    setRemovingId(item.id);
    try {
      const response = await fetch(`/api/v1/admin/auth/passkeys/${item.id}`, {
        method: "DELETE",
        credentials: "include",
      });
      if (!response.ok) throw new Error(await responseMessage(response, "Passkey 移除失败。"));
      setPasskeys((items) => items.filter((candidate) => candidate.id !== item.id));
      notify("Passkey 已移除", "success");
    } catch (error) {
      notify(error instanceof Error ? error.message : "Passkey 移除失败。", "danger");
    } finally {
      setRemovingId(null);
    }
  };

  const logout = async () => {
    if (busy) return;
    setBusy("logout");
    try {
      await fetch("/api/v1/admin/auth/logout", { method: "POST", credentials: "include" });
    } finally {
      window.location.replace("/admin/login");
    }
  };

  return (
    <>
      {open && (
        <>
          <button className="admin-account-dismiss" type="button" aria-label="关闭账户菜单" onClick={onClose} />
          <section className="admin-account-menu" role="menu" aria-label="管理员账户">
            <header className="admin-account-hero">
              <AdminProfileAvatar admin={admin} className="admin-account-avatar" />
              <div><span>唯一管理员</span><b>{admin.username}</b><small>{admin.email || "尚未设置联系邮箱"}</small></div>
            </header>
            <div className="admin-account-actions">
              <button type="button" role="menuitem" onClick={() => openDialog("profile")}>编辑个人资料</button>
              <button type="button" role="menuitem" onClick={() => openDialog("password")}>修改密码</button>
              <button type="button" role="menuitem" onClick={() => openDialog("passkey")}>通行密钥</button>
              <button className="danger" type="button" role="menuitem" onClick={logout} disabled={busy === "logout"}>{busy === "logout" ? "正在注销…" : "注销登录"}</button>
            </div>
          </section>
        </>
      )}

      {dialog === "profile" && (
        <div className="admin-account-dialog" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && closeDialog()}>
          <form className="admin-profile-dialog" onSubmit={saveProfile} role="dialog" aria-modal="true" aria-labelledby="profile-title">
            <header><div><span>ACCOUNT / PROFILE</span><h2 id="profile-title">个人资料</h2></div><button type="button" aria-label="关闭个人资料" onClick={closeDialog}>×</button></header>
            <nav className="admin-profile-tabs" aria-label="个人资料分区">
              <button type="button" className={profileTab === "public" ? "active" : ""} aria-current={profileTab === "public" ? "page" : undefined} onClick={() => setProfileTab("public")}>公开资料</button>
              <button type="button" className={profileTab === "sync" ? "active" : ""} aria-current={profileTab === "sync" ? "page" : undefined} onClick={() => setProfileTab("sync")}>内容同步</button>
              <span>{profileTab === "public" ? "保存后立即联动关于页" : "仅管理员可见"}</span>
            </nav>
            {profileTab === "public" ? <>
              <div className="admin-avatar-editor">
                <AdminProfileAvatar admin={admin} className="admin-avatar-preview" />
                <div><b>个人头像</b><small>关于页与后台共用同一头像和裁剪位置</small><span><button type="button" onClick={() => setAvatarPickerOpen((value) => !value)}>更换头像</button></span></div>
              </div>
              {avatarPickerOpen && <div className="admin-avatar-library" aria-label="从素材库选择头像">
                {avatarAssets.map((asset) => <button className={avatarAssetId === asset.id ? "selected" : ""} type="button" key={asset.id} title={asset.name} onClick={() => { setCropAsset(asset); setAvatarCrop({ x: 0, y: 0, zoom: 1 }); }}><Image src={asset.file.url} width={96} height={96} unoptimized alt={asset.name} /></button>)}
                {avatarAssetsLoading && <p>正在读取图片素材…</p>}
                {!avatarAssetsLoading && avatarAssets.length === 0 && <p>素材库暂无图片，请先到素材库新增图片。</p>}
              </div>}
              <div className="admin-profile-fields admin-public-profile-fields">
                <label>公开昵称<input maxLength={60} value={profileAbout.display_name} onChange={(event) => updateAbout("display_name", event.target.value)} placeholder={admin.username} /><small>留空时使用管理员用户名</small></label>
                <label>联系邮箱<input type="email" autoComplete="email" maxLength={254} value={profileEmail} onChange={(event) => setProfileEmail(event.target.value)} placeholder="name@example.com" /><small>留空时关于页不显示邮件入口</small></label>
                <label className="admin-profile-span">一句话简介<textarea rows={2} maxLength={160} value={profileAbout.bio} onChange={(event) => updateAbout("bio", event.target.value)} placeholder="你是谁，以及你关心什么" /><small>{profileAbout.bio.length}/160</small></label>
                <label>所在地<input maxLength={80} value={profileAbout.location} onChange={(event) => updateAbout("location", event.target.value)} placeholder="例如：Shanghai · China" /></label>
                <label>当前状态<input maxLength={80} value={profileAbout.status} onChange={(event) => updateAbout("status", event.target.value)} placeholder="例如：持续写作中" /></label>
                <label className="admin-profile-span">个人介绍（支持 Markdown）<textarea rows={6} maxLength={5000} value={profileAbout.intro_md} onChange={(event) => updateAbout("intro_md", event.target.value)} placeholder="写一段真正属于你的自我介绍…" /><small>{profileAbout.intro_md.length}/5000</small></label>
              </div>
              <section className="admin-profile-collection">
                <header><div><b>技能与兴趣</b><small>ABOUT / TAGS · 最多 12 项</small></div><button type="button" disabled={profileAbout.skills.length >= 12} onClick={() => updateAbout("skills", [...profileAbout.skills, ""])}>＋ 添加</button></header>
                <div className="admin-profile-skill-list">
                  {profileAbout.skills.map((skill, index) => <label key={index}><input aria-label={`技能与兴趣 ${index + 1}`} maxLength={40} value={skill} onChange={(event) => updateSkill(index, event.target.value)} placeholder="例如：Rust" /><button type="button" aria-label={`移除技能 ${skill || index + 1}`} onClick={() => updateAbout("skills", profileAbout.skills.filter((_, candidate) => candidate !== index))}>×</button></label>)}
                  {!profileAbout.skills.length && <p>还没有添加技能或兴趣，关于页会自动隐藏这一栏。</p>}
                </div>
              </section>
              <section className="admin-profile-collection">
                <header><div><b>社交链接</b><small>SOCIAL LINKS · 最多 8 个</small></div><button type="button" disabled={profileAbout.socials.length >= 8} onClick={() => updateAbout("socials", [...profileAbout.socials, { label: "", url: "", icon_asset_id: null }])}>＋ 添加</button></header>
                <div className="admin-profile-social-list">
                  {profileAbout.socials.map((social, index) => {
                    const selectedAsset = avatarAssets.find((asset) => asset.id === social.icon_asset_id);
                    const iconUrl = selectedAsset?.file.url || social.icon_url;
                    return <div className="admin-profile-social-row" key={index}>
                      <button className="admin-social-icon-trigger" type="button" aria-label={`更改 ${social.label || `社交链接 ${index + 1}`} 的图标`} aria-expanded={socialIconPicker === index} onClick={() => setSocialIconPicker((current) => current === index ? null : index)}>
                        {iconUrl ? <Image src={iconUrl} width={30} height={30} unoptimized alt="" /> : <span aria-hidden="true">＋</span>}
                      </button>
                      <input aria-label={`社交平台 ${index + 1}`} maxLength={30} value={social.label} onChange={(event) => updateSocial(index, "label", event.target.value)} placeholder="GitHub" />
                      <input aria-label={`社交链接 ${index + 1}`} type="url" maxLength={2048} value={social.url} onChange={(event) => updateSocial(index, "url", event.target.value)} placeholder="https://github.com/…" />
                      <button className="admin-social-remove" type="button" aria-label={`移除社交链接 ${social.label || index + 1}`} onClick={() => updateAbout("socials", profileAbout.socials.filter((_, candidate) => candidate !== index))}>×</button>
                      {socialIconPicker === index && <div className="admin-social-icon-picker" aria-label="从素材库选择社交图标">
                        <header><span>从图片素材中选择图标</span><button type="button" onClick={() => updateSocialIcon(index, null)}>不使用图标</button></header>
                        <div>
                          {avatarAssets.map((asset) => <button className={social.icon_asset_id === asset.id ? "selected" : ""} type="button" key={asset.id} title={asset.name} aria-label={`使用 ${asset.name}`} onClick={() => updateSocialIcon(index, asset)}><Image src={asset.file.url} width={44} height={44} unoptimized alt="" /></button>)}
                        </div>
                        {avatarAssetsLoading && <p>正在读取图片素材…</p>}
                        {!avatarAssetsLoading && !avatarAssets.length && <p>素材库暂无图片，请先到素材库新增图片。</p>}
                      </div>}
                    </div>;
                  })}
                  {!profileAbout.socials.length && <p>添加后会以可访问的外部链接显示在资料卡上。</p>}
                </div>
              </section>
              <label className="admin-profile-site-note">关于本站<textarea rows={4} maxLength={2000} value={profileAbout.site_note} onChange={(event) => updateAbout("site_note", event.target.value)} placeholder="说说这个站点的来历、设计或正在发生的变化…" /><small>{profileAbout.site_note.length}/2000</small></label>
            </> : <section className="admin-profile-sync-panel">
              <div><span>PRIVATE CONNECTIONS</span><h3>内容同步账号</h3><p>这些凭据只用于拉取你的追番和游戏数据，不会出现在关于页或公开接口中。</p></div>
              <div className="admin-profile-fields">
                <label>B 站 UID<input inputMode="numeric" pattern="[0-9]*" maxLength={20} value={profileBilibiliUid} onChange={(event) => setProfileBilibiliUid(event.target.value)} placeholder="例如：12345678" /></label>
                <label>SteamID64<input inputMode="numeric" pattern="[0-9]*" maxLength={17} required={steamWebApiKeyPresent} aria-invalid={steamWebApiKeyPresent && !steamId64Present} value={profileSteamId64} onChange={(event) => setProfileSteamId64(event.target.value)} placeholder="例如：76561198000000000" /></label>
                <label className="admin-profile-span">Steam Web API Key<input type="password" autoComplete="new-password" spellCheck={false} maxLength={32} aria-invalid={steamWebApiKeyPresent && !steamId64Present} value={profileSteamWebApiKey} onChange={(event) => setProfileSteamWebApiKey(event.target.value)} placeholder={admin.steam_web_api_key_configured ? "留空并保存即可移除" : "32 位 Web API Key"} /><small>{admin.steam_web_api_key_configured ? `${admin.steam_web_api_key_masked} · 已加密保存；留空会移除` : "尚未配置；Key 与 SteamID64 需一起填写"}</small></label>
              </div>
            </section>}
            {profileError && <div className="admin-account-error" role="alert">! {profileError}</div>}
            <footer><button type="button" onClick={closeDialog}>取消</button><button className="admin-primary" disabled={busy === "profile" || (profileTab === "sync" && !steamPairComplete)}>{busy === "profile" ? "正在保存…" : "保存资料"}</button></footer>
          </form>
        </div>
      )}

      {cropAsset && (
        <div className="admin-account-dialog" role="presentation">
          <section className="admin-avatar-crop-dialog" role="dialog" aria-modal="true" aria-labelledby="avatar-crop-title">
            <header><div><span>PROFILE / AVATAR</span><h2 id="avatar-crop-title">框定头像范围</h2></div><button type="button" aria-label="关闭头像裁剪" onClick={() => setCropAsset(null)}>×</button></header>
            <div className="admin-avatar-cropper">
              <div
                className="admin-avatar-crop-stage"
                onPointerDown={(event) => {
                  event.currentTarget.setPointerCapture(event.pointerId);
                  cropDrag.current = { pointerId: event.pointerId, x: event.clientX, y: event.clientY, cropX: avatarCrop.x, cropY: avatarCrop.y };
                }}
                onPointerMove={(event) => {
                  const drag = cropDrag.current;
                  if (!drag || drag.pointerId !== event.pointerId) return;
                  const bounds = event.currentTarget.getBoundingClientRect();
                  setAvatarCrop((value) => ({
                    ...value,
                    x: Math.max(-1, Math.min(1, drag.cropX - (event.clientX - drag.x) / bounds.width * 2)),
                    y: Math.max(-1, Math.min(1, drag.cropY - (event.clientY - drag.y) / bounds.height * 2)),
                  }));
                }}
                onPointerUp={() => { cropDrag.current = null; }}
                onPointerCancel={() => { cropDrag.current = null; }}
              >
                <Image src={cropAsset.file.url} width={640} height={640} unoptimized draggable={false} alt="" style={{ width: "100%", height: "100%", objectPosition: `${50 + avatarCrop.x * 50}% ${50 + avatarCrop.y * 50}%`, transform: `translate(-50%, -50%) scale(${avatarCrop.zoom})` }} />
                <span aria-hidden="true" />
              </div>
              <div className="admin-avatar-crop-controls">
                <label><span>缩放</span><input type="range" min="1" max="3" step="0.01" value={avatarCrop.zoom} onChange={(event) => setAvatarCrop((value) => ({ ...value, zoom: Number(event.target.value) }))} /></label>
                <div><button type="button" onClick={() => setAvatarCrop({ x: 0, y: 0, zoom: 1 })}>居中</button></div>
              </div>
            </div>
            <footer><button type="button" onClick={() => setCropAsset(null)}>取消</button><button className="admin-primary" type="button" onClick={() => {
              setAvatarAssetId(cropAsset.id);
              onAdminChange({
                ...(profileOriginal.current ?? admin),
                avatar_url: cropAsset.file.url,
                avatar_asset_id: cropAsset.id,
                avatar_crop_x: avatarCrop.x,
                avatar_crop_y: avatarCrop.y,
                avatar_crop_zoom: avatarCrop.zoom,
              });
              setCropAsset(null);
              setAvatarPickerOpen(false);
            }}>确定</button></footer>
          </section>
        </div>
      )}

      {dialog === "password" && (
        <div className="admin-account-dialog" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && closeDialog()}>
          <form onSubmit={changePassword} role="dialog" aria-modal="true" aria-labelledby="change-password-title">
            <header><div><span>SECURITY / CREDENTIALS</span><h2 id="change-password-title">修改密码</h2></div><button type="button" aria-label="关闭修改密码" onClick={closeDialog}>×</button></header>
            <p>更新密码后，当前设备会立即退出登录。</p>
            <label>当前密码<input type="password" autoComplete="current-password" value={currentPassword} onChange={(event) => setCurrentPassword(event.target.value)} autoFocus required /></label>
            <label>新密码<input type="password" autoComplete="new-password" minLength={12} maxLength={128} value={newPassword} onChange={(event) => setNewPassword(event.target.value)} required /><small>12–128 个字符</small></label>
            <label>确认新密码<input type="password" autoComplete="new-password" minLength={12} maxLength={128} value={confirmPassword} onChange={(event) => setConfirmPassword(event.target.value)} required /></label>
            <label className="admin-session-option"><input type="checkbox" checked={revokeOtherSessions} onChange={(event) => setRevokeOtherSessions(event.target.checked)} /><span><b>撤销其他设备会话</b><small>同时让其他设备保存的七日登录凭据失效。</small></span></label>
            {passwordError && <div className="admin-account-error" role="alert">! {passwordError}</div>}
            <footer><button type="button" onClick={closeDialog}>取消</button><button className="admin-primary" disabled={busy === "password"}>{busy === "password" ? "正在更新…" : "确认修改"}</button></footer>
          </form>
        </div>
      )}

      {dialog === "passkey" && (
        <div className="admin-account-dialog" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && closeDialog()}>
          <section className="admin-passkey-dialog" role="dialog" aria-modal="true" aria-labelledby="passkey-title">
            <header><div><span>SECURITY / PASSKEY</span><h2 id="passkey-title">通行密钥</h2></div><button type="button" aria-label="关闭通行密钥" onClick={closeDialog}>×</button></header>
            <p>把登录凭据保存在设备或密码管理器中。指纹和面容数据不会发送到本站。</p>
            <div className="passkey-enroll">
              <label>设备名称<input value={passkeyLabel} maxLength={80} onChange={(event) => setPasskeyLabel(event.target.value)} placeholder="例如：工作电脑 · Windows Hello" /></label>
              <button className="admin-primary" type="button" onClick={savePasskey} disabled={!passkeySupported || busy === "passkey"}>{busy === "passkey" ? "等待系统验证…" : "＋ 保存到此设备"}</button>
              {!passkeySupported && <small>当前浏览器不支持 Passkey，请使用新版 Chrome、Edge 或 Safari。</small>}
            </div>
            <div className="passkey-list">
              <h3>已保存 <span>{passkeys.length}</span></h3>
              {passkeysLoading ? <div className="passkey-empty">正在读取凭据…</div> : passkeys.length ? passkeys.map((item) => (
                <article key={item.id}><i aria-hidden="true">⌁</i><div><b>{item.label}</b><small>添加于 {new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium" }).format(new Date(item.created_at))}</small></div><button type="button" onClick={() => removePasskey(item)} disabled={removingId === item.id}>{removingId === item.id ? "…" : "移除"}</button></article>
              )) : <div className="passkey-empty">尚未保存 Passkey</div>}
            </div>
          </section>
        </div>
      )}
    </>
  );
}
