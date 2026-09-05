let baxiaReady;
let touches = 0;
let visibilityChanges = 0;
const openedAt = Date.now();
if (typeof window !== "undefined") {
    window.addEventListener("pointerup", () => { touches = Math.min(255, touches + 1); }, { passive: true });
    document.addEventListener("visibilitychange", () => { visibilityChanges = Math.min(255, visibilityChanges + 1); });
}

function loadScript(src) {
    return new Promise((resolve, reject) => {
        const script = document.createElement("script");
        const timer = setTimeout(() => { script.remove(); reject(new Error("大麦凭证脚本加载超时，请检查网络")); }, 8000);
        script.src = src;
        script.onload = () => { clearTimeout(timer); resolve(); };
        script.onerror = () => { clearTimeout(timer); script.remove(); reject(new Error("无法加载大麦凭证脚本，请检查网络")); };
        document.head.appendChild(script);
    });
}

export async function damaiCredentials() {
    if (!baxiaReady) {
        baxiaReady = (async () => {
            await loadScript("https://g.alicdn.com/??/AWSC/AWSC/awsc.js,/sd/baxia-entry/baxiaCommon.js");
            await loadScript("https://g.alicdn.com/??/sd/baxia/2.5.0/baxiaCommon.js");
            if (!window.baxiaCommon) throw new Error("大麦凭证初始化失败");
            window.baxiaCommon.init({ checkApiPath: path => /mtop\.trade\.order\.(build|create)\.h5/.test(path) });
        })().catch(error => { baxiaReady = null; throw error; });
    }
    await baxiaReady;
    for (let attempt = 0; attempt < 30; attempt++) {
        const module = window.__baxia__?.getFYModule;
        if (module) {
            const ua = module.getFYToken();
            const umidtoken = module.getUidToken();
            if (ua && umidtoken) return { ua, umidtoken };
        }
        await new Promise(resolve => setTimeout(resolve, 100));
    }
    baxiaReady = null;
    throw new Error("大麦设备凭证尚未就绪，请稍后重试");
}

export function biliCredentials(projectId) {
    const elapsed = Math.min(65535, Math.max(0, Math.floor((Date.now() - openedAt) / 1000)));
    const dimensions = [window.scrollX, window.scrollY, window.innerWidth, window.innerHeight, window.outerWidth,
        window.outerHeight, window.screenX, window.screenY, screen.width, screen.height, screen.availWidth,
        history.length, "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36".length,
        `https://mall.bilibili.com/neul-next/ticket-renovation/detail.html?id=${projectId}`.length,
        Math.round(10 * (window.devicePixelRatio || 1)), Date.now() % 256];
    const dimension = n => (dimensions[n % 16] + dimensions[n * 3 % 16] + 17 * n) & 255;
    const fields = [dimension(1), touches, dimension(2), visibilityChanges, dimension(3), dimension(4), 0, dimension(5),
        elapsed >> 8, elapsed & 255, elapsed >> 8, elapsed & 255, dimension(6), dimension(7), dimension(8), dimension(9)];
    return { ctoken: btoa(String.fromCharCode(...fields.flatMap(byte => [byte, 0]))) };
}
