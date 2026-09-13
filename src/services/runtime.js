import { reactive, computed } from "vue";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/api/shell";
import { Message, Notification } from "@arco-design/web-vue";
import { damaiCredentials, biliCredentials } from "./credentials";
import { initSettingTable, changeLogTableName, initLogTable, insert, select, selectAll, settingTableName, logTableName } from "../sql";
import successAudio from "../assets/success-audio.mp3";
import { applyTheme } from "./theme";
import { setWechatEnabled, syncWechatConnection, taskWechatConfig, testWechatNotification as sendWechatTest } from "./wechatSettings";
import { accountStorageKey, emptyAccounts, loadAccounts, upsertAccount, removeAccount, defaultAccount } from "./accounts";
import { normalizeSettings, commitSettings, restoreLegacySettings } from "./settings";

export const desktop = typeof window !== "undefined" && Boolean(window.__TAURI_IPC__);
export function readLocal(key, fallback) {
    try { return JSON.parse(localStorage.getItem(key)) ?? fallback; } catch { return fallback; }
}
export function saveLocal(key, value) {
    try { localStorage.setItem(key, JSON.stringify(value)); } catch { Message.warning("本地存储不可用，当前设置仅在本次运行中有效"); }
}

let initialAccounts = emptyAccounts();
let accountError = "";
try { initialAccounts = loadAccounts(localStorage); } catch { accountError = "账号读取或迁移失败，原数据已保留，请检查本机存储后重试"; }

const savedSettings = readLocal("tickets.settings", {});
export const runtime = reactive({
    accounts: initialAccounts, accountError,
    ready: false, tasks: {}, logs: [], clock: null, syncing: false, storageError: "", subscriptions: {},
    settings: normalizeSettings(savedSettings),
});
export const activeTasks = computed(() => Object.values(runtime.tasks).filter(isActive));
export const taskList = computed(() => Object.values(runtime.tasks).sort((a, b) => b.updatedAt - a.updatedAt));
export function isActive(task) { return task && ["waiting", "running"].includes(task.status); }
export const statusLabels = { waiting: "等待开始", found: "发现余票", completed: "监控结束", running: "执行中", succeeded: "待支付", failed: "未完成", cancelled: "已停止", needs_action: "需要处理", interrupted: "已中断" };
export function currentTask(platform) {
    const purchases = taskList.value.filter(task => task.platform === platform && task.mode !== "monitor");
    return purchases.find(isActive) || purchases[0];
}
export const monitorTasks = computed(() => taskList.value.filter(task => task.mode === "monitor"));
export const activeMonitors = computed(() => monitorTasks.value.filter(isActive));

export async function call(command, args) {
    if (!desktop) throw new Error("请在桌面应用中使用此功能：npm run tauri dev");
    return invoke(command, args);
}

export function errorText(error) { return error instanceof Error ? error.message : String(error); }

export async function openExternal(url) {
    try {
        const parsed = new URL(url);
        const allowed = ["damai.cn", "bilibili.com", "github.com"];
        if (parsed.protocol !== "https:" || !allowed.some(host => parsed.hostname === host || parsed.hostname.endsWith(`.${host}`))) throw new Error("无效的外部链接");
        if (desktop) await open(parsed.href);
        else window.open(parsed.href, "_blank", "noopener,noreferrer");
    } catch (error) { Message.error(errorText(error)); }
}

export function record(platform, title, status = "info", msg = "") {
    const entry = { time: Date.now(), type: platform, title, status, msg };
    runtime.logs.unshift(entry);
    runtime.logs = runtime.logs.slice(0, 500);
    if (desktop && !runtime.storageError) insert(logTableName, entry).catch(() => { runtime.storageError = "日志暂时无法写入数据库，本次记录仍可导出"; });
}

