<script setup lang="ts">
import { onBeforeUnmount, onMounted } from "vue";
import { useCad } from "../state/cad";

const {
  canvasEl,
  canvasWrap,
  viewportError,
  selected,
  featureAnchor,
  toolbarStyle,
  aiPrompt,
  aiBusy,
  mountViewport,
  unmountViewport,
  applyFeaturePrompt,
} = useCad();

onMounted(() => {
  if (canvasEl.value && canvasWrap.value) mountViewport(canvasEl.value, canvasWrap.value);
});
onBeforeUnmount(unmountViewport);
</script>

<template>
  <div ref="canvasWrap" class="viewport-wrap">
    <canvas
      ref="canvasEl"
      class="viewport"
      aria-label="3D 模型视口：拖拽旋转，滚轮缩放，点击特征选中"
    />
    <div v-if="viewportError" class="vp-error">
      <div class="vp-error-title">3D 画布不可用</div>
      <pre class="vp-error-msg">{{ viewportError }}</pre>
    </div>
    <!-- 选中特征后悬浮的 AI 工具条（Word 迷你工具栏式），跟随画布拾取点 -->
    <div v-if="selected && featureAnchor" class="ft-bar" :style="toolbarStyle">
      <el-tag type="primary" effect="plain" size="small" class="ft-id">{{ selected.id }}</el-tag>
      <span class="ft-name">{{ selected.label }}</span>
      <el-input
        v-model="aiPrompt"
        size="small"
        class="ft-input"
        placeholder="描述怎么改，如：孔径改为 12"
        :disabled="aiBusy"
        @keydown.enter="applyFeaturePrompt"
      />
      <el-button
        type="primary"
        size="small"
        class="ft-go"
        :loading="aiBusy"
        @click="applyFeaturePrompt"
      >
        AI 修改
      </el-button>
    </div>
  </div>
</template>

<style scoped>
.viewport-wrap {
  position: relative;
  flex: 1;
  min-width: 0;
  /* 画布可被压缩但不能消失；过高会让窄窗口下命令行被挤出、盖住状态栏 */
  min-height: 120px;
  overflow: hidden;
}
.viewport {
  display: block;
}
.vp-error {
  position: absolute;
  inset: 24px;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  background: rgba(255, 255, 255, 0.96);
  border: 1px solid var(--el-color-danger);
  border-radius: var(--cad-radius);
  padding: 24px;
  text-align: center;
}
.vp-error-title {
  font-size: 15px;
  font-weight: 600;
  color: var(--el-color-danger);
}
.vp-error-msg {
  max-width: 560px;
  font-size: 13px;
  color: var(--cad-text);
  white-space: pre-wrap;
  word-break: break-word;
}

/* 画布悬浮 AI 工具条 */
.ft-bar {
  position: absolute;
  z-index: 20;
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: 460px;
  padding: 6px 8px;
  background: var(--cad-panel);
  border: 1px solid var(--cad-line);
  border-radius: var(--cad-radius);
  box-shadow: 0 4px 16px rgba(15, 23, 42, 0.16);
  transform: translate(-50%, calc(-100% - 12px));
  animation: ft-bar-in 0.14s ease-out;
}
/* 入场仅做轻微上浮，避免每帧参与合成导致掉帧 */
@keyframes ft-bar-in {
  from {
    opacity: 0;
    transform: translate(-50%, calc(-100% - 4px));
  }
}
.ft-id {
  flex: none;
}
.ft-name {
  flex: none;
  max-width: 110px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
  color: var(--cad-muted);
}
.ft-input {
  flex: 1;
  min-width: 120px;
}
.ft-go {
  flex: none;
}
</style>
