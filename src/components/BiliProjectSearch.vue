<script setup>
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import UiIcon from "./common/UiIcon.vue";
import { call, errorText } from "../services/runtime";
import { normalizeBiliSearch } from "../services/bilibiliSearch";
import { projectId } from "../services/platforms";

const props = defineProps({
    account: { type: Object, required: true },
    problem: { type: String, default: "" },
    disabled: Boolean,
    ready: Boolean,
    value: { type: String, default: "" },
    project: { type: Object, default: null },
    projectLoading: Boolean,
});
const emit = defineEmits(["select", "busy", "editing"]);
const query = ref(props.value);
const input = ref(null);
const editing = ref(!props.project);
const lastKeyword = ref("");
const pendingId = ref("");
const results = ref(null);
const resultsOpen = ref(true);
const page = ref(1);
const loading = ref(false);
const error = ref("");
const coverFailed = ref(false);
const inputId = computed(() => {
    const id = projectId(query.value, "bilibili");
    return id && Number.isSafeInteger(Number(id)) ? id : "";
});
const directInput = computed(() => Boolean(inputId.value)
    || /^[+-]?\d+(?:\.\d+)?$/.test(query.value.trim())
    || /^(?:[a-z][a-z\d+.-]*:|www\.|(?:[^/\s]+\.)?bilibili\.com(?:[/?#]|$))/i.test(query.value.trim()));
let generation = 0;

watch(loading, value => emit("busy", value), { flush: "sync" });
watch(editing, value => emit("editing", value), { immediate: true, flush: "sync" });
watch([() => query.value.trim(), () => JSON.stringify(props.account), () => props.problem], () => {
    generation++;
    loading.value = false;
    results.value = null;
    page.value = 1;
    error.value = "";
    resultsOpen.value = true;
}, { flush: "sync" });
watch(() => props.value, (value, previous) => {
    if (!query.value.trim() || query.value === previous || inputId.value) query.value = value;
});
watch([() => props.project, () => props.projectLoading], ([project, projectLoading], [, wasLoading]) => {
    if (project) {
        editing.value = false;
        pendingId.value = "";
    } else {
        editing.value = true;
        if (!projectLoading && wasLoading && pendingId.value) {
            query.value = pendingId.value;
            pendingId.value = "";
        }
    }
});
watch(() => props.project?.image, () => { coverFailed.value = false; });
watch(() => props.disabled, disabled => {
    if (disabled && loading.value) { generation++; loading.value = false; }
}, { flush: "sync" });
onBeforeUnmount(() => { generation++; emit("busy", false); emit("editing", false); });

async function search(targetPage = 1) {
    if (props.disabled || !props.ready || loading.value) return;
    error.value = "";
    if (!query.value.trim()) { error.value = "请输入活动名称、城市、链接或 ID"; return; }
    if (props.problem) { error.value = props.problem; return; }
    lastKeyword.value = query.value.trim();
    resultsOpen.value = true;
    const version = ++generation;
    loading.value = true;
    try {
        const raw = await call("bili_search", { account: { ...props.account }, keyword: lastKeyword.value, page: targetPage });
        if (version !== generation) return;
        results.value = normalizeBiliSearch(raw, targetPage);
        page.value = targetPage;
    } catch (value) {
        if (version === generation) error.value = errorText(value);
    } finally {
        if (version === generation) loading.value = false;
    }
}

function submit() {
    if (props.disabled || !props.ready || loading.value) return;
    if (!directInput.value) { search(); return; }
    if (!inputId.value) { error.value = "请输入有效的官方商品链接或项目编号"; return; }
    select(inputId.value);
}

function select(id) {
    if (props.disabled || !props.ready || loading.value) return;
    if (props.problem) { error.value = props.problem; return; }
    error.value = "";
    resultsOpen.value = false;
    pendingId.value = id;
    emit("select", id);
}

async function changeProject() {
    if (props.disabled) return;
    if (lastKeyword.value) query.value = lastKeyword.value;
    resultsOpen.value = true;
    editing.value = true;
    await nextTick();
    input.value?.focus();
}

function cancelChange() {
    if (props.disabled || !props.project) return;
    generation++;
    loading.value = false;
    error.value = "";
    editing.value = false;
}
</script>

<template>
    <div class="bili-project-picker" :aria-busy="loading || projectLoading">
        <p v-if="projectLoading && !project" class="project-loading" role="status"><UiIcon name="refresh" class="spinning" />正在加载活动…</p>
        <div v-else-if="project && !editing" class="selected-project">
            <img v-if="project.image && !coverFailed" :src="project.image" alt="" class="project-cover" referrerpolicy="no-referrer" @error="coverFailed = true" />
            <span v-else class="project-cover placeholder-cover"><UiIcon name="ticket" /></span>
            <div class="selected-project-info"><strong>{{ project.name }}</strong><p>{{ [project.venue, `ID ${project.id}`].filter(Boolean).join(' · ') }}</p></div>
            <div class="selected-project-actions"><button type="button" class="button secondary small" :disabled="disabled" @click="changeProject">更换</button><button type="button" class="text-button" :disabled="disabled || !ready" @click="select(project.id)"><UiIcon name="refresh" />刷新</button></div>
        </div>
        <template v-else>
            <form class="search-form" @submit.prevent="submit">
                <div class="search-label-row"><label class="field-label" for="bili-project-query">活动</label><button v-if="project" type="button" class="text-button" :disabled="disabled" @click="cancelChange">取消更换</button></div>
                <div class="search-input-row"><input id="bili-project-query" ref="input" v-model="query" class="text-input" type="search" placeholder="搜索活动名称、城市，或粘贴链接 / ID" autocomplete="off" :disabled="disabled" /><button type="submit" class="button primary" :disabled="disabled || loading || !ready"><UiIcon :name="loading ? 'refresh' : directInput ? 'link' : 'search'" :class="{ spinning: loading }" />{{ loading ? '搜索中…' : directInput ? '加载活动' : '搜索' }}</button></div>
            </form>
            <div aria-live="polite">
                <p v-if="error" class="inline-error" role="alert">{{ error }}</p>
                <p v-if="loading" class="search-message" role="status">正在搜索活动…</p>
                <template v-else-if="results && resultsOpen">
                    <div class="search-results-heading"><span>{{ results.total === null ? '搜索结果' : `找到 ${results.total} 个活动` }}</span><span>第 {{ page }} 页</span></div>
                    <div v-if="results.items.length" class="search-results">
                        <button v-for="item in results.items" :key="item.id" type="button" class="search-result" :class="{ chosen: project?.id === item.id }" :disabled="disabled || !ready" :aria-pressed="project?.id === item.id" @click="select(item.id)">
                            <img v-if="item.image" :src="item.image" alt="" class="project-cover" loading="lazy" referrerpolicy="no-referrer" @error="item.image = ''" /><span v-else class="project-cover placeholder-cover"><UiIcon name="ticket" /></span>
                            <span class="search-result-info"><strong>{{ item.name }}</strong><span v-if="item.venue || item.date">{{ [item.venue, item.date].filter(Boolean).join(' · ') }}</span></span>
                            <span class="search-result-price">{{ item.price }}</span><UiIcon name="chevron" class="result-arrow" />
                        </button>
                    </div>
                    <p v-else class="search-message">{{ page > 1 ? '这一页没有更多活动，可以返回上一页。' : '没有找到相关活动，试试其他名称或城市。' }}</p>
                    <nav v-if="page > 1 || results.hasMore" class="search-pagination" aria-label="搜索结果翻页"><button type="button" class="text-button" :disabled="disabled || !ready || page <= 1" @click="search(page - 1)">上一页</button><span>{{ page }}</span><button type="button" class="text-button" :disabled="disabled || !ready || !results.hasMore" @click="search(page + 1)">下一页</button></nav>
                </template>
            </div>
        </template>
    </div>
</template>

<style scoped>
.bili-project-picker { min-width: 0; }
.search-label-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 18px; margin-bottom: 8px; }
.search-label-row .field-label { margin-bottom: 0; }
.search-input-row { display: flex; gap: 10px; }
.search-input-row .text-input { min-width: 0; flex: 1; }
.search-input-row .button { flex-shrink: 0; }
.search-results-heading { display: flex; justify-content: space-between; gap: 12px; margin: 20px 0 8px; color: var(--muted); font-size: 11px; }
.search-results { display: flex; flex-direction: column; max-height: 380px; overflow-y: auto; }
.search-result { display: flex; align-items: center; gap: 12px; padding: 12px 8px; min-width: 0; width: 100%; border: 0; border-bottom: 1px solid var(--border); border-radius: 0; background: transparent; text-align: left; color: var(--ink); transition: background .15s; }
.search-result:last-child { border-bottom: 0; }
.search-result:hover:not(:disabled), .search-result.chosen { background: var(--accent-soft); }
.search-result:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; border-radius: 4px; }
.project-cover { width: 42px; height: 56px; flex-shrink: 0; object-fit: cover; border-radius: 4px; background: var(--surface-subtle); }
.placeholder-cover .ui-icon { width: 22px; height: 22px; }
.search-result-info { display: flex; flex-direction: column; gap: 6px; min-width: 0; flex: 1; }
.search-result-info > strong { font-size: 12px; font-weight: 550; line-height: 1.6; display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 2; overflow: hidden; overflow-wrap: anywhere; }
.search-result-info > span { font-size: 10px; line-height: 1.6; color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.search-result-price { flex-shrink: 0; max-width: 140px; color: var(--accent); font-size: 12px; line-height: 1.6; font-weight: 550; }
.result-arrow { flex-shrink: 0; width: 15px; height: 15px; color: var(--muted); }
.search-message { text-align: center; padding: 22px 12px; font-size: 12px; line-height: 1.8; color: var(--muted); }
.search-pagination { display: flex; justify-content: center; align-items: center; gap: 20px; margin-top: 14px; }
.search-pagination > span { color: var(--muted); font-size: 11px; }
.selected-project { display: flex; align-items: center; gap: 13px; min-height: 64px; }
.selected-project-info { min-width: 0; flex: 1; }
.selected-project-info strong { display: block; font-size: 14px; font-weight: 550; line-height: 1.65; overflow-wrap: anywhere; }
.selected-project-info p { color: var(--muted); font-size: 11px; line-height: 1.7; margin-top: 6px; overflow-wrap: anywhere; }
.selected-project-actions { display: flex; align-items: center; gap: 14px; flex-shrink: 0; }
.selected-project-actions .text-button { display: inline-flex; align-items: center; gap: 4px; }
.selected-project-actions .ui-icon { width: 13px; height: 13px; }
.project-loading { display: flex; align-items: center; justify-content: center; gap: 9px; min-height: 74px; color: var(--muted); font-size: 12px; }
.project-loading .ui-icon { width: 16px; height: 16px; }
@media (max-width: 600px) {
    .search-result { flex-wrap: wrap; gap: 7px 10px; }
    .search-result-info { flex-basis: calc(100% - 78px); }
    .search-result-price { margin-left: 52px; }
    .result-arrow { margin-left: auto; }
    .selected-project { flex-wrap: wrap; }
    .selected-project-actions { width: 100%; justify-content: flex-end; }
}
</style>
