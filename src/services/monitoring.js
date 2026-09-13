import { timestamp } from "./platforms.js";

export function restoreMonitorForm(saved = {}, clock = null) {
    return {
        intervalSeconds: 60, maxChecks: 0, scheduled: false, startAt: "", hasEnd: false, endAt: "",
        ...saved,
        offsetMs: clock?.offsetMs ?? saved?.offsetMs ?? 0,
    };
}

export function normalizeWechat(value = {}) {
    const legacy = value != null && Object.hasOwn(value, "executable");
    return {
        enabled: value?.enabled === true,
        target: legacy ? "" : String(value?.target || "").trim(),
        accountId: legacy ? "" : String(value?.accountId || "").trim(),
    };
}

export function wechatProblem(value) {
    const config = normalizeWechat(value);
    if (!config.enabled) return "";
    if (!config.accountId || /[\s\x00-\x1f\x7f]/.test(config.accountId) || config.accountId.length > 256) return "请在设置中扫码绑定微信";
    if (!/^[^\s@]+@im\.wechat$/.test(config.target) || /[\x00-\x1f\x7f]/.test(config.target) || config.target.length > 256) return "请先在微信中向 Bot 发一条消息，建立通知会话";
    return "";
}

export function monitorOptions(form, wechat, now = Date.now()) {
    if (!Number.isInteger(form.intervalSeconds) || form.intervalSeconds < 5 || form.intervalSeconds > 3600) throw new Error("查询间隔须为 5–3600 秒的整数");
    if (!Number.isInteger(form.maxChecks) || form.maxChecks < 0 || form.maxChecks > 100000) throw new Error("查询次数须为 0–100000 的整数，0 表示不限次数");
    if (!Number.isInteger(form.offsetMs) || Math.abs(form.offsetMs) > 86400000) throw new Error("修正时间须为 ±86400000 毫秒内的整数");
    const startAt = form.scheduled ? timestamp(form.startAt) : 0;
    const endAt = form.hasEnd ? timestamp(form.endAt) : 0;
    if (form.scheduled && !startAt) throw new Error("请选择有效的监控开始时间");
    if (form.hasEnd && (!endAt || endAt <= Math.max(startAt, now + form.offsetMs))) throw new Error("结束时间必须晚于开始时间和当前时间");
    const config = normalizeWechat(wechat);
    // 提交任务时从后端获取当前绑定，页面缓存中的接收会话可能已经过期。
    return { startAt, endAt, intervalMs: form.intervalSeconds * 1000, maxAttempts: form.maxChecks, offsetMs: form.offsetMs, wechat: config };
}
