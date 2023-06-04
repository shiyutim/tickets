<script setup lang="js">
import { ref, reactive, onMounted } from "vue";
import {
    getSign,
    loadBaxiaScript,
    initBaxia,
    loadBaxiaTime,
    getHeaderUaAndUmidtoken,
    getToken,
    commonTip,
    isSuccess,
    combinationOrderParams,
    joinMsg,
    encode,
} from "../../utils/dm/index.js";
import { invoke } from "@tauri-apps/api/tauri";
import { Message } from "@arco-design/web-vue";

onMounted(() => {
    // 加载凭证脚本
    loadBaxiaScript();
    // 初始化凭证
    setTimeout(() => {
        let res = initBaxia();
        if (!res) {
            Message.error(
                "初始化凭证脚本失败，本脚本将无法继续，请重新启动程序。如一直无法加载，请联系管理员修复。"
            );
        }
    }, loadBaxiaTime);
});

const form = reactive({
    token: "", // 获取 sign 使用
    cookie: "",
    itemId: "",
    num: "1",
    retry: "2",
});

const handleSubmit = data => {
    if (data.errors) {
        return;
    }

    getProductInfo();
};

const productInfo = ref(null);

async function getProductInfo() {
    // 根据 cookie 解析 token，用来生成 t 和 sign
    form.token = getToken(form.cookie);
    const data = `{"itemId":"${form.itemId}","bizCode":"ali.china.damai","scenario":"itemsku","exParams":"{\\"dataType\\":4,\\"dataId\\":\\"\\",\\"privilegeActId\\":\\"\\"}","dmChannel":"damai@damaih5_h5"}`;
    const [t, sign] = getSign(data, form.token);
    console.log(form.cookie)

    try {
        const res = await invoke("get_product_info", {
            t,
            sign,
            itemid: form.itemId,
            cookie: form.cookie,
        });

        const parseData = JSON.parse(res);
        if (Array.isArray(parseData.ret) && parseData.ret.length) {
            const message = parseData.ret[0];

            commonTip(message);

            if (isSuccess(message)) {
                const result = parseData.data
                    ? JSON.parse(parseData.data.result)
                    : "";
                if (result) {
                    if (result.itemBuyBtn.btnStatus === "303") {
                        Message.warning(`商品已下架`);
                        return;
                    }
                    productInfo.value = result;
                }
            } else {
                Message.error("获取商品详情失败");
            }
        }
    } catch (e) {
        Message.error(joinMsg(["获取商品详情失败", JSON.stringify(e)]));
    }
}

const skuInfo = reactive({});

// 当前高亮的 票档
const activeSku = ref(null);

async function getSkuInfo(item) {
    if (!item && item.performId) {
        Message.error("场地信息或场次 id 获取错误，请重新获取商品信息");
        return;
    }

    const data = `{"itemId":"${form.itemId}","bizCode":"ali.china.damai","scenario":"itemsku","exParams":"{\\"dataType\\":2,\\"dataId\\":\\"${item.performId}\\",\\"privilegeActId\\":\\"\\"}","dmChannel":"damai@damaih5_h5"}`;
    const [t, sign] = getSign(data, form.token);

    try {
        const res = await invoke("get_ticket_list", {
            t,
            sign,
            itemid: form.itemId,
            cookie: form.cookie,
            dataid: item.performId,
        });

        const parseData = JSON.parse(res);
        if (Array.isArray(parseData.ret) && parseData.ret.length) {
            const message = parseData.ret[0];

            commonTip(message);

            if (isSuccess(message)) {
                const result = parseData.data
                    ? JSON.parse(parseData.data.result)
                    : "";
                if (result) {
                    skuInfo[item.performId] = result;
                }
            }
        }
    } catch (e) {
        Message.error(joinMsg(["票档获取失败，请重试", JSON.stringify(e)]));
    }
}

