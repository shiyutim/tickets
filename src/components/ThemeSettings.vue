<script setup>
import { computed, ref, watch, onActivated, onDeactivated, onBeforeUnmount } from "vue";
import UiIcon from "./common/UiIcon.vue";
import { DEFAULT_THEME, THEME_PRESETS, normalizeTheme, applyTheme, readSavedTheme } from "../services/theme";

const props = defineProps({ modelValue: Object, savedTheme: Object });
const emit = defineEmits(["update:modelValue"]);
const theme = computed(() => normalizeTheme(props.modelValue));
const active = ref(true);
const colorFields = [{ key: "accent", label: "全局与大麦主色" }, { key: "bilibiliAccent", label: "Bilibili 主色" }];
function update(values) { emit("update:modelValue", normalizeTheme({ ...theme.value, ...values })); }
function restore() { active.value = false; applyTheme(props.savedTheme || readSavedTheme()); }
watch(theme, value => { if (active.value) applyTheme(value); }, { immediate: true });
onActivated(() => { active.value = true; applyTheme(theme.value); });
onDeactivated(restore);
onBeforeUnmount(restore);
</script>

<template>
    <section class="panel settings-panel theme-settings">
        <div class="section-heading">
            <span class="section-icon"><UiIcon name="settings" /></span>
            <div><h2>界面配色</h2><p>选择喜欢的颜色，让每一次期待更有你的风格</p></div>
            <button type="button" class="text-button" @click="update(DEFAULT_THEME)">恢复默认</button>
        </div>
        <fieldset class="theme-fieldset">
            <legend class="field-label">外观</legend>
            <div class="segmented-control theme-mode" aria-label="界面明暗">
                <button v-for="mode in [{ value: 'light', label: '浅色' }, { value: 'dark', label: '深色' }]" :key="mode.value" type="button" :class="{ selected: theme.mode === mode.value }" :aria-pressed="theme.mode === mode.value" @click="update({ mode: mode.value })">{{ mode.label }}</button>
            </div>
        </fieldset>
        <fieldset class="theme-fieldset">
            <legend class="field-label">预设配色</legend>
            <div class="theme-presets">
                <button v-for="preset in THEME_PRESETS" :key="preset.id" type="button" class="theme-preset" :class="{ selected: theme.accent === preset.accent && theme.bilibiliAccent === preset.bilibiliAccent }" :aria-pressed="theme.accent === preset.accent && theme.bilibiliAccent === preset.bilibiliAccent" @click="update(preset)">
                    <span class="theme-swatches" aria-hidden="true"><span :style="{ background: preset.accent }"></span><span :style="{ background: preset.bilibiliAccent }"></span></span>
                    <span>{{ preset.label }}</span>
                </button>
            </div>
        </fieldset>
        <div class="two-fields theme-colors">
            <div v-for="field in colorFields" :key="field.key" class="field">
                <label class="field-label" :for="`theme-${field.key}`">{{ field.label }}</label>
                <div class="theme-color-control"><input :id="`theme-${field.key}`" type="color" :value="theme[field.key]" @input="update({ [field.key]: $event.target.value })" /><span>{{ theme[field.key].toUpperCase() }}</span></div>
            </div>
        </div>
        <p class="field-hint">修改后即时预览，点击上方“保存设置”保留配色。文字和按钮会自动调整明暗，保持清晰易读。</p>
    </section>
</template>
