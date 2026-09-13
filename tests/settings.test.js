import test from "node:test";
import assert from "node:assert/strict";
import { commitSettings, normalizeSettings, restoreLegacySettings } from "../src/services/settings.js";
import { setWechatEnabled, syncWechatConnection } from "../src/services/wechatSettings.js";
import { logTableForAppId } from "../src/sql/names.js";

const legacy = () => ({ proxy: "http://old.local:8000", appid: "old", appid_list: '[{"id":"old","desc":"旧记录"}]' });
function storage(initial) {
    let raw = initial === undefined ? null : JSON.stringify(initial);
    return {
        fail: false, writes: 0,
        getItem: () => raw,
        setItem(key, value) {
            assert.equal(key, "tickets.settings");
            this.writes++;
            if (this.fail) throw new Error("QuotaExceededError");
            raw = value;
        },
        saved: () => JSON.parse(raw),
    };
}
function restart(disk, database) {
    const saved = disk.saved();
    const current = normalizeSettings(saved);
    restoreLegacySettings(current, saved, database, disk);
    return current;
}

test("a failed settings commit preserves the effective proxy and log group across restart", () => {
    const database = legacy();
    const originalDatabase = structuredClone(database);
    const disk = storage(normalizeSettings({ proxy: database.proxy, appid: database.appid }));
    const current = restart(disk, database);
    const before = structuredClone(current);
    disk.fail = true;

    assert.throws(() => commitSettings(current, { ...current, proxy: "http://new.local:9000", appid: "new" }, disk), /修改尚未生效/);
    assert.deepEqual(current, before);
    assert.deepEqual(disk.saved(), before);
    assert.deepEqual(database, originalDatabase);
    const reloaded = restart(disk, database);
    assert.equal(reloaded.proxy, before.proxy);
    assert.equal(logTableForAppId(reloaded.appid), "old_LOG");
});

test("committed local settings remain authoritative over stale SQLite values on restart", () => {
    const database = legacy();
    const originalDatabase = structuredClone(database);
    const disk = storage();
    const current = restart(disk, database);
    commitSettings(current, { ...current, proxy: "socks5h://new.local:1080", appid: "new" }, disk);

    const reloaded = restart(disk, database);
    assert.equal(reloaded.proxy, "socks5h://new.local:1080");
    assert.equal(logTableForAppId(reloaded.appid), "new_LOG");
    assert.deepEqual(database, originalDatabase);

    commitSettings(current, { ...current, proxy: "", appid: "", appid_list: [] }, disk);
    const cleared = restart(disk, database);
    assert.equal(cleared.proxy, "");
    assert.equal(logTableForAppId(cleared.appid), "LOG");
    assert.deepEqual(cleared.appid_list, []);
});

test("SQLite-only settings migrate once and retain their log group", () => {
    const database = legacy();
    const originalDatabase = structuredClone(database);
    const disk = storage();
    const current = restart(disk, database);
    assert.equal(current.proxy, database.proxy);
    assert.deepEqual(current.appid_list, [{ id: "old", desc: "旧记录" }]);
    assert.equal(logTableForAppId(current.appid), "old_LOG");
    assert.deepEqual(disk.saved(), current);
    assert.equal(disk.writes, 1);
    assert.deepEqual(restart(disk, database), current);
    assert.equal(disk.writes, 1);
    assert.deepEqual(database, originalDatabase);
});

test("legacy migration fills only missing fields and preserves current notification settings", () => {
    const saved = { proxy: "", sound: false, wechat: { enabled: false, accountId: "bot", target: "user@im.wechat" } };
    const disk = storage(saved);
    const current = normalizeSettings(saved);
    // A binding refresh may finish while the legacy database is being read.
    setWechatEnabled(current, true, disk);
    syncWechatConnection(current, { status: "ready", accountId: "bot", target: "latest@im.wechat" }, disk);
    restoreLegacySettings(current, saved, legacy(), disk);

    assert.equal(current.proxy, "");
    assert.equal(current.appid, "old");
    assert.equal(current.sound, false);
    assert.deepEqual(disk.saved().wechat, { enabled: true, accountId: "bot", target: "latest@im.wechat" });
});

test("failed migration preserves the legacy source and restores its values before later saves", () => {
    const database = legacy();
    const originalDatabase = structuredClone(database);
    const disk = storage();
    const current = normalizeSettings();
    disk.fail = true;
    assert.throws(() => restoreLegacySettings(current, null, database, disk), /原数据库已保留/);
    assert.equal(disk.saved(), null);
    assert.deepEqual(database, originalDatabase);
    assert.equal(current.proxy, database.proxy);
    assert.equal(current.appid, database.appid);

    disk.fail = false;
    syncWechatConnection(current, { status: "ready", accountId: "bot", target: "user@im.wechat" }, disk);
    const reloaded = restart(disk, database);
    assert.equal(reloaded.proxy, database.proxy);
    assert.equal(logTableForAppId(reloaded.appid), "old_LOG");
    assert.deepEqual(reloaded.wechat, current.wechat);
});

test("automatic saves during a database outage do not hide legacy settings on the next startup", () => {
    const disk = storage();
    const current = normalizeSettings();
    // SQLite cannot be read this time, but the separate Wechat status request succeeds.
    syncWechatConnection(current, { status: "ready", accountId: "bot", target: "user@im.wechat" }, disk);
    assert.equal(disk.saved().proxy, "");

    const reloaded = restart(disk, legacy());
    assert.equal(reloaded.proxy, "http://old.local:8000");
    assert.equal(logTableForAppId(reloaded.appid), "old_LOG");
    assert.deepEqual(reloaded.legacySettingsPending, []);
    assert.deepEqual(reloaded.wechat, current.wechat);
});

test("an explicit settings save during a database outage takes precedence over legacy values", () => {
    const disk = storage();
    const current = normalizeSettings();
    commitSettings(current, { ...current, proxy: "", appid: "new" }, disk);
    const reloaded = restart(disk, legacy());
    assert.equal(reloaded.proxy, "");
    assert.equal(logTableForAppId(reloaded.appid), "new_LOG");
});

test("a stale general form cannot overwrite an independently committed Wechat preference", () => {
    const disk = storage();
    const current = normalizeSettings({ wechat: { enabled: false, accountId: "bot", target: "user@im.wechat" } });
    const form = structuredClone(current);
    setWechatEnabled(current, true, disk);
    syncWechatConnection(current, { status: "ready", accountId: "bot", target: "latest@im.wechat" }, disk);
    commitSettings(current, { ...form, sound: false, appid: "new" }, disk);
    assert.equal(disk.saved().sound, false);
    assert.equal(disk.saved().appid, "new");
    assert.deepEqual(disk.saved().wechat, { enabled: true, accountId: "bot", target: "latest@im.wechat" });
});

test("invalid settings fail before changing stored or effective settings", () => {
    const current = normalizeSettings();
    const disk = storage(current);
    const before = structuredClone(current);
    assert.throws(() => commitSettings(current, { ...current, proxy: "http://" }, disk), /代理地址/);
    assert.equal(disk.writes, 0);
    assert.deepEqual(disk.saved(), before);
    assert.deepEqual(current, before);
});
