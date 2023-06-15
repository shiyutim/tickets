<script setup>
import Header from "./components/common/Header.vue";
import { routes } from "./router";
</script>

<template>
    <!-- <Dm /> -->
    <div class="app-container">
        <div class="nav-wrap">
            <a-menu
                :default-selected-keys="['my']"
                :style="{ width: '200px', height: '100%' }"
            >
                <router-link
                    v-for="item in routes.filter(
                        (item) => item.meta && !item.meta.hideInMenu
                    )"
                    :key="item.name"
                    :to="item.path"
                >
                    <a-menu-item :key="item.name">
                        {{ item.meta.name }}
                    </a-menu-item>
                </router-link>
            </a-menu>
        </div>

        <div class="right">
            <Header></Header>

            <router-view></router-view>
        </div>
    </div>

    <!-- <router-link to="/my">Go to Home</router-link> -->
</template>

<style scoped lang="scss">
.app-container {
    display: flex;
    flex-flow: row nowrap;
}
.right {
    width: 100%;
    flex: 1;
}
.nav-wrap {
    width: 200px;
    box-sizing: border-box;
    height: 600px;
    background-color: var(--color-neutral-2);
}
</style>
