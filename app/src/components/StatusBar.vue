<script setup lang="ts">
import { useCad } from "../state/cad";

const { cursor, unit, view, viewLabel, modelName, featureCount, selectedText } = useCad();
</script>

<template>
  <footer class="statusbar">
    <span class="st-item">
      光标 <b>{{ cursor ? `X ${cursor.x.toFixed(2)} · Y ${cursor.y.toFixed(2)}` : "—" }}</b>
    </span>
    <el-divider direction="vertical" />
    <span class="st-item">单位 <b>{{ unit }}</b></span>
    <el-divider direction="vertical" />
    <span class="st-item">视图 <b>{{ viewLabel(view) }}</b></span>
    <el-divider direction="vertical" />
    <span class="st-item">模型 <b>{{ modelName }}</b></span>
    <el-divider direction="vertical" />
    <span class="st-item">特征 <b>{{ featureCount }}</b></span>
    <el-divider direction="vertical" />
    <span class="st-item">选中 <b>{{ selectedText }}</b></span>
    <span class="st-spacer" />
    <el-tooltip
      content="视图层为快照渲染（ADR-011）；连接 Creo 原生内核写入 .prt 属 ROADMAP P2"
      placement="top"
      :show-after="600"
    >
      <span class="st-item st-kernel"><i class="st-dot" />Creo 内核 未连接</span>
    </el-tooltip>
  </footer>
</template>

<style scoped>
.statusbar {
  display: flex;
  align-items: center;
  gap: 10px;
  height: var(--cad-statusbar-h);
  flex: none;
  padding: 0 12px;
  font-size: 12px;
  color: var(--cad-muted);
  background: var(--cad-panel);
  border-top: 1px solid var(--cad-line);
}
.st-item b {
  font-family: var(--cad-mono);
  font-weight: 600;
  color: #374151;
}
.st-kernel {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  color: var(--cad-muted);
}
.st-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: #9ca3af;
  flex: none;
}
.st-spacer {
  margin-left: auto;
}
.statusbar :deep(.el-divider--vertical) {
  height: 14px;
  margin: 0 4px;
}
</style>
