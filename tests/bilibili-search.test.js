import test from "node:test";
import assert from "node:assert/strict";
import { BILI_SEARCH_PAGE_SIZE, normalizeBiliSearch } from "../src/services/bilibiliSearch.js";

const event = overrides => ({
    id: 1005826, ticket_id: 1005826,
    project_name: "上海·中秋瑶娥Cosplay游园会", title: "上海·中秋瑶娥Cosplay游园会",
    cover: "https://i1.hdslb.com/bfs/openplatform/202608/xzkWnh6B1787743555_1787743555819.jpg",
    url: "https://mall.bilibili.com/neul-next/ticket-renovation/detail.html?id=1005826",
    city: "上海市", venue_name: "上海月星环球港", tlabel: "2026.09.25 - 09.26",
    price_low: 2800, price_high: 18000, is_free: false, is_price: true, sale_flag: "预售中",
    ...overrides,
});

test("Bilibili search maps the real result response and converts cents to yuan", () => {
    const raw = { result: [event()], total: 21, numResults: null, pagesize: null, page: null, numPages: null, isLastBrush: false, seid: null };
    assert.deepEqual(normalizeBiliSearch(raw), {
        items: [{
            id: "1005826", name: "上海·中秋瑶娥Cosplay游园会",
            image: event().cover, venue: "上海市 · 上海月星环球港", date: "2026.09.25 - 09.26",
            price: "¥28 起", status: "预售中",
        }],
        total: 21, hasMore: true,
    });
    assert.equal(normalizeBiliSearch(raw, 2).hasMore, false);
    assert.equal(BILI_SEARCH_PAGE_SIZE, 20);
});

test("empty results require an array or explicit zero total with null result", () => {
    assert.deepEqual(normalizeBiliSearch({ result: [], total: 0 }), { items: [], total: 0, hasMore: false });
    assert.deepEqual(normalizeBiliSearch({ result: null, total: 0 }), { items: [], total: 0, hasMore: false });
    assert.deepEqual(normalizeBiliSearch({ result: [] }), { items: [], total: null, hasMore: false });
    for (const raw of [undefined, null, [], "bad", 0, {}, { total: 0 }, { result: null }, { result: null, total: 1 }, { result: null, total: "0" }, { result: {} }, { result: "[]" }, { list: [] }]) {
        assert.throws(() => normalizeBiliSearch(raw), /搜索结果格式异常/);
    }
});

test("search filters malformed and unsafe IDs and deduplicates numeric and string IDs", () => {
    const result = [event(), event({ id: "1005826", project_name: "重复" }), event({ id: null, ticket_id: "123" })];
    for (const id of [0, -1, 1.5, true, {}, [], "", "01", "-1", "1.2", "1e2", "123&other=4", "javascript:alert(1)", Number.MAX_SAFE_INTEGER + 1, "9007199254740992"]) {
        result.push(event({ id }));
    }
    result.push(null, false, [], "bad", {});
    assert.deepEqual(normalizeBiliSearch({ result }).items.map(item => item.id), ["1005826", "123"]);
});

test("search images accept only HTTP(S) URLs and upgrade HTTP", () => {
    const result = [event({ id: 1, cover: "http://i0.hdslb.com/image.jpg" }), event({ id: 2, cover: " https://i1.hdslb.com/image.png " })];
    for (const [index, cover] of ["javascript:alert(1)", "data:image/svg+xml,<svg/>", "file:///tmp/image.png", "//i0.hdslb.com/image.jpg", "/image.jpg", "https://user:secret@example.com/image.jpg", {}, null].entries()) {
        result.push(event({ id: index + 3, cover }));
    }
    const images = normalizeBiliSearch({ result }).items.map(item => item.image);
    assert.deepEqual(images.slice(0, 2), ["https://i0.hdslb.com/image.jpg", "https://i1.hdslb.com/image.png"]);
    assert.ok(images.slice(2).every(value => value === ""));
});

test("search display fields remain plain strings without object coercion", () => {
    const [item] = normalizeBiliSearch({ result: [event({ project_name: {}, title: " <b>活动</b> ", city: {}, venue_name: " 场馆 ", tlabel: [], sale_flag: {} })] }).items;
    assert.equal(item.name, "<b>活动</b>");
    assert.equal(item.venue, "场馆");
    assert.equal(item.date, "");
    assert.equal(item.status, "");
    assert.equal(normalizeBiliSearch({ result: [event({ project_name: null, title: 123 })] }).items[0].name, "会员购活动");
});

test("search prices distinguish free, unknown, fixed and starting prices", () => {
    const cases = [
        [{ is_free: true, price_low: 0, price_high: 0 }, "免费"],
        [{ is_price: false }, "票价待公布"],
        [{ price_low: null }, "票价待公布"],
        [{ price_low: -100 }, "票价待公布"],
        [{ price_low: 1.2 }, "票价待公布"],
        [{ price_low: Infinity }, "票价待公布"],
        [{ price_low: true }, "票价待公布"],
        [{ price_low: 1990, price_high: 9800 }, "¥19.9 起"],
        [{ price_low: 2801, price_high: 2801 }, "¥28.01"],
        [{ price_low: 18000, price_high: 18000 }, "¥180"],
        [{ price_low: 10000, price_high: undefined }, "¥100 起"],
        [{ price_low: 0, price_high: 0 }, "¥0"],
    ];
    for (const [overrides, expected] of cases) {
        assert.equal(normalizeBiliSearch({ result: [event(overrides)] }).items[0].price, expected);
    }
});

test("search pagination uses total before the server flag and ignores nullable metadata", () => {
    assert.equal(normalizeBiliSearch({ result: [event()], total: 21, isLastBrush: true }).hasMore, true);
    assert.equal(normalizeBiliSearch({ result: [event()], total: 20, isLastBrush: false }).hasMore, false);
    assert.equal(normalizeBiliSearch({ result: [event()], total: null, isLastBrush: true }).hasMore, false);
    assert.equal(normalizeBiliSearch({ result: [event()], total: null, isLastBrush: false }).hasMore, true);
    for (const total of [undefined, null, "21", false, -1, 1.5, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
        const normalized = normalizeBiliSearch({ result: [event()], total, pagesize: null, page: null, numPages: null });
        assert.equal(normalized.total, null);
        assert.equal(normalized.hasMore, false);
    }
});

test("search pagination fallback uses raw page size before filtering and deduplication", () => {
    const result = Array.from({ length: 20 }, () => event());
    const normalized = normalizeBiliSearch({ result });
    assert.equal(normalized.items.length, 1);
    assert.equal(normalized.hasMore, true);
    assert.equal(normalizeBiliSearch({ result: result.slice(0, 19) }).hasMore, false);
    for (const page of [0, -1, 1.5, "2", null, NaN, Infinity]) {
        assert.throws(() => normalizeBiliSearch({ result: [] }, page), /搜索页码无效/);
    }
});
