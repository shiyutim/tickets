import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { computed, ref } from "vue";
import { persistGeneralSettings, setWechatEnabled, syncWechatConnection, taskWechatConfig, testWechatNotification } from "../src/services/wechatSettings.js";

const connection = { status: "ready", accountId: "my-bot", target: "user@im.wechat" };
const settings = (enabled = false) => ({ sound: true, proxy: "", wechat: { enabled, accountId: connection.accountId, target: connection.target } });
function storage() {
    const values = new Map();
    return { setItem: (key, value) => values.set(key, value), saved: () => JSON.parse(values.get("tickets.settings")) };
}

function deferred() {
    let resolve;
    const promise = new Promise(done => { resolve = done; });
    return { promise, resolve };
}

function settingsPage(state, call, onSync = () => {}, source = readFileSync(new URL("../src/components/WechatSettings.vue", import.meta.url), "utf8")) {
    const script = source.split("<script setup>")[1].split("</script>")[0].replace(/^import .*;\r?\n/gm, "");
    const noop = () => {};
    const disk = storage();
    const dependencies = {
        ref, computed, onMounted: noop, onActivated: noop, onDeactivated: noop, onBeforeUnmount: noop,
        QRCode: {}, call, desktop: true, errorText: String, runtime: { settings: state },
        saveWechatSettings: noop, testWechatNotification: noop, setTimeout: noop, clearTimeout: noop,
        syncWechatStatus(value) { syncWechatConnection(state, value, disk); onSync(value); },
    };
    return new Function(...Object.keys(dependencies), `${script}\nreturn { activate, deactivate, pollLogin, disconnect, session };`)(...Object.values(dependencies));
}

test("a successful test notification leaves automatic reminders disabled until the switch is saved", async () => {
    const state = settings();
    const disk = storage();
    const deliveries = [];
    const result = await testWechatNotification(state, connection, async config => deliveries.push(config));
    assert.match(result, /仍未启用/);
    assert.match(result, /上方/);
    assert.deepEqual(deliveries, [{ ...state.wechat, enabled: true }]);
    assert.equal(state.wechat.enabled, false);

    setWechatEnabled(state, true, disk);
    assert.equal(state.wechat.enabled, true);
    assert.equal(disk.saved().wechat.enabled, true);
    assert.equal(disk.saved().sound, true);

    assert.match(await testWechatNotification(state, connection, async () => {}), /提醒已启用/);
    setWechatEnabled(state, false, disk);
    assert.equal(disk.saved().wechat.enabled, false);
});

test("saving a stale general settings form preserves the independently saved switch and recipient", () => {
    const state = settings();
    const staleForm = structuredClone(state);
    staleForm.sound = false;
    const disk = storage();
    setWechatEnabled(state, true, disk);
    syncWechatConnection(state, { status: "ready", accountId: "new-bot", target: "new-user@im.wechat" }, disk);

    persistGeneralSettings(state, staleForm, disk);
    assert.deepEqual(state.wechat, { enabled: true, accountId: "new-bot", target: "new-user@im.wechat" });
    assert.deepEqual(disk.saved().wechat, state.wechat);
    assert.equal(disk.saved().sound, false);
});

test("binding synchronization persists the current recipient without implicitly enabling notifications", () => {
    const state = { sound: true, wechat: { enabled: false, accountId: "", target: "" } };
    const disk = storage();
    syncWechatConnection(state, connection, disk);
    assert.deepEqual(disk.saved().wechat, { enabled: false, accountId: "my-bot", target: "user@im.wechat" });
    setWechatEnabled(state, true, disk);
    syncWechatConnection(state, { ...connection, target: "another@im.wechat" }, disk);
    assert.deepEqual(disk.saved().wechat, { enabled: true, accountId: "my-bot", target: "another@im.wechat" });

    syncWechatConnection(state, { status: "unbound", accountId: "", target: "" }, disk);
    assert.deepEqual(state.wechat, { enabled: false, accountId: "", target: "" });
    assert.deepEqual(disk.saved().wechat, state.wechat);
});

