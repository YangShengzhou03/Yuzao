<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import { useCad } from "./state/cad";
import TopBar from "./components/TopBar.vue";
import Ribbon from "./components/Ribbon.vue";
import ViewportPane from "./components/ViewportPane.vue";
import CommandLine from "./components/CommandLine.vue";
import SidePanel from "./components/SidePanel.vue";
import StatusBar from "./components/StatusBar.vue";
import PreferencesDialog from "./components/PreferencesDialog.vue";

const { fullscreen, boot, shutdown } = useCad();

onMounted(boot);
onBeforeUnmount(shutdown);
</script>

<template>
  <div class="shell" :class="{ 'is-fullscreen': fullscreen }">
    <TopBar />
    <Ribbon />

    <!-- 工作区 -->
    <div class="workarea">
      <div class="canvas-col">
        <ViewportPane />
        <CommandLine />
      </div>
      <SidePanel />
    </div>

    <StatusBar />
    <PreferencesDialog />
  </div>
</template>

<style>
/* ===== 单一浅色 CAD 主题 ===== */
:root {
  --cad-bg: #eef1f5;
  --cad-panel: #ffffff;
  --cad-line: #d7dde6;
  --cad-text: #1f2937;
  --cad-muted: #6b7280;
  --cad-accent: #1f6feb;
  --cad-radius: 4px;
  --cad-mono: Consolas, "Cascadia Mono", "Microsoft YaHei", monospace;

  /* 结构高度：Windows 标准控件 32px 为基准 */
  --cad-topbar-h: 38px;
  --cad-cmdline-h: 40px;
  --cad-statusbar-h: 26px;
  --cad-side-w: 280px;

  font-family: "Segoe UI", "Microsoft YaHei", system-ui, sans-serif;
  font-size: 13px;
  color: var(--cad-text);
  -webkit-font-smoothing: antialiased;

  /* Element Plus 主色（唯一主题） */
  --el-color-primary: #1f6feb;
  --el-color-primary-light-3: #5b97f0;
  --el-color-primary-light-5: #8fb8f4;
  --el-color-primary-light-7: #bdd3f9;
  --el-color-primary-light-8: #d3e2fb;
  --el-color-primary-light-9: #e8f0fe;
  --el-color-primary-dark-2: #1a5cc9;
  --el-color-success: #15803d;
  --el-color-success-light-9: #e6f6ec;
  --el-color-warning: #d97706;
  --el-color-warning-light-9: #fdf3e0;
  --el-color-danger: #dc2626;
  --el-color-danger-light-9: #fdeaea;
  --el-border-color: #d7dde6;
  --el-border-color-light: #e4e7ed;
  --el-border-color-lighter: #ebeef5;
  --el-border-color-extra-light: #f2f6fc;
  --el-text-color-primary: #1f2937;
  --el-text-color-regular: #4b5563;
  --el-text-color-secondary: #6b7280;
  --el-text-color-placeholder: #9ca3af;
  --el-bg-color: #ffffff;
  --el-bg-color-overlay: #ffffff;
  --el-fill-color-blank: #ffffff;
  --el-box-shadow-light: 0 8px 24px rgba(15, 23, 42, 0.12);

  /* 结构尺寸（仅布局用，控件一律走 Element Plus 默认尺寸） */
  --el-menu-horizontal-height: var(--cad-topbar-h);
  --el-menu-item-height: 32px;
  --el-menu-sub-item-height: 32px;
  --el-menu-font-size: 13px;
  --el-menu-item-font-size: 13px;
  --el-menu-icon-width: 18px;
  --el-menu-base-level-padding: 14px;
  --el-menu-text-color: #1f2937;
  --el-menu-active-color: #1f6feb;
  --el-menu-hover-bg-color: #e8f0fe;
  --el-menu-border-color: #d7dde6;
  --el-tabs-header-height: 34px;
}

html,
body,
#app {
  margin: 0;
  padding: 0;
  height: 100%;
  overflow: hidden;
}

button {
  font: inherit;
  color: inherit;
}

pre {
  margin: 0;
  font-family: inherit;
  white-space: pre-wrap;
  word-break: break-word;
}

/* Windows 下让拖拽区同时支持触摸与触控笔 */
[data-tauri-drag-region] {
  -webkit-app-region: drag;
  app-region: drag;
}

/* Element Plus 布局归一化 */
.el-menu-item {
  justify-content: flex-start;
}
.el-menu--popup {
  min-width: 220px;
}
.el-menu--popup .el-menu-item {
  width: 100%;
}
.el-tabs__header {
  margin-bottom: 0;
}
.el-tree {
  background: transparent;
}
.el-descriptions__body {
  background: transparent;
}
.el-scrollbar {
  height: 100%;
}
.el-divider--vertical {
  height: 20px;
  margin: 0 6px;
}
</style>

<style scoped>
.shell {
  position: relative;
  display: flex;
  flex-direction: column;
  height: 100%;
  box-sizing: border-box;
  background: var(--cad-bg);
  border: 1px solid var(--cad-line);
}

/* ===== 工作区 ===== */
.workarea {
  display: flex;
  flex: 1;
  min-height: 0;
}
/* 画布列：画布 + 命令行，宽度跟随画布；右侧 side 独立贯穿到底 */
.canvas-col {
  display: flex;
  flex: 1;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
}
</style>
