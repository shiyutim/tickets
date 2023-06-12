import dayjs from "dayjs";
import { useStore } from "vuex";
import { SET_LOG } from "../../src/store/mutation-types.js";

export default class Log {
    constructor() {
        this.store = useStore();
        this.format = "YYYY-MM-DD";
    }

    getDay() {
        return dayjs().format(this.format);
    }

    // 保存
    save(data) {
        let storage = this.getAllStorage();
        let current = this.getCurrentStorage() || [];
        current.push(data);
        storage[this.getDay()] = current;

        localStorage.setItem("log", JSON.stringify(storage));
        this.store.commit(SET_LOG, data);
    }

    // 获取所有
    getAllStorage() {
        let res = localStorage.getItem("log");
        res = res ? JSON.parse(res) : {};

        return res;
    }

    getAllStorageList() {
        let all = this.getAllStorage();
        return Object.values(all);
    }

    // 获取当天
    getCurrentStorage() {
        return this.getDayStorage();
    }

    // 获取指定天数
    getDayStorage(day = this.getDay()) {
        return this.getAllStorage()[day] || [];
    }

    getRangeDayStorage(startDate, endDate) {
        const res = [];
        let list = this.getRangeList(startDate, endDate);
        list.forEach((item) => {
            res.push(this.getDayStorage(item));
        });

        return res;
    }

    getRangeList(startDate, endDate) {
        const dates = [];
        let currentDate = new Date(startDate);

        while (currentDate <= endDate) {
            dates.push(dayjs(currentDate).format(this.format));
            currentDate.setDate(currentDate.getDate() + 1);
        }

        return dates;
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