test("storage errors leave the previous effective notification settings intact", () => {
    const brokenDisk = { setItem() { throw new Error("QuotaExceededError"); } };
    for (const enabled of [false, true]) {
        const state = settings(enabled);
        const before = structuredClone(state);
        assert.throws(() => setWechatEnabled(state, !enabled, brokenDisk), /修改尚未生效/);
        assert.deepEqual(state, before);
        assert.throws(() => syncWechatConnection(state, { ...connection, target: "new@im.wechat" }, brokenDisk), /无法保存/);
        assert.deepEqual(state, before);
        assert.throws(() => persistGeneralSettings(state, { sound: false, wechat: { enabled: !enabled } }, brokenDisk), /无法保存/);
        assert.deepEqual(state, before);
    }
});

test("an incomplete binding cannot enable reminders or send a test notification", async () => {
    const state = { wechat: { enabled: false, accountId: "my-bot", target: "" } };
    const disk = storage();
    let deliveries = 0;
    assert.throws(() => setWechatEnabled(state, true, disk), /通知会话/);
    assert.equal(state.wechat.enabled, false);
    await assert.rejects(testWechatNotification(state, state.wechat, async () => { deliveries++; }), /通知会话/);
    assert.equal(deliveries, 0);
});

test("failed test deliveries do not change the saved reminder preference", async () => {
    for (const enabled of [false, true]) {
        const state = settings(enabled);
        await assert.rejects(testWechatNotification(state, connection, async () => { throw new Error("send failed"); }), /send failed/);
        assert.equal(state.wechat.enabled, enabled);
    }
});

for (const lineEnding of ["LF", "CRLF"]) test(`a task reads the confirmed binding when leaving settings before the status refresh finishes (${lineEnding})`, async () => {
    const state = settings(true);
    const before = structuredClone(state);
    const confirmed = { status: "ready", accountId: "new-bot", target: "new-user@im.wechat" };
    const initialSync = deferred();
    const refreshStarted = deferred();
    const refresh = deferred();
    let current = connection;
    let statusReads = 0;
    const call = async command => {
        if (command === "get_wechat_status") {
            if (++statusReads === 1) return current;
            if (statusReads === 2) { refreshStarted.resolve(); return refresh.promise; }
            return current;
        }
        assert.equal(command, "poll_wechat_login");
        current = confirmed;
        return { sessionId: "qr", status: "confirmed", qrContent: "" };
    };
    const source = readFileSync(new URL("../src/components/WechatSettings.vue", import.meta.url), "utf8")
        .replace(/\r?\n/g, lineEnding === "CRLF" ? "\r\n" : "\n");
    const page = settingsPage(state, call, () => initialSync.resolve(), source);
    page.activate();
    await initialSync.promise;
    page.session.value = { sessionId: "qr", status: "wait", qrContent: "qr-content", expiresAt: Date.now() + 30_000 };
    const login = page.pollLogin();
    await refreshStarted.promise;
    page.deactivate();

    const task = await taskWechatConfig(state, () => call("get_wechat_status"));
    assert.deepEqual(task, { enabled: true, accountId: confirmed.accountId, target: confirmed.target });
    refresh.resolve(confirmed);
    await login;
    assert.deepEqual(state, before, "a late page response must not overwrite global settings");
});

test("late binding lookups cannot restore an account or reminder preference after disconnect", async () => {
    const state = settings(true);
    const response = deferred();
    const task = taskWechatConfig(state, () => response.promise);
    syncWechatConnection(state, { status: "unbound", accountId: "", target: "" }, storage());
    response.resolve(connection);
    await task;
    assert.deepEqual(state.wechat, { enabled: false, accountId: "", target: "" });

    let reads = 0;
    assert.deepEqual(await taskWechatConfig(state, async () => { reads++; return connection; }), state.wechat);
    assert.equal(reads, 0, "disabled reminders do not require a status request");
});

test("task configuration refreshes incomplete identities and preserves backend validation of missing bindings", async () => {
    const state = { wechat: { enabled: true, accountId: "", target: "" } };
    assert.deepEqual(await taskWechatConfig(state, async () => connection), {
        enabled: true, accountId: connection.accountId, target: connection.target,
    });
    const unbound = await taskWechatConfig(settings(true), async () => ({ status: "unbound", accountId: "", target: "" }));
    assert.deepEqual(unbound, { enabled: true, accountId: "", target: "" });
});

test("a status lookup failure leaves purchase creation possible without changing saved notification settings", async () => {
    const state = settings(true);
    const before = structuredClone(state);
    const config = await taskWechatConfig(state, async () => { throw new Error("IPC unavailable"); });
    assert.deepEqual(config, before.wechat);
    assert.deepEqual(state, before);
});
