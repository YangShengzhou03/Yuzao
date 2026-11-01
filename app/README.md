# 语造 工作台（Tauri 2 + Vue 3）

CAD 风格的桌面工作台：三维画布 + 菜单栏 + 底部指令输入。

## 界面结构

| 区域 | 内容 |
|---|---|
| 菜单栏 | 文件 / 编辑 / 视图 / 特征 / 工具 / 帮助 |
| 工具条 | 标准视图、线框/着色、网格/坐标轴、适应、校验、重建 |
| 画布 | three.js 三维视口（网格、坐标轴、视角立方、拾取、光标坐标） |
| 侧栏 | 模型树 / 属性 / 会话 |
| 指令行 | `命令:` 输入框，支持自然语言指令与 FeatureSpec JSON |
| 状态栏 | 光标坐标、单位、视图、模型、特征数、内核状态 |

## 交互约定

- 左键旋转、中键平移、右键平移、滚轮缩放
- 点击画布实体 → 拾取特征 id → 属性面板显示参数（对应目标 G2）
- 命令行支持：`help`、`等轴测/前视/顶视/右视…`、`适应`、`线框/着色`、`网格`、`坐标轴`、`单位 mm|inch`、`校验`、`重建`、`清空`，以及直接粘贴 FeatureSpec JSON

## 目录

```
src/
  App.vue              # 工作台外壳：菜单栏 / 工具条 / 画布 / 侧栏 / 指令行 / 状态栏
  viewport/scene3d.ts  # three.js 视口：相机、网格、视角立方、拾取、标准视图
  model/model.ts       # FeatureSpec JSON → 预览实体 + 模型树节点
src-tauri/             # Rust 调度层：validate_spec 等 Tauri 命令
```

## 命令

```bash
npm run dev     # 仅前端（Vite）
npm run start   # tauri dev（完整桌面窗口）
npm run lint    # vue-tsc 严格类型检查
npm run build   # 前端产物构建
```