const orderDetail = ref(null);
async function skuHandle(item) {
    isCanClick.value = false;
    activeSku.value = item.skuId;
    const data = `{"buyNow":true,"exParams":"{\\"channel\\":\\"damai_app\\",\\"damai\\":\\"1\\",\\"umpChannel\\":\\"100031004\\",\\"subChannel\\":\\"damai@damaih5_h5\\",\\"atomSplit\\":1,\\"serviceVersion\\":\\"2.0.0\\",\\"customerType\\":\\"default\\"}","buyParam":"${item.itemId}_${form.num}_${item.skuId}","dmChannel":"damai@damaih5_h5"}`;

    const [t, sign] = getSign(data, form.token);
    const [ua, umidtoken] = getHeaderUaAndUmidtoken();

    try {
        const res = await invoke("get_ticket_detail", {
            t,
            sign,
            cookie: form.cookie,
            data,
            ua,
            umidtoken,
        });

        const parseData = JSON.parse(res);

        if (Array.isArray(parseData.ret) && parseData.ret.length) {
            const message = parseData.ret[0];

            commonTip(message);

            if (isSuccess(message)) {
                orderDetail.value = parseData.data;
                isCanClick.value = true;
            } else {
                Message.error(joinMsg(["error", ...parseData.ret]));
            }
        }
    } catch (e) {
        Message.error(joinMsg(["订单详情获取失败，请重试", JSON.stringify(e)]))
    }
}

async function createOrder(data, submitref) {
    const [t, sign] = getSign(data, form.token);
    const [ua, umidtoken] = getHeaderUaAndUmidtoken();

    let lastData = encode({
        data
    })
    lastData = `${lastData}&bx-ua=${ua}&bx-umidtoken=${umidtoken}`;

    try {
        const res = await invoke("create_order", {
            cookie: form.cookie,
            t,
            sign,
            data: lastData,
            submitref,
        });

        const parseData = JSON.parse(res);
        if (Array.isArray(parseData.ret) && parseData.ret.length) {
            const message = parseData.ret[0];

            commonTip(message);

            if (isSuccess(message)) {
                Message.success("购买成功!!!，请在订单页进行支付");
                return true;
            } else {
                Message.error(joinMsg(["error", ...parseData.ret]));
            }
        }
    } catch (e) {
        Message.error(joinMsg(["订单发送失败，请重试", JSON.stringify(e)]))
    } finally {
        return false;
    }
}

function getSecret(data) {
    return data.global
        ? data.global.secretKey + "=" + data.global.secretValue
        : "";
}

const isCanClick = ref(false);

async function buy() {
    let maxRetry = Number(form.retry);

    const confirmParams = JSON.stringify(
        combinationOrderParams(orderDetail.value)
    );
    const submitref = getSecret(orderDetail.value);

    while (maxRetry--) {
        const res = await createOrder(confirmParams, submitref);
        if (res) return;
    }
}
</script>

