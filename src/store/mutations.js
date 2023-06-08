export default {
    addForm(state, payload) {
        state.form = payload;
    },
    addVisitUser(state, payload) {
        state.dm.visitUserList = payload;
    },
    setSelectVisitUser(state, payload) {
        state.dm.selectVisitUserList = Array.isArray(payload) ? payload : [];
    },
    pushBuyHistory(state, payload) {
        if (Array.isArray(payload)) {
            state.dm.buyHistory = [...state.dm.buyHistory, ...payload];
        } else {
            state.dm.buyHistory = [...state.dm.buyHistory, payload];
        }
    },
};
