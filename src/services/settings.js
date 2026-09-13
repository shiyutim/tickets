import { normalizeProxySettings, validateProxySettings } from "./proxies.js";
import { normalizeTheme } from "./theme.js";
import { normalizeWechat } from "./monitoring.js";
import { persistGeneralSettings } from "./wechatSettings.js";

const legacyFields = ["proxy", "appid", "appid_list"];

export function normalizeSettings(value = {}) {
    const settings = value && typeof value === "object" && !Array.isArray(value) ? value : {};
    return {
        appid: "", appid_list: [], autoSync: true, sound: true, ...settings,
        ...normalizeProxySettings(settings), theme: normalizeTheme(settings.theme), wechat: normalizeWechat(settings.wechat),
        // Automatic Wechat saves can happen before SQLite becomes available.
        // Remember which defaults still need a legacy lookup across those saves.
        legacySettingsPending: Array.isArray(settings.legacySettingsPending)
            ? legacyFields.filter(key => settings.legacySettingsPending.includes(key))
            : legacyFields.filter(key => !Object.prototype.hasOwnProperty.call(settings, key)),
    };
}

export function commitSettings(current, values, storage) {
    const settings = normalizeSettings({ ...values, legacySettingsPending: [] });
    validateProxySettings(settings);
    // This is the only commit. SQLite stores logs and is only read for legacy settings.
    persistGeneralSettings(current, settings, storage);
    return current;
}

export function restoreLegacySettings(current, saved, legacy, storage) {
    const pending = normalizeSettings(saved).legacySettingsPending;
    if (!pending.length) return;
    const values = {};
    for (const key of pending) {
        if (legacy) values[key] = key === "appid_list" ? JSON.parse(legacy[key] || "[]") : legacy[key] || "";
    }
    // These values were already committed by an older version. Keep them usable
    // if migration fails; the source database is never modified or removed.
    Object.assign(current, normalizeSettings({ ...current, ...values, legacySettingsPending: [] }));
    try { persistGeneralSettings(current, current, storage); }
    catch { throw new Error(legacy ? "旧版设置已恢复，但暂时无法迁移保存；原数据库已保留，请检查本机存储后重试" : "设置暂时无法保存到本机，请检查本机存储后重试"); }
}
