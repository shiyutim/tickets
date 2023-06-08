import { createStore } from "vuex";
import state from "./state";
import mutations from "./mutations";

const store = createStore({
    state,
    mutations,
});

export default store;
