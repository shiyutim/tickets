import Database from "tauri-plugin-sql-api";
import { appConfigDir } from "@tauri-apps/api/path";

// sql文件名
export const dbName = import.meta.env.DEV ? `sql-test.db` : `sql.db`;
let db;
// 日志表名称
// 根据 appid，动态更改
export let logTableName = "LOG";
// 全局setting表名称
export const settingTableName = "SETTINGS";

// 修改log 表名
// 表名通过数据库中取
export const changeLogTableName = async () => {
    await initDb();
    // 读取settings
    // 取appid
    // 设置唯一表名
    const res = await selectAll(settingTableName);

    if (Array.isArray(res) && res.length) {
        if (res[0] && res[0].appid) {
            logTableName = `${res[0].appid}_LOG`;
        }
    }
};

// 初始化数据库
export const initDb = async () => {
    if (db) return;
    db = await Database.load(`sqlite:${await appConfigDir()}${dbName}`);
};

// 初始化 日志表
export const initLogTable = async () => {
    await initDb();
    await db.execute(
        `CREATE TABLE IF NOT EXISTS ${logTableName} (id INTEGER PRIMARY KEY AUTOINCREMENT, time TIMESTAMP, type TEXT, status INTEGER, title TEXT, msg TEXT);`
    );
};

// 初始化 设置表
export const initSettingTable = async () => {
    await initDb();
    await db.execute(
        `CREATE TABLE IF NOT EXISTS ${settingTableName} (proxy TEXT, appid_list TEXT, appid TEXT)`
    );
};

// 添加逻辑
export const insert = async (tableName, params) => {
    const [retKeys, retValues] = getSqlInsetQuery(params);

    return await db.execute(
        `INSERT INTO ${tableName} (${retKeys.join()}) VALUES (${retValues.join()});`
    );
};

function getSqlInsetQuery(object) {
    const retKeys = [];
    const retValues = [];

    for (const [key, values] of Object.entries(object)) {
        retKeys.push(key);
        retValues.push(
            typeof values === "object"
                ? `'${JSON.stringify(values)}'`
                : JSON.stringify(values)
        );
    }

    return [retKeys, retValues];
}

function getSqlUpdateQuery(object) {
    let result = [];
    for (const [key, values] of Object.entries(object)) {
        let current = `${key}=${
            typeof values === "object"
                ? `'${JSON.stringify(values)}'`
                : JSON.stringify(values)
        }`;

        result.push(current);
    }

    return result.join();
}

// !!!默认更新所有
export const update = async (tableName, params) => {
    return execute(`UPDATE ${tableName} SET ${getSqlUpdateQuery(params)}`);
};

export const execute = async (query) => {
    return await db.execute(query);
};

// 获取指定
export const select = async (query) => {
    return await db.select(query);
};

// 获取所有
export const selectAll = async (tableName) => {
    return select(`SELECT * FROM ${tableName}`);
};
