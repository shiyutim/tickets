<template>
    <div class="notice-bar" @click="tipClick">
        <div ref="wrap" class="notice-bar__wrap">
            <div
                ref="content"
                class="notice-bar__content"
                :style="contentStyle"
            >
                {{ text }}
            </div>
        </div>
    </div>
</template>

<script setup>
import { ref, onMounted, onBeforeUnmount } from "vue";

const props = defineProps({
    text: {
        type: String,
        default: "",
    },
    speed: {
        type: Number,
        default: 20,
    },
    defaultWidth: {
        type: Number,
        default: 375,
    },
});

const contentStyle = ref({
    transitionDuration: "0s",
    transform: "translateX(0px)",
});

let wrapWidth = 0;
let contentWidth = 0;
let time = 0;
let timer = null;
let convertSpeed = 1;

const wrap = ref(null);
const content = ref(null);

const init = () => {
    const _width = window.innerWidth;
    convertSpeed = (_width / props.defaultWidth) * props.speed;
    wrapWidth = wrap.value.offsetWidth;
    contentWidth = content.value.offsetWidth;
    startAnimate();
    timer = setInterval(startAnimate, time * 1000);
    onBeforeUnmount(() => {
        clearInterval(timer);
        timer = null;
    });
};

const startAnimate = () => {
    contentStyle.value.transitionDuration = "0s";
    contentStyle.value.transform = `translateX(${wrapWidth}px)`;
    time = (wrapWidth + contentWidth) / convertSpeed;
    setTimeout(() => {
        contentStyle.value.transitionDuration = `${time}s`;
        contentStyle.value.transform = `translateX(-${contentWidth}px)`;
    }, 200);
};

const tipClick = () => {
    emit("click");
};

onMounted(() => {
    if (props.text) {
        init();
    }
});

watch(() => props.text, init);
</script>

<style scoped lang="scss">
.notice-bar {
    position: relative;
    width: 100%;
    height: 40px;
    padding-left: 0;
    padding-right: 0;
    font-size: 12px;
    font-family: PingFangSC-Regular, PingFang SC;
    font-weight: 400;
    color: #868daa;
    display: flex;
    align-items: center;

    .notice-bar__icon {
        width: 56px;
        height: 28px;
        margin-right: 20px;

        img {
            width: 100%;
        }
    }

    .notice-bar__wrap {
        position: relative;
        display: flex;
        flex: 1;
        height: 100%;
        align-items: center;
        overflow: hidden;

        .notice-bar__content {
            position: absolute;
            white-space: nowrap;
            transition-timing-function: linear;
        }
    }
}
</style>
