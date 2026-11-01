<script setup lang="ts">
import { useCad } from "../state/cad";

const {
  view,
  viewOptions,
  wireframe,
  showGrid,
  showAxes,
  sideOpen,
  sideTab,
  running,
  menubarOpen,
  onTopbarEnter,
  onTopbarLeave,
  fitView,
  validateSpec,
  rebuild,
  openTree,
  openLog,
} = useCad();
</script>

<template>
  <!-- 功能区（Ribbon）：全屏时与菜单栏一起收起，鼠标移到顶部一并展开 -->
  <div
    class="ribbon"
    :class="{ 'tb-open': menubarOpen }"
    @mouseenter="onTopbarEnter"
    @mouseleave="onTopbarLeave"
  >
    <div class="rp-group">
      <div class="rp-body">
        <el-segmented v-model="view" :options="viewOptions" />
        <el-tooltip content="缩放至模型范围（F）" placement="bottom" :show-after="600">
          <el-button @click="fitView">适应</el-button>
        </el-tooltip>
      </div>
      <div class="rp-caption">视图</div>
    </div>

    <el-divider direction="vertical" />

    <div class="rp-group">
      <div class="rp-body">
        <label class="sw-item" title="开启为线框显示，关闭为着色显示（W）">
          <span>线框</span>
          <el-switch v-model="wireframe" />
        </label>
        <label class="sw-item" title="显示或隐藏参考网格（G）">
          <span>网格</span>
          <el-switch v-model="showGrid" />
        </label>
        <label class="sw-item" title="显示或隐藏坐标轴（X）">
          <span>坐标轴</span>
          <el-switch v-model="showAxes" />
        </label>
      </div>
      <div class="rp-caption">显示</div>
    </div>

    <el-divider direction="vertical" />

    <div class="rp-group">
      <div class="rp-body">
        <el-tooltip
          content="调用 Rust 侧 FeatureSpec 校验器，通过后自动重建预览"
          placement="bottom"
          :show-after="600"
        >
          <el-button type="primary" :loading="running" @click="validateSpec">校验</el-button>
        </el-tooltip>
        <el-tooltip content="按当前 FeatureSpec 重建预览实体" placement="bottom" :show-after="600">
          <el-button @click="rebuild">重建</el-button>
        </el-tooltip>
      </div>
      <div class="rp-caption">特征</div>
    </div>

    <el-divider direction="vertical" />

    <div class="rp-group">
      <div class="rp-body">
        <el-tooltip content="显示或隐藏模型树与属性面板" placement="bottom" :show-after="600">
          <el-button :type="sideOpen && sideTab === 'tree' ? 'primary' : 'default'" @click="openTree">
            模型树
          </el-button>
        </el-tooltip>
        <el-tooltip content="切换到会话面板查看命令历史" placement="bottom" :show-after="600">
          <el-button :type="sideTab === 'log' ? 'primary' : 'default'" @click="openLog">对话</el-button>
        </el-tooltip>
      </div>
      <div class="rp-caption">面板</div>
    </div>
  </div>
</template>

<style scoped>
.ribbon {
  display: flex;
  flex-wrap: wrap;
  align-items: stretch;
  gap: 4px 6px;
  padding: 5px 10px;
  flex: none;
  background: var(--cad-panel);
  border-bottom: 1px solid var(--cad-line);
}
/* 全屏：功能区与菜单栏一起浮在画布上方，默认收起 */
.shell.is-fullscreen .ribbon {
  position: absolute;
  left: 0;
  right: 0;
  top: var(--cad-topbar-h);
  z-index: 40;
  transform: translateY(calc(-100% - var(--cad-topbar-h)));
  transition: transform 0.18s ease;
  box-shadow: 0 2px 10px rgba(15, 23, 42, 0.18);
}
.shell.is-fullscreen .ribbon.tb-open {
  transform: translateY(0);
}
.rp-group {
  border: 1px solid var(--el-border-color-lighter);
  border-radius: var(--cad-radius);
  background: #fcfdfe;
}
.rp-body {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 6px 10px;
}
.rp-caption {
  font-size: 12px;
  line-height: 1.2;
  color: var(--cad-muted);
  text-align: center;
  padding: 0 8px 3px;
}
.sw-item {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--cad-text);
  cursor: pointer;
  white-space: nowrap;
  border-radius: var(--cad-radius);
  padding: 2px 4px;
  transition: background-color 0.12s ease, color 0.12s ease;
}
.sw-item:hover {
  background: #eef2f8;
}
.sw-item:hover span {
  color: var(--cad-accent);
}
.sw-item .el-switch {
  margin: 0;
}
</style>
