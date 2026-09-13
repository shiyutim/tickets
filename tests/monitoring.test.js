import test from "node:test";
import assert from "node:assert/strict";
import { monitorOptions, normalizeWechat, restoreMonitorForm, wechatProblem } from "../src/services/monitoring.js";

const form = () => ({ intervalSeconds: 60, maxChecks: 0, offsetMs: 0, scheduled: false, hasEnd: false });

test("a monitor opened after clock synchronization uses the current offset instead of the saved offset", () => {
    const saved = { intervalSeconds: 30, maxChecks: 10, offsetMs: -60000, scheduled: true, startAt: "2026-09-07T10:00:00" };
    for (const offsetMs of [120000, 0]) {
        const restored = restoreMonitorForm(saved, { offsetMs });
        const options = monitorOptions(restored, {}, Date.UTC(2026, 8, 6));
        assert.equal(options.offsetMs, offsetMs);
        assert.equal(options.intervalMs, 30000);
        assert.equal(options.maxAttempts, 10);
        assert.equal(options.startAt, Date.UTC(2026, 8, 7, 2));
    }
    assert.equal(saved.offsetMs, -60000);
});

test("a monitor keeps its manual offset when no clock sample is available", () => {
    assert.equal(restoreMonitorForm({ offsetMs: -60000 }).offsetMs, -60000);
    assert.equal(restoreMonitorForm().offsetMs, 0);
});

test("monitor accepts unlimited polling and uses Beijing time for its schedule", () => {
    const options = monitorOptions({ ...form(), scheduled: true, startAt: "2026-09-07T10:00:00", hasEnd: true, endAt: "2026-09-07T11:00:00" }, {}, Date.UTC(2026, 8, 6));
    assert.equal(options.maxAttempts, 0);
    assert.equal(options.intervalMs, 60000);
    assert.equal(options.startAt, Date.UTC(2026, 8, 7, 2));
    assert.equal(options.endAt - options.startAt, 3600000);
    assert.equal(options.wechat.enabled, false);
});

test("monitor rejects invalid intervals, limits and dates before starting", () => {
    for (const intervalSeconds of [0, 4, 3601, 5.1, "60", NaN]) assert.throws(() => monitorOptions({ ...form(), intervalSeconds }, {}), /间隔/);
    for (const maxChecks of [-1, 100001, 1.2]) assert.throws(() => monitorOptions({ ...form(), maxChecks }, {}), /次数/);
    assert.throws(() => monitorOptions({ ...form(), scheduled: true, startAt: "" }, {}), /开始时间/);
    assert.throws(() => monitorOptions({ ...form(), hasEnd: true, endAt: "2020-01-01T00:00" }, {}), /结束时间/);
    assert.throws(() => monitorOptions({ ...form(), offsetMs: Infinity }, {}), /修正时间/);
});

test("notification config requires both a bot account and an explicit WeChat recipient", () => {
    assert.equal(normalizeWechat(null).enabled, false);
    const config = { enabled: true, accountId: "my-bot", target: "user@im.wechat" };
    assert.equal(wechatProblem(config), "");
    for (const target of ["", "@im.wechat", "13800000000", "user\0@im.wechat", "user @im.wechat", "a@b@im.wechat"]) assert.match(wechatProblem({ ...config, target }), /通知会话/);
    for (const accountId of ["", "my bot", "my\0bot", "x".repeat(257)]) assert.match(wechatProblem({ ...config, accountId }), /扫码绑定/);
    assert.match(wechatProblem({ ...config, target: "" }), /通知会话/);
    assert.equal(wechatProblem({ enabled: false }), "");
});

test("legacy OpenClaw configuration requires a new direct binding and never carries credentials", () => {
    const legacy = { enabled: true, executable: "openclaw", accountId: "old-bot", target: "old-user@im.wechat" };
    assert.deepEqual(normalizeWechat(legacy), { enabled: true, accountId: "", target: "" });
    assert.match(wechatProblem(legacy), /扫码绑定/);
    assert.deepEqual(normalizeWechat({ enabled: true, accountId: " bot ", target: " user@im.wechat ", token: "secret", contextToken: "secret" }), { enabled: true, accountId: "bot", target: "user@im.wechat" });
});

test("monitor notification settings pin the selected account and recipient", () => {
    const config = { enabled: true, accountId: "bot-a", target: "user-a@im.wechat" };
    const options = monitorOptions(form(), config);
    config.accountId = "bot-b";
    config.target = "user-b@im.wechat";
    assert.deepEqual(options.wechat, { enabled: true, accountId: "bot-a", target: "user-a@im.wechat" });
});

test("monitor timing can be submitted while the cached WeChat recipient awaits refresh", () => {
    const options = monitorOptions(form(), { enabled: true, accountId: "bot", target: "" });
    assert.equal(options.wechat.enabled, true);
    assert.equal(options.intervalMs, 60000);
});
