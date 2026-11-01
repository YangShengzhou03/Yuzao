<script setup lang="ts">
import { DataAnalysis, Document, Files, FolderOpened, InfoFilled } from "@element-plus/icons-vue";
import { useCad } from "../state/cad";

const {
  sideOpen,
  sideTab,
  modelName,
  featureCount,
  treeData,
  selected,
  editableParams,
  specText,
  running,
  messages,
  opLabel,
  onNodeClick,
  onParamEdit,
  validateSpec,
  rebuild,
} = useCad();
</script>

<template>
  <aside v-if="sideOpen" class="side">
    <el-tabs v-model="sideTab" class="side-tabs">
      <el-tab-pane name="tree">
        <template #label><el-icon class="tab-ic"><Files /></el-icon>模型树</template>
        <el-scrollbar>
          <div class="pane-pad">
            <div class="pane-head">
              <span>{{ modelName }} · {{ featureCount }} 个特征</span>
            </div>
            <el-tree
              class="tree"
              :data="treeData"
              :props="{ label: 'label', children: 'children' }"
              node-key="id"
              :current-node-key="selected?.id ?? null"
              highlight-current
              default-expand-all
              :expand-on-click-node="false"
              @node-click="onNodeClick"
            >
              <template #default="{ data, node }">
                <span class="tree-node">
                  <el-icon :size="15" class="tn-ic">
                    <FolderOpened v-if="node.level === 1" />
                    <Document v-else />
                  </el-icon>
                  <template v-if="node.level === 1">{{ data.label }}</template>
                  <template v-else>
                    <span class="tn-id">{{ data.id }}</span>
                    <span class="tn-label">{{ data.label }}</span>
                  </template>
                </span>
              </template>
            </el-tree>
            <el-text class="note">
              模型树节点即 Creo 原生特征（G1）；点击节点可在「属性」页查看参数。
            </el-text>
          </div>
        </el-scrollbar>
      </el-tab-pane>

      <el-tab-pane name="props">
        <template #label><el-icon class="tab-ic"><InfoFilled /></el-icon>属性</template>
        <el-scrollbar>
          <div class="pane-pad">
            <div class="pane-head">
              <span>{{ selected ? selected.label : "属性" }}</span>
              <el-tag v-if="selected" type="primary" effect="plain">{{ selected.id }}</el-tag>
            </div>
            <el-descriptions v-if="selected" :column="1" border class="props-desc">
              <el-descriptions-item label="操作">{{ opLabel(selected.op) }}</el-descriptions-item>
              <el-descriptions-item v-for="[k, v] in selected.params" :key="k" :label="k">
                {{ v }}
              </el-descriptions-item>
            </el-descriptions>
            <el-empty v-else :image-size="64" description="未选中特征，点击画布或模型树节点" />

            <template v-if="editableParams.length">
              <div class="pane-head">参数编辑</div>
              <div class="param-edit">
                <label v-for="p in editableParams" :key="p.target" class="param-row">
                  <span class="param-label">{{ p.label }}</span>
                  <el-input-number
                    :model-value="p.value"
                    :min="0.1"
                    :step="1"
                    size="small"
                    controls-position="right"
                    @change="(v: number | undefined) => onParamEdit(p.target, v)"
                  />
                </label>
              </div>
              <el-text class="note" size="small">
                修改即写入 FeatureSpec 的 edits 并重建预览；内核连接后对应 Regenerate（G3）。
              </el-text>
            </template>

            <div class="pane-head">FeatureSpec 输入</div>
            <el-input v-model="specText" type="textarea" class="spec-input" :rows="10" />
            <div class="pane-actions">
              <el-button type="primary" :loading="running" @click="validateSpec">校验</el-button>
              <el-button @click="rebuild">重建</el-button>
            </div>
          </div>
        </el-scrollbar>
      </el-tab-pane>

      <el-tab-pane name="log">
        <template #label><el-icon class="tab-ic"><DataAnalysis /></el-icon>会话</template>
        <el-scrollbar>
          <div class="pane-pad">
            <div class="pane-head">
              <span>命令与响应</span>
              <el-popconfirm
                title="确定清空全部会话记录？"
                confirm-button-text="清空"
                cancel-button-text="取消"
                @confirm="messages = []"
              >
                <template #reference>
                  <el-button text>清空</el-button>
                </template>
              </el-popconfirm>
            </div>
            <el-timeline class="timeline">
              <el-timeline-item
                v-for="(m, i) in messages"
                :key="i"
                :timestamp="m.time"
                :type="m.role === 'user' ? 'primary' : m.role === 'assistant' ? 'success' : 'info'"
                :hollow="m.role !== 'user'"
              >
                <div class="msg-who">{{ m.role === "user" ? "你" : m.role === "assistant" ? "AI" : "系统" }}</div>
                <pre class="msg-text">{{ m.text }}</pre>
              </el-timeline-item>
            </el-timeline>
            <el-empty v-if="messages.length === 0" :image-size="64" description="暂无记录" />
          </div>
        </el-scrollbar>
      </el-tab-pane>
    </el-tabs>
  </aside>
