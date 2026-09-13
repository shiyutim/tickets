export const DEFAULT_THEME = Object.freeze({ mode: "light", accent: "#087f6d", bilibiliAccent: "#cd5c80" });
export const THEME_PRESETS = Object.freeze([
    { id: "forest", label: "青松", accent: "#087f6d", bilibiliAccent: "#cd5c80" },
    { id: "ocean", label: "海蓝", accent: "#2765c2", bilibiliAccent: "#087e95" },
    { id: "violet", label: "暮紫", accent: "#8056b8", bilibiliAccent: "#b44d88" },
    { id: "sunset", label: "暖橙", accent: "#b55c22", bilibiliAccent: "#b94459" },
]);

function normalizeColor(value, fallback) {
    if (typeof value !== "string") return fallback;
    const color = value.trim().toLowerCase();
    if (/^#[\da-f]{6}$/.test(color)) return color;
    if (/^#[\da-f]{3}$/.test(color)) return `#${[...color.slice(1)].map(char => char + char).join("")}`;
    return fallback;
}

export function normalizeTheme(value) {
    const theme = value && typeof value === "object" ? value : {};
    return {
        mode: theme.mode === "dark" ? "dark" : "light",
        accent: normalizeColor(theme.accent, DEFAULT_THEME.accent),
        bilibiliAccent: normalizeColor(theme.bilibiliAccent, DEFAULT_THEME.bilibiliAccent),
    };
}

function channels(color) { return color.slice(1).match(/../g).map(channel => parseInt(channel, 16)); }
function mix(left, right, amount) {
    const target = channels(right);
    return `#${channels(left).map((channel, index) => Math.round(channel + (target[index] - channel) * amount).toString(16).padStart(2, "0")).join("")}`;
}
function luminance(color) {
    return channels(color).map(channel => channel / 255).map(channel => channel <= .04045 ? channel / 12.92 : ((channel + .055) / 1.055) ** 2.4)
        .reduce((sum, channel, index) => sum + channel * [.2126, .7152, .0722][index], 0);
}
function contrast(left, right) {
    const values = [luminance(left), luminance(right)].sort((a, b) => b - a);
    return (values[0] + .05) / (values[1] + .05);
}
function readable(color, backgrounds, dark) {
    for (let step = 0; step <= 100; step++) {
        const candidate = mix(color, dark ? "#ffffff" : "#000000", step / 100);
        if (backgrounds.every(background => contrast(candidate, background) >= 4.5)) return candidate;
    }
    return dark ? "#ffffff" : "#000000";
}

export function themeVariables(value) {
    const theme = normalizeTheme(value);
    const dark = theme.mode === "dark";
    const surface = dark ? "#20262d" : "#ffffff";
    const background = mix(dark ? "#151a20" : "#f5f7f7", theme.accent, dark ? .03 : .015);
    const subtle = mix(surface, theme.accent, dark ? .06 : .035);
    const variables = {
        "--page-bg": background,
        "--surface": surface,
        "--surface-subtle": subtle,
        "--input-bg": dark ? "#192127" : "#fcfdfd",
        "--ink": dark ? "#ecf2f4" : "#213b38",
        "--secondary": dark ? "#c2cdd1" : "#4f6368",
        "--muted": dark ? "#a4b1b7" : "#687c81",
        "--border": dark ? "#3b4750" : "#dce5e7",
        "--border-hover": dark ? "#73828d" : "#a3b7bd",
        "--shadow": dark ? "#00000030" : "#173d2510",
        "--success": dark ? "#82d5ad" : "#277449",
        "--success-bg": dark ? "#1f332b" : "#edf6f0",
        "--success-border": dark ? "#3a5f4e" : "#c9e3d2",
        "--danger": dark ? "#ffb2a7" : "#aa4033",
        "--danger-bg": dark ? "#3d2827" : "#fff1ee",
        "--danger-border": dark ? "#76504c" : "#edc9c2",
        "--warning": dark ? "#f2cf8b" : "#866021",
        "--warning-bg": dark ? "#382f21" : "#fff7e8",
        "--warning-border": dark ? "#6c5835" : "#e6d4ad",
    };
    for (const [prefix, color] of [["--accent", theme.accent], ["--bili-accent", theme.bilibiliAccent]]) {
        const soft = mix(surface, color, dark ? .14 : .08);
        const tint = mix(surface, color, dark ? .2 : .14);
        const accent = readable(color, [surface, background, subtle, soft, tint], dark);
        variables[prefix] = accent;
        variables[`${prefix}-contrast`] = contrast(accent, "#ffffff") >= contrast(accent, "#000000") ? "#ffffff" : "#000000";
        variables[`${prefix}-soft`] = soft;
        variables[`${prefix}-tint`] = tint;
        variables[`${prefix}-border`] = mix(surface, accent, .35);
        variables[`${prefix}-ring`] = `${accent}30`;
    }
    return variables;
}

export function applyTheme(value, root = globalThis.document?.documentElement) {
    const theme = normalizeTheme(value);
    if (root) {
        for (const [name, color] of Object.entries(themeVariables(theme))) root.style.setProperty(name, color);
        root.style.colorScheme = theme.mode;
        root.dataset.themeMode = theme.mode;
    }
    return theme;
}

export function readSavedTheme() {
    try { return normalizeTheme(JSON.parse(globalThis.localStorage?.getItem("tickets.settings") || "{}").theme); }
    catch { return normalizeTheme(); }
}
