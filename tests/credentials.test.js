import test from "node:test";
import assert from "node:assert/strict";
import { biliCredentials } from "../src/services/credentials.js";

test("Bilibili credentials use the user agent supplied for the current request", t => {
    t.mock.method(Date, "now", () => 1_700_000_000_000);
    globalThis.window = {
        scrollX: 0, scrollY: 0, innerWidth: 1200, innerHeight: 800,
        outerWidth: 1200, outerHeight: 800, screenX: 0, screenY: 0, devicePixelRatio: 1,
    };
    globalThis.screen = { width: 1920, height: 1080, availWidth: 1920 };
    globalThis.history = { length: 1 };
    t.after(() => { delete globalThis.window; delete globalThis.screen; delete globalThis.history; });

    const first = biliCredentials(123, "request-agent-one");
    const repeated = biliCredentials(123, "request-agent-one");
    const changed = biliCredentials(123, "a-different-request-agent");
    assert.deepEqual(first, repeated);
    assert.notEqual(first.ctoken, changed.ctoken);
    assert.equal(Buffer.from(first.ctoken, "base64").length, 32);
});