</template>

<style scoped>
.side {
  width: var(--cad-side-w);
  flex: none;
  display: flex;
  flex-direction: column;
  background: var(--cad-panel);
  border-left: 1px solid var(--cad-line);
  min-height: 0;
}
.side-tabs {
  height: 100%;
  display: flex;
  flex-direction: column;
}
.side-tabs > :deep(.el-tabs__header) {
  flex: none;
  margin-bottom: 0;
  padding: 8px 12px 0;
}
.side-tabs > :deep(.el-tabs__content) {
  flex: 1;
  min-height: 0;
  padding: 0;
  overflow: hidden;
}
.side-tabs :deep(.el-tab-pane) {
  height: 100%;
}
.tab-ic {
  margin-right: 6px;
  vertical-align: -2px;
}
.pane-pad {
  padding: 10px 12px 14px;
}
.pane-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  font-size: 13px;
  font-weight: 600;
  color: #374151;
  margin-bottom: 8px;
}
.tree {
  border: 1px solid var(--cad-line);
  border-radius: var(--cad-radius);
  padding: 6px 0;
}
.tree-node {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  line-height: 1.5;
}
.tn-ic {
  flex: none;
  color: var(--cad-accent);
}
.tn-id {
  font-family: var(--cad-mono);
  font-size: 12px;
  color: var(--cad-accent);
  flex: none;
}
.tn-label {
  color: var(--cad-text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.note {
  margin-top: 10px;
  display: block;
  line-height: 1.5;
}
.props-desc {
  margin-bottom: 6px;
}
.param-edit {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-bottom: 8px;
}
.param-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  border-radius: var(--cad-radius);
  padding: 2px 4px;
  transition: background-color 0.12s ease;
}
.param-row:hover {
  background: #eef2f8;
}
.param-label {
  font-size: 13px;
  color: var(--cad-text);
}
.param-row .el-input-number {
  width: 120px;
}
.props-desc :deep(.el-descriptions__label) {
  width: 68px;
  font-size: 13px;
  color: var(--cad-muted);
}
.props-desc :deep(.el-descriptions__content) {
  font-family: var(--cad-mono);
  font-size: 12px;
}
.spec-input {
  margin-bottom: 10px;
}
.spec-input :deep(.el-textarea__inner) {
  font-family: var(--cad-mono);
  font-size: 12px;
  line-height: 1.5;
  background: #fbfcfe;
  border-radius: var(--cad-radius);
}
.pane-actions {
  display: flex;
  gap: 8px;
}
.timeline {
  padding: 2px 0 0 2px;
}
.msg-who {
  font-size: 12px;
  color: var(--cad-muted);
  margin-bottom: 3px;
}
.msg-text {
  font-size: 13px;
  line-height: 1.55;
}
</style>
