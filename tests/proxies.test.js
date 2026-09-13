import test from "node:test";
import assert from "node:assert/strict";
import { normalizeProxySettings, validateProxySettings, proxyAccount, proxyProblem } from "../src/services/proxies.js";

test("legacy proxy settings retain manual routing and disabled proxies stay direct", () => {
    const settings = normalizeProxySettings({ proxy: " socks5h://localhost:1080 " });
    validateProxySettings(settings);
    assert.equal(settings.proxyMode, "manual");
    assert.equal(proxyAccount(settings, true).proxy, "socks5h://localhost:1080");
    assert.deepEqual(proxyAccount(settings, false), { proxy: "", subscriptionId: "", subscriptionNodeId: "" });
});

test("subscription configuration rejects unsupported URLs, duplicate IDs and missing selection", () => {
    const settings = normalizeProxySettings({ subscriptions: [{ id: "a", name: "订阅", url: "https://example.com/sub" }], proxyMode: "subscription", subscriptionId: "a" });
    validateProxySettings(settings);
    assert.throws(() => validateProxySettings({ ...settings, subscriptionId: "missing" }), /请选择/);
    assert.throws(() => validateProxySettings({ ...settings, subscriptions: [...settings.subscriptions, ...settings.subscriptions] }), /重复/);
    assert.throws(() => validateProxySettings({ ...settings, subscriptions: [{ id: "a", name: "订阅", url: "file:///tmp/proxies" }] }), /HTTP/);
    assert.throws(() => validateProxySettings({ ...settings, proxy: "http://" }), /代理地址/);
});

test("a changed subscription URL or removed node cannot silently reuse old proxy routing", () => {
    const settings = normalizeProxySettings({ proxyMode: "subscription", subscriptionId: "a", subscriptionNodeId: "node", subscriptions: [{ id: "a", name: "订阅", url: "https://example.com/new" }] });
    const snapshot = { url: "https://example.com/old", nodes: [{ id: "node" }] };
    assert.match(proxyProblem(settings, { a: snapshot }), /刷新/);
    snapshot.url = settings.subscriptions[0].url;
    assert.equal(proxyProblem(settings, { a: snapshot }), "");
    snapshot.nodes = [{ id: "replacement" }];
    assert.match(proxyProblem(settings, { a: snapshot }), /重新选择/);
    assert.deepEqual(proxyAccount(settings, true), { proxy: "", subscriptionId: "a", subscriptionNodeId: "node" });
    settings.subscriptionNodeId = "";
    assert.equal(proxyProblem(settings, { a: snapshot }), "");
});