function acceptTask(task, notify = false) {
    const previous = runtime.tasks[task.id];
    if (previous && previous.revision >= task.revision) return;
    runtime.tasks[task.id] = task;
    saveLocal("tickets.tasks", taskList.value.slice(0, 50));
    if (notify) {
        record(task.platform, task.message, task.status);
        if (!isActive(task) && previous?.status !== task.status) {
            Notification[["succeeded", "found"].includes(task.status) ? "success" : task.status === "failed" ? "error" : "info"]({ title: statusLabels[task.status], content: task.message, duration: 7000 });
            if (["succeeded", "found"].includes(task.status) && runtime.settings.sound) new Audio(successAudio).play().catch(() => {});
        }
        if (task.notificationStatus === "failed" && previous?.notificationStatus !== "failed") {
            Notification.warning({ title: "微信 ClawBot 通知未确认送达", content: task.message, duration: 10000 });
        }
    }
}

export async function syncClock() {
    if (runtime.syncing) return;
    runtime.syncing = true;
    try {
        const sample = await call("sync_clock");
        runtime.clock = sample;
        record("system", "自动校时完成", "success", `${sample.source}，修正 ${sample.offsetMs} ms，往返 ${sample.roundTripMs} ms`);
        return sample;
    } catch (error) {
        record("system", "自动校时失败", "error", errorText(error));
        throw error;
    } finally { runtime.syncing = false; }
}

export async function startTask(request) {
    if (!runtime.ready) throw new Error("应用正在初始化，请稍后重试");
    const wechat = await taskWechatConfig(runtime.settings, () => call("get_wechat_status"));
    request = { ...request, config: { ...request.config, wechat } };
    const task = await call("start_ticket_task", { request });
    acceptTask(task);
    return task;
}

export async function stopTask(id) {
    try { await call("cancel_ticket_task", { id }); } catch (error) { Message.error(errorText(error)); }
}

function persistAccounts(state) {
    if (runtime.accountError) throw new Error(runtime.accountError);
    try { localStorage.setItem(accountStorageKey, JSON.stringify(state)); }
    catch { throw new Error("账号无法保存到本机，修改尚未生效，请检查存储空间或权限"); }
    runtime.accounts = state;
}

export function saveAccount(input) {
    persistAccounts(upsertAccount(runtime.accounts, input));
    record("system", "账号凭证已保存", "success");
}
export function deleteAccount(id) {
    persistAccounts(removeAccount(runtime.accounts, id));
    record("system", "账号凭证已删除", "info");
}
export function setDefaultAccount(id) { persistAccounts(defaultAccount(runtime.accounts, id)); }
export function reloadAccounts() {
    try { const state = loadAccounts(localStorage); runtime.accounts = state; runtime.accountError = ""; }
    catch { runtime.accountError = "账号读取或迁移失败，原数据已保留，请检查本机存储后重试"; }
}

export function saveWechatSettings(enabled) {
    const config = setWechatEnabled(runtime.settings, enabled, localStorage);
    record("system", enabled ? "微信购票与余票提醒已启用" : "微信购票与余票提醒已关闭", "success");
    return config;
}

export function syncWechatStatus(connection) {
    syncWechatConnection(runtime.settings, connection, localStorage);
}

export function testWechatNotification(connection) {
    return sendWechatTest(runtime.settings, connection, config => call("test_wechat_notification", { config }));
}

export async function saveSettings(settings) {
    settings = commitSettings(runtime.settings, settings, localStorage);
    applyTheme(runtime.settings.theme);
    changeLogTableName(settings.appid);
    if (desktop && !runtime.storageError) {
        try { await initLogTable(); }
        catch { runtime.storageError = "设置已保存，但日志数据库暂不可用，当前操作记录仍可在本次运行中导出"; }
    }
    for (const id of Object.keys(runtime.subscriptions)) {
        if (settings.subscriptions.some(source => source.id === id)) continue;
        try {
            if (desktop) await call("remove_subscription", { id });
            delete runtime.subscriptions[id];
        } catch (error) { Message.warning(`设置已保存，但订阅缓存清理失败：${errorText(error)}`); }
    }
    if (desktop && settings.proxyMode === "subscription") {
        const source = settings.subscriptions.find(item => item.id === settings.subscriptionId);
        const snapshot = runtime.subscriptions[source.id];
        if (!snapshot?.loading && (snapshot?.url !== source.url || !snapshot?.nodes?.length)) refreshSubscription(source).catch(() => {});
    }
    record("system", "全局设置已保存", "success");
}

