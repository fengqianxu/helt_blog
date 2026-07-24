"use client";

import Image from "next/image";
import { FormEvent, useCallback, useEffect, useRef, useState } from "react";

import {
  AdminAsset,
  AdminIdentity,
  cx,
  DEFAULT_PROFILE_AVATAR_URL,
  Notify,
  responseMessage,
} from "./shared";

type PasskeyItem = { id: number; label: string; created_at: string };
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
  const [profileBilibiliUid, setProfileBilibiliUid] = useState(admin.bilibili_uid);
  const [profileError, setProfileError] = useState("");
  const [avatarAssetId, setAvatarAssetId] = useState<number | null>(admin.avatar_asset_id);
  const [avatarAssets, setAvatarAssets] = useState<AdminAsset[]>([]);
  const [avatarAssetsLoading, setAvatarAssetsLoading] = useState(false);
  const [avatarPickerOpen, setAvatarPickerOpen] = useState(false);
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

  const closeDialog = useCallback(() => {
    if (busy) return;
    if (dialog === "profile" && profileOriginal.current) {
      onAdminChange(profileOriginal.current);
    }
    setDialog(null);
    setCropAsset(null);
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
      setProfileBilibiliUid(admin.bilibili_uid);
      setAvatarAssetId(admin.avatar_asset_id);
      setAvatarCrop({
        x: admin.avatar_crop_x || 0,
        y: admin.avatar_crop_y || 0,
        zoom: admin.avatar_crop_zoom || 1,
      });
      setAvatarPickerOpen(false);
    }
    if (next === "passkey") setPasskeysLoading(true);
    setDialog(next);
  };

  const saveProfile = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (profileBilibiliUid && !/^\d{1,20}$/.test(profileBilibiliUid.trim())) {
      setProfileError("B 站 UID 只能包含数字，且不能超过 20 位。");
      return;
    }
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
          avatar_asset_id: avatarAssetId,
          avatar_crop_x: avatarCrop.x,
          avatar_crop_y: avatarCrop.y,
          avatar_crop_zoom: avatarCrop.zoom,
        }),
      });
      if (!response.ok) throw new Error(await responseMessage(response, "个人资料保存失败，请稍后重试。"));
      const updated = await response.json() as AdminIdentity;
      profileOriginal.current = updated;
      onAdminChange(updated);
      setDialog(null);
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
            <div className="admin-avatar-editor">
              <AdminProfileAvatar admin={admin} className="admin-avatar-preview" />
              <div><b>个人头像</b><span><button type="button" onClick={() => setAvatarPickerOpen((value) => !value)}>更换头像</button></span></div>
            </div>
            {avatarPickerOpen && <div className="admin-avatar-library" aria-label="从素材库选择头像">
              {avatarAssets.map((asset) => <button className={avatarAssetId === asset.id ? "selected" : ""} type="button" key={asset.id} title={asset.name} onClick={() => { setCropAsset(asset); setAvatarCrop({ x: 0, y: 0, zoom: 1 }); }}><Image src={asset.file.url} width={96} height={96} unoptimized alt={asset.name} /></button>)}
              {avatarAssetsLoading && <p>正在读取图片素材…</p>}
              {!avatarAssetsLoading && avatarAssets.length === 0 && <p>素材库暂无图片，请先到素材库新增图片。</p>}
            </div>}
            <div className="admin-profile-fields">
              <label>邮箱地址<input type="email" autoComplete="email" maxLength={254} value={profileEmail} onChange={(event) => setProfileEmail(event.target.value)} placeholder="name@example.com" /></label>
              <label>B 站 UID<input inputMode="numeric" pattern="[0-9]*" maxLength={20} value={profileBilibiliUid} onChange={(event) => setProfileBilibiliUid(event.target.value)} placeholder="例如：12345678" /></label>
            </div>
            {profileError && <div className="admin-account-error" role="alert">! {profileError}</div>}
            <footer><button type="button" onClick={closeDialog}>取消</button><button className="admin-primary" disabled={busy === "profile"}>{busy === "profile" ? "正在保存…" : "保存资料"}</button></footer>
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
