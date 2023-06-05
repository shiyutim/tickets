export function getQueryString(name, search) {
    var reg = new RegExp("(^|&)" + name + "=([^&]*)(&|$)", "i");
    var r = (search || window.location.search).substr(1).match(reg);
    if (r != null) return unescape(r[2]);
    return null;
}