export async function refreshSubscription(source) {
    const previous = runtime.subscriptions[source.id];
    if (previous?.loading) return;
    runtime.subscriptions[source.id] = { ...(previous?.url === source.url ? previous : {}), url: source.url, loading: true, error: "" };
    const state = runtime.subscriptions[source.id];
    try {
        const result = await call("refresh_subscription", { id: source.id, url: source.url });
        Object.assign(state, result);
        return result;
    } catch (error) {
        state.error = errorText(error);
        throw error;
    } finally { state.loading = false; }
}

let initialization;
export function initializeRuntime() {
    if (initialization) return initialization;
    initialization = (async () => {
        const saved = readLocal("tickets.tasks", []);
        for (const task of Array.isArray(saved) ? saved : []) {
            if (isActive(task)) { task.status = "interrupted"; task.message = "应用已重启，任务没有自动恢复，请重新确认后启动"; }
            if (task.notificationStatus === "pending") {
                task.notificationStatus = "unknown";
                task.message = `${task.message.replace(/；正在发送微信通知$/, "")}；上次微信通知的送达状态未知，请检查微信`;
            }
            runtime.tasks[task.id] = task;
        }
        if (desktop) {
            try {
                await initSettingTable();
                const settings = (await selectAll(settingTableName))[0];
                try { restoreLegacySettings(runtime.settings, savedSettings, settings, localStorage); }
                catch (error) { Message.warning(errorText(error)); }
                changeLogTableName(runtime.settings.appid); await initLogTable();
                runtime.logs = await select(`SELECT * FROM "${logTableName}" ORDER BY time DESC LIMIT 500`);
            } catch { runtime.storageError = "数据库暂不可用，当前操作记录仍可在本次运行中导出"; }
            try { syncWechatStatus(await call("get_wechat_status")); }
            catch (error) { Message.warning(`微信绑定状态同步失败：${errorText(error)}`); }
            await listen("ticket-task", event => acceptTask(event.payload, true));
            await listen("ticket-credentials", async ({ payload }) => {
                let credentials;
                try { credentials = payload.platform === "dm" ? await damaiCredentials() : biliCredentials(payload.projectId, payload.userAgent); }
                catch (error) { credentials = { error: errorText(error) }; }
                await call("provide_ticket_credentials", { requestId: payload.requestId, credentials }).catch(() => {});
            });
            for (const task of await call("list_ticket_tasks")) { delete runtime.tasks[task.id]; acceptTask(task); }
            const { appWindow } = await import("@tauri-apps/api/window");
            const { confirm } = await import("@tauri-apps/api/dialog");
            await appWindow.onCloseRequested(async event => {
                const hasNotifications = taskList.value.some(task => task.notificationStatus === "pending");
                if ((activeTasks.value.length || hasNotifications) && !await confirm(hasNotifications ? "仍有微信通知正在发送，退出应用会中断发送和运行中的任务。确定退出？" : "仍有购票或监控任务在运行，退出应用会停止这些任务。确定退出？", { title: "退出 Tickets", type: "warning" })) event.preventDefault();
            });
        }
        runtime.ready = true;
        if (desktop) {
            const source = runtime.settings.subscriptions.find(item => item.id === runtime.settings.subscriptionId);
            if (source) refreshSubscription(source).catch(() => {});
        }
        if (desktop && runtime.settings.autoSync) syncClock().catch(error => Message.warning(errorText(error)));
    })().catch(error => { runtime.storageError = errorText(error); throw error; });
    return initialization;
}
