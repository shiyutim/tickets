import test from "node:test";
import assert from "node:assert/strict";
import { DEFAULT_THEME, THEME_PRESETS, normalizeTheme, themeVariables, applyTheme } from "../src/services/theme.js";

function contrast(left, right) {
    const luminance = color => color.slice(1).match(/../g).map(value => parseInt(value, 16) / 255)
        .map(value => value <= .04045 ? value / 12.92 : ((value + .055) / 1.055) ** 2.4)
        .reduce((sum, value, index) => sum + value * [.2126, .7152, .0722][index], 0);
    const [high, low] = [luminance(left), luminance(right)].sort((a, b) => b - a);
    return (high + .05) / (low + .05);
}

test("theme settings recover from missing or malformed persisted values", () => {
    for (const value of [undefined, null, "dark", [], { mode: "invalid", accent: "red", bilibiliAccent: "url(example)" }]) {
        assert.deepEqual(normalizeTheme(value), DEFAULT_THEME);
    }
    assert.deepEqual(normalizeTheme({ mode: "dark", accent: " #AbC ", bilibiliAccent: "#123ABC", extra: true }), {
        mode: "dark", accent: "#aabbcc", bilibiliAccent: "#123abc",
    });
});

test("custom and preset accents remain readable on both light and dark surfaces", () => {
    const themes = [...THEME_PRESETS, ...["#ffffff", "#000000", "#ffff00", "#00ff00", "#0000ff", "#ff0000"].map(accent => ({ accent, bilibiliAccent: accent }))];
    for (const mode of ["light", "dark"]) for (const theme of themes) {
        const vars = themeVariables({ ...theme, mode });
        for (const prefix of ["--accent", "--bili-accent"]) {
            for (const background of ["--surface", "--page-bg", "--surface-subtle", `${prefix}-soft`, `${prefix}-tint`]) {
                assert.ok(contrast(vars[prefix], vars[background]) >= 4.5, `${mode} ${theme.accent} ${prefix} on ${background}`);
            }
            assert.ok(contrast(vars[prefix], vars[`${prefix}-contrast`]) >= 4.5);
        }
        for (const semantic of ["success", "danger", "warning"]) assert.ok(contrast(vars[`--${semantic}`], vars[`--${semantic}-bg`]) >= 4.5);
    }
});

test("changing brand colors preserves status meaning", () => {
    for (const mode of ["light", "dark"]) {
        const before = themeVariables({ mode, accent: "#ff0000", bilibiliAccent: "#ffffff" });
        const after = themeVariables({ mode, accent: "#0000ff", bilibiliAccent: "#000000" });
        for (const name of ["success", "warning", "danger"]) {
            assert.equal(before[`--${name}`], after[`--${name}`]);
            assert.equal(before[`--${name}-bg`], after[`--${name}-bg`]);
        }
        assert.notEqual(before["--accent"], after["--accent"]);
        assert.notEqual(before["--bili-accent"], after["--bili-accent"]);
    }
});

test("applying a new theme replaces all root colors and the native control scheme", () => {
    const colors = new Map();
    const root = { style: { setProperty: (name, value) => colors.set(name, value) }, dataset: {} };
    applyTheme({ mode: "dark", accent: "#ffff00" }, root);
    const restored = applyTheme(DEFAULT_THEME, root);
    assert.deepEqual(restored, DEFAULT_THEME);
    assert.deepEqual(Object.fromEntries(colors), themeVariables(DEFAULT_THEME));
    assert.equal(root.style.colorScheme, "light");
    assert.equal(root.dataset.themeMode, "light");
});
