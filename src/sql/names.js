export function logTableForAppId(appId) {
    return typeof appId === "string" && /^[a-zA-Z0-9_-]+$/.test(appId) ? `${appId}_LOG` : "LOG";
}
