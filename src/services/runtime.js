import { reactive, computed } from "vue";
import { invoke } from "@tauri-apps/api/tauri";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/api/shell";
import { Message, Notification } from "@arco-design/web-vue";
import { damaiCredentials, biliCredentials } from "./credentials";
import { initSettingTable, changeLogTableName, initLogTable, insert, select, selectAll, settingTableName, logTableName, update } from "../sql";
import successAudio from "../assets/success-audio.mp3";

export const desktop = typeof window !== "undefined" && Boolean(window.__TAURI_IPC__);
export function readLocal(key, fallback) {
    try { return JSON.parse(localStorage.getItem(key)) ?? fallback; } catch { return fallback; }
}
export function saveLocal(key, value) {
    try { localStorage.setItem(key, JSON.stringify(value)); } catch { Message.warning("本地存储不可用，当前设置仅在本次运行中有效"); }
}

export const runtime = reactive({
    ready: false, tasks: {}, logs: [], clock: null, syncing: false, storageError: "",
    settings: { proxy: "", appid: "", appid_list: [], autoSync: true, sound: true, ...readLocal("tickets.settings", {}) },
});
export const activeTasks = computed(() => Object.values(runtime.tasks).filter(isActive));
export const taskList = computed(() => Object.values(runtime.tasks).sort((a, b) => b.updatedAt - a.updatedAt));
export function isActive(task) { return task && ["waiting", "running"].includes(task.status); }
export const statusLabels = { waiting: "等待开售", running: "执行中", succeeded: "待支付", failed: "未完成", cancelled: "已停止", needs_action: "需要处理", interrupted: "已中断" };
export function currentTask(platform) { return taskList.value.find(task => task.platform === platform); }

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
            Notification[task.status === "succeeded" ? "success" : task.status === "failed" ? "error" : "info"]({ title: statusLabels[task.status], content: task.message, duration: 7000 });
            if (task.status === "succeeded" && runtime.settings.sound) new Audio(successAudio).play().catch(() => {});
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
    const task = await call("start_ticket_task", { request });
    acceptTask(task);
    return task;
}

export async function stopTask(id) {
    try { await call("cancel_ticket_task", { id }); } catch (error) { Message.error(errorText(error)); }
}

export async function saveSettings(settings) {
    if (settings.proxy && !/^(https?|socks5h?):\/\/[^\s]+$/.test(settings.proxy)) throw new Error("请输入有效的代理地址");
    if (desktop && !runtime.storageError) {
        const rows = await selectAll(settingTableName);
        const values = { proxy: settings.proxy, appid: settings.appid, appid_list: settings.appid_list };
        if (rows.length) await update(settingTableName, values); else await insert(settingTableName, values);
    }
    Object.assign(runtime.settings, settings);
    saveLocal("tickets.settings", runtime.settings);
    if (desktop && !runtime.storageError) { await changeLogTableName(); await initLogTable(); }
    record("system", "全局设置已保存", "success");
}

let initialization;
export function initializeRuntime() {
    if (initialization) return initialization;
    initialization = (async () => {
        const saved = readLocal("tickets.tasks", []);
        for (const task of Array.isArray(saved) ? saved : []) {
            if (isActive(task)) { task.status = "interrupted"; task.message = "应用已重启，任务没有自动恢复，请重新确认后启动"; }
            runtime.tasks[task.id] = task;
        }
        if (desktop) {
            try {
                await initSettingTable(); await changeLogTableName(); await initLogTable();
                const settings = (await selectAll(settingTableName))[0];
                if (settings) Object.assign(runtime.settings, { proxy: settings.proxy || "", appid: settings.appid || "", appid_list: JSON.parse(settings.appid_list || "[]") });
                runtime.logs = await select(`SELECT * FROM "${logTableName}" ORDER BY time DESC LIMIT 500`);
            } catch { runtime.storageError = "数据库暂不可用，当前操作记录仍可在本次运行中导出"; }
            await listen("ticket-task", event => acceptTask(event.payload, true));
            await listen("ticket-credentials", async ({ payload }) => {
                let credentials;
                try { credentials = payload.platform === "dm" ? await damaiCredentials() : biliCredentials(payload.projectId); }
                catch (error) { credentials = { error: errorText(error) }; }
                await call("provide_ticket_credentials", { requestId: payload.requestId, credentials }).catch(() => {});
            });
            for (const task of await call("list_ticket_tasks")) { delete runtime.tasks[task.id]; acceptTask(task); }
            const { appWindow } = await import("@tauri-apps/api/window");
            const { confirm } = await import("@tauri-apps/api/dialog");
            await appWindow.onCloseRequested(async event => {
                if (activeTasks.value.length && !await confirm("仍有购票任务在运行，退出应用会停止这些任务。确定退出？", { title: "退出 Tickets", type: "warning" })) event.preventDefault();
            });
        }
        runtime.ready = true;
        if (desktop && runtime.settings.autoSync) syncClock().catch(error => Message.warning(errorText(error)));
    })().catch(error => { runtime.storageError = errorText(error); throw error; });
    return initialization;
}
