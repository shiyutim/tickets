import {
    ADD_VISIT_USER,
    ADD_FORM,
    SET_SELECT_VISIT_USER,
    SET_LOG,
} from "./mutation-types.js";
import dayjs from "dayjs";

export default {
    [ADD_VISIT_USER](state, payload) {
        state.dm.visitUserList = payload;
    },
    [ADD_FORM](state, payload) {
        state.dm.form = payload;
    },
    [SET_SELECT_VISIT_USER](state, payload) {
        state.dm.selectVisitUserList = Array.isArray(payload) ? payload : [];
    },
    pushBuyHistory(state, payload) {
        if (Array.isArray(payload)) {
            state.dm.buyHistory = [...state.dm.buyHistory, ...payload];
        } else {
            state.dm.buyHistory = [...state.dm.buyHistory, payload];
        }
    },
    [SET_LOG](state, payload) {
        const day = dayjs().format(`YYYY-MM-DD`);
        if (state.dm.log[day]) {
            state.dm.log[day].push(payload);
        } else {
            state.dm.log[day] = [payload];
        }
    },
};