<template>
    <div class="container">
        <section>
            <a-form
                :model="form"
                :style="{ width: '600px' }"
                @submit="handleSubmit"
            >
                <a-form-item field="cookie" label="cookie" required>
                    <a-textarea
                        v-model="form.cookie"
                        placeholder="请输入 cookie"
                        allow-clear
                    />
                </a-form-item>

                <a-form-item
                    field="itemId"
                    tooltip="进入商品页面，查看页面路径上的 itemId"
                    label="itemId"
                    required
                >
                    <a-input
                        v-model="form.itemId"
                        placeholder="请输入itemId..."
                    />
                </a-form-item>

                <a-form-item
                    field="num"
                    label="购买张数"
                    required
                    tooltip="目前的购买张数必须跟实名信息个数一致"
                >
                    <a-input
                        v-model="form.num"
                        placeholder="请输入购买数量..."
                    />
                </a-form-item>
                <a-form-item
                    field="retry"
                    label="重试次数"
                    required
                    tooltip="下订单失败的重试次数"
                >
                    <a-input v-model="form.retry" disabled />
                </a-form-item>

                <a-form-item>
                    <a-button html-type="submit">获取商品信息</a-button>
                </a-form-item>
            </a-form>
        </section>

        <section class="product-wrap">
            <div class="subtitle">商品信息</div>
            <div v-if="productInfo">
                <div class="info-wrap" v-if="productInfo.itemBasicInfo">
                    <div class="left">
                        <img
                            class="img"
                            :src="productInfo.itemBasicInfo.mainImageUrl"
                        />
                    </div>

                    <div class="right">
                        <div class="column">
                            <div>{{ productInfo.itemBasicInfo.cityName }}</div>
                            <div>{{ productInfo.itemBasicInfo.venueName }}</div>
                        </div>
                        <div class="column">
                            <div class="label">名称：</div>
                            <div class="value">
                                {{
                                    productInfo.itemBasicInfo.itemTitle ||
                                    productInfo.itemBasicInfo.projectTitle
                                }}
                            </div>
                        </div>
                        <div class="column">
                            <div class="label">itemId：</div>
                            <div class="value">
                                {{ productInfo.itemBasicInfo.itemId }}
                            </div>
                        </div>
                        <div class="column">
                            <div class="label">价格：</div>
                            <div class="value">
                                {{ productInfo.itemBasicInfo.priceRange }}
                            </div>
                        </div>

                        <a-button
                            style="margin-bottom: 10px"
                            type="primary"
                            status="success"
                            :disabled="!isCanClick"
                            @click="buy"
                            >开始抢票</a-button
                        >

                        <div class="ticket-list">
                            <div
                                v-for="item in productInfo.performCalendar
                                    .performViews"
                                class="ticket-item"
                                @click="getSkuInfo(item)"
                            >
                                {{ item.performName }}
                            </div>
                        </div>

                        <div class="sku-wrap">
                            <div v-for="item in skuInfo" class="sku">
                                <div
                                    class="sku-item"
                                    :class="{
                                        'active-sku': activeSku === sku.skuId,
                                    }"
                                    v-for="sku in item.perform.skuList"
                                    @click="skuHandle(sku)"
                                >
                                    <div>{{ sku.priceName }}</div>
                                    <div>{{ sku.price }}</div>
                                    <div class="tag">
                                        {{
                                            Array.isArray(sku.tags) &&
                                            sku.tags.length
                                                ? sku.tags[0].tagDesc
                                                : ""
                                        }}
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>
                </div>
            </div>
            <div v-else>请先获取商品信息</div>
        </section>
    </div>
</template>

<style scoped lang="scss">
.product-wrap {
    padding: 20px;
}
.info-wrap {
    display: flex;
    flex-flow: row nowrap;
}
.left {
    margin-right: 20px;
    .img {
        width: 250px;
        height: 350px;
    }
}
.column {
    display: flex;
    flex-flow: row nowrap;
    align-items: center;

    margin-bottom: 20px;
    &:last-child {
        margin-bottom: 0;
    }

    .label {
        margin-right: 5px;
        font-weight: 600;
    }
}

.subtitle {
    font-size: 20px;
    margin-bottom: 10px;
}

.ticket-list {
    display: flex;
    flex-flow: row nowrap;
}
.ticket-item {
    border: 1px solid #ccc;
    width: 200px;
    padding: 20px 0;
    margin-bottom: 10px;
    margin-right: 30px;

    text-align: center;
    cursor: pointer;
    &:hover {
        background: #eee;
    }
}

.sku-wrap {
    display: flex;
    flex-flow: row wrap;

    .sku-item {
        border: 1px solid #ccc;
        cursor: pointer;
        padding: 20px 0;
        margin-bottom: 10px;

        .tag {
            color: #ff0000;
        }
    }

    .active-sku {
        background: #eee;
    }
}
</style>
