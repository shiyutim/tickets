import { normalizeWechat, wechatProblem } from "./monitoring.js";

function persist(settings, values, storage, message) {
    try { storage.setItem("tickets.settings", JSON.stringify(values)); }
    catch { throw new Error(message); }
    Object.assign(settings, values);
}

function persistWechat(settings, value, storage) {
    const wechat = normalizeWechat(value);
    if (JSON.stringify(wechat) !== JSON.stringify(settings.wechat)) {
        persist(settings, { ...settings, wechat }, storage, "微信提醒设置无法保存到本机，修改尚未生效，请检查存储空间或权限后重试");
    }
    return wechat;
}

export function setWechatEnabled(settings, enabled, storage) {
    const wechat = normalizeWechat({ ...settings.wechat, enabled });
    const problem = wechatProblem(wechat);
    if (problem) throw new Error(problem);
    return persistWechat(settings, wechat, storage);
}

export function syncWechatConnection(settings, connection, storage) {
    return persistWechat(settings, {
        ...settings.wechat,
        enabled: connection.status === "unbound" ? false : settings.wechat.enabled,
        accountId: connection.accountId,
        target: connection.target,
    }, storage);
}

export async function taskWechatConfig(settings, loadConnection) {
    const saved = normalizeWechat(settings.wechat);
    if (!saved.enabled) return saved;
    try {
        const connection = await loadConnection();
        // Binding can change while the settings page is inactive. Read it for
        // this task without letting a delayed response overwrite saved settings.
        return normalizeWechat({ ...saved, accountId: connection.accountId, target: connection.target });
    } catch {
        // A notification status failure must not prevent a purchase. The backend
        // still validates monitor bindings and reports purchase delivery errors.
        return saved;
    }
}

export function persistGeneralSettings(settings, values, storage) {
    persist(settings, { ...settings, ...values, wechat: normalizeWechat(settings.wechat) }, storage,
        "设置无法保存到本机，修改尚未生效，请检查存储空间或权限后重试");
}

export async function testWechatNotification(settings, connection, send) {
    const config = normalizeWechat({ enabled: true, accountId: connection.accountId, target: connection.target });
    const problem = wechatProblem(config);
    if (problem) throw new Error(problem);
    await send(config);
    return settings.wechat.enabled
        ? "测试通知已发送，请在微信中确认收到。购票与余票提醒已启用，新任务将使用此设置。"
        : "测试通知已发送，请在微信中确认收到。自动提醒仍未启用，请打开上方的“启用微信购票与余票提醒”开关，打开后立即保存。";
}
