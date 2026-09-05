import Database from "tauri-plugin-sql-api";
import { appConfigDir } from "@tauri-apps/api/path";

export const dbName = import.meta.env.DEV ? "sql-test.db" : "sql.db";
export let logTableName = "LOG";
export const settingTableName = "SETTINGS";
let database;

function identifier(name) {
    if (!/^[a-zA-Z0-9_-]+$/.test(name)) throw new Error("无效的数据库标识符");
    return `"${name}"`;
}

export const dbPath = async () => `sqlite:${await appConfigDir()}${dbName}`;
export async function initDb() {
    if (!database) database = dbPath().then(path => Database.load(path)).catch(error => { database = null; throw error; });
    return database;
}
export async function execute(query, values = []) { return (await initDb()).execute(query, values); }
export async function select(query, values = []) { return (await initDb()).select(query, values); }
export const selectAll = table => select(`SELECT * FROM ${identifier(table)}`);
export async function getAppId() { return (await selectAll(settingTableName))[0]?.appid || ""; }
export async function changeLogTableName() {
    const appId = await getAppId();
    logTableName = appId && /^[a-zA-Z0-9_-]+$/.test(appId) ? `${appId}_LOG` : "LOG";
}
export const initSettingTable = () => execute(`CREATE TABLE IF NOT EXISTS SETTINGS (proxy TEXT, appid_list TEXT, appid TEXT)`);
export const initLogTable = () => execute(`CREATE TABLE IF NOT EXISTS ${identifier(logTableName)} (id INTEGER PRIMARY KEY AUTOINCREMENT, time TIMESTAMP, type TEXT, status TEXT, title TEXT, msg TEXT)`);
const sqlValue = value => value !== null && typeof value === "object" ? JSON.stringify(value) : value;
export function insert(table, values) {
    const keys = Object.keys(values);
    return execute(`INSERT INTO ${identifier(table)} (${keys.map(identifier).join(",")}) VALUES (${keys.map(() => "?").join(",")})`, Object.values(values).map(sqlValue));
}
export function update(table, values) {
    return execute(`UPDATE ${identifier(table)} SET ${Object.keys(values).map(key => `${identifier(key)} = ?`).join(",")}`, Object.values(values).map(sqlValue));
}
