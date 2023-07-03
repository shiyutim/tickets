import { useStore } from "vuex";
import { SET_LOG } from "../../src/store/mutation-types.js";
import {
    select,
    insert,
    selectAll,
    logTableName,
} from "../../src/sql/index.js";
import dayjs from "dayjs";

export default class Log {
    constructor() {
        this.store = useStore();
        this.format = "YYYY-MM-DD";
    }

    // 保存 数据库、vuex
    async save(data) {
        let res = await insert(logTableName, data);
        if (res) {
            this.store.commit(SET_LOG, res);
        }
    }

    // 获取所有
    async getAllStore() {
        const query = `SELECT * FROM ${logTableName} ORDER BY time DESC`;
        return await select(query);
    }

    // 获取指定天数
    async getRangeDayStorage(startDate, endDate) {
        const start = dayjs(startDate).startOf("date").valueOf();
        const end = dayjs(endDate).endOf("date").valueOf();

        const query = `SELECT * FROM ${logTableName} WHERE time <= ${end} AND time >= ${start} ORDER BY time DESC`;
        return await select(query);
    }

    getTemplate(type, title = "", status = "", msg = "") {
        return {
            time: Date.now(),
            type,
            status,
            title,
            msg,
        };
    }
}
