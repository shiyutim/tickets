export function getQueryString(name, search) {
    var reg = new RegExp("(^|&)" + name + "=([^&]*)(&|$)", "i");
    var r = (search || window.location.search).substr(1).match(reg);
    if (r != null) return unescape(r[2]);
    return null;
}

// 获取指定范围随机数字
export function roundNum(smlNum, larNum) {
    return Math.round((larNum - smlNum) * Math.random() + smlNum);
}

// 生成app唯一 标识
export function createAppId() {
    return (
        `app` +
        Math.random().toString(36).substring(2, 15) +
        Math.random().toString(36).substring(2, 15)
    );
}
