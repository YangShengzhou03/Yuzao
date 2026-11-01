<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref } from "vue";
import { Check, Close, Cpu, FullScreen, Minus } from "@element-plus/icons-vue";
import { useCad } from "../state/cad";

const {
  fullscreen,
  menubarOpen,
  menuGroups,
  unit,
  unitOptions,
  onFilePickerReady,
  onFilePicked,
  onTopbarEnter,
  onTopbarLeave,
  onMenuSelect,
  onMenuOpen,
  onMenuClose,
  revealMenubar,
  toggleMaximize,
  minimize,
  toggleFullscreen,
  onClose,
} = useCad();

// 文件选择器归本组件所有；state 只持有「打开选择器」的回调，避免跨组件 ref 传递。
const fileInput = ref<HTMLInputElement | null>(null);
onMounted(() => onFilePickerReady(() => fileInput.value?.click()));
onBeforeUnmount(() => onFilePickerReady(null));
</script>

<template>
  <!-- 全屏时顶栏收起，仅留顶部 6px 热区；鼠标移入即展开 -->
  <div v-if="fullscreen && !menubarOpen" class="top-reveal" @mouseenter="revealMenubar" />
  <input
    ref="fileInput"
    type="file"
    accept=".json,application/json"
    class="file-hidden"
    @change="onFilePicked"
  />

  <!-- 顶栏：菜单栏与窗口控制合并，中部空白为拖拽区 -->
  <header
    class="topbar"
    :class="{ 'tb-open': menubarOpen }"
    @mouseenter="onTopbarEnter"
    @mouseleave="onTopbarLeave"
  >
    <el-menu
      class="menubar"
      mode="horizontal"
      :ellipsis="false"
      @select="onMenuSelect"
      @open="onMenuOpen"
      @close="onMenuClose"
    >
      <el-sub-menu v-for="g in menuGroups" :key="g.name" :index="g.name">
        <template #title>{{ g.name }}</template>
        <el-menu-item v-for="item in g.items" :key="item.label" :index="item.label">
          <el-icon v-if="item.active" class="mi-check"><Check /></el-icon>
          <span>{{ item.label }}</span>
          <span class="mi-kbd">{{ item.shortcut }}</span>
        </el-menu-item>
      </el-sub-menu>
    </el-menu>

    <div class="tb-drag" data-tauri-drag-region @dblclick="toggleMaximize" />

    <div class="menu-right">
      <span class="mr-label">单位</span>
      <el-select v-model="unit" class="unit-select">
        <el-option v-for="o in unitOptions" :key="o.value" :value="o.value" :label="o.label" />
      </el-select>
      <el-tag type="warning" effect="light" class="mr-tag">
        <el-icon><Cpu /></el-icon>Creo 内核 · 未连接
      </el-tag>
    </div>

    <div class="tb-controls" @dblclick.stop>
      <el-tooltip content="最小化" placement="bottom" :show-after="600">
        <button class="cap-btn" @click="minimize">
          <el-icon :size="14"><Minus /></el-icon>
        </button>
      </el-tooltip>
      <el-tooltip
        :content="fullscreen ? '退出全屏（F11）' : '全屏（F11）'"
        placement="bottom"
        :show-after="600"
      >
        <button class="cap-btn" @click="toggleFullscreen">
          <el-icon :size="14"><FullScreen /></el-icon>
        </button>
      </el-tooltip>
      <el-tooltip content="关闭" placement="bottom" :show-after="600">
        <button class="cap-btn cap-close" @click="onClose">
          <el-icon :size="15"><Close /></el-icon>
        </button>
      </el-tooltip>
    </div>
  </header>
</template>

<style scoped>
.topbar {
  display: flex;
  align-items: stretch;
  height: var(--cad-topbar-h);
  flex: none;
  background: var(--cad-panel);
  border-bottom: 1px solid var(--cad-line);
  user-select: none;
}
/* 全屏：菜单栏浮在画布上方，默认收起，鼠标移到顶部热区才展开 */
.shell.is-fullscreen .topbar {
  position: absolute;
  left: 0;
  right: 0;
  top: 0;
  z-index: 40;
  transform: translateY(-100%);
  transition: transform 0.18s ease;
  box-shadow: 0 2px 10px rgba(15, 23, 42, 0.18);
}
.shell.is-fullscreen .topbar.tb-open {
  transform: translateY(0);
}
.top-reveal {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 6px;
  z-index: 41;
}
.file-hidden {
  display: none;
}
.tb-drag {
  flex: 1;
  min-width: 24px;
}
.tb-controls {
  display: flex;
  align-items: stretch;
  flex: none;
}
.cap-btn {
  width: 46px;
  height: var(--cad-topbar-h);
  border: 0;
  background: transparent;
  color: #374151;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  transition: background-color 0.12s ease, color 0.12s ease;
}
.cap-btn:hover {
  background: #e5e7eb;
}
.cap-btn:active {
  background: #d1d5db;
}
.cap-close:hover {
  background: #e81123;
  color: #ffffff;
}
.cap-close:active {
  background: #c50e1f;
}

/* 菜单栏（位于顶栏内） */
.menubar {
  flex: none;
  border-bottom: none;
}
/* 横向菜单已有展开的下拉面板，标题右侧的下拉箭头冗余，去掉更干净 */
.menubar :deep(.el-sub-menu__icon-arrow) {
  display: none;
}
.mi-check {
  color: var(--cad-accent);
  margin-right: 6px;
}
.mi-kbd {
  margin-left: auto;
  padding-left: 24px;
  color: var(--cad-muted);
  font-size: 12px;
  font-family: var(--cad-mono);
}
.menu-right {
  display: flex;
  align-items: center;
  gap: 10px;
  flex: none;
  padding: 0 12px 0 16px;
  user-select: none;
}
.mr-label {
  font-size: 13px;
  color: var(--cad-muted);
}
.unit-select {
  width: 108px;
}
.mr-tag {
  font-size: 12px;
}
.mr-tag .el-icon {
  margin-right: 6px;
}
</style>
