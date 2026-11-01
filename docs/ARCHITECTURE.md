# 架构（Creo 原生参数化路线）

> 状态：已定稿。替代此前 FreeCAD/Qt 时代的 ADR-007 / ADR-008（历史快照见 git 提交
> `chore: 转向 Creo/Toolkit 路线前的完整快照`）。

## 1. 分层

```
┌─────────────────────────────────────────────────────────┐
│ 前端  Vue3 + Vite（Tauri WebView）                        │
│  对话 UI · 特征树 · 参数面板 · USD 画布（渲染 + 拾取）      │
└───────────────┬─────────────────────────────────────────┘
                │ Tauri IPC (JSON)
┌───────────────▼─────────────────────────────────────────┐
│ 调度层  Rust（Tauri 主进程）                              │
│  LLM 调用 · FeatureSpec 解析/校验 · 特征ID映射            │
│  任务队列 · 版本/Undo-Redo · USD 导出编排                 │
└───────────────┬─────────────────────────────────────────┘
                │ 本地 IPC（命名管道）
┌───────────────▼─────────────────────────────────────────┐
│ 真值内核  Creo Parametric（客户已安装）                    │
│  R1: aicad_creo.exe（OTK 异步）  /  R2: aicad_protk.dll   │
│  特征创建 · 参数修改 · Regenerate（DAG 最小重算）· 保存    │
└───────────────┬─────────────────────────────────────────┘
                │ 导出 STEP / VRML / STL
┌───────────────▼─────────────────────────────────────────┐
│ 视图层  .usda（BrepArray 或网格 + customData.featureId）  │
└─────────────────────────────────────────────────────────┘
```

**铁律**：真值只有一份——Creo 的 `.prt/.asm`。USD 是快照，任何编辑都回到 Creo 上执行。

## 2. 为什么这样分工

| 需求 | 承担者 | 说明 |
|---|---|---|
| 原生特征树 / 最小重算 | Creo | 只有 Creo 能产出工程师可继续编辑的 prt/asm |
| NL → 结构化意图 | Rust + LLM | LLM 只产出 JSON，不产出 API 调用或脚本 |
| 画布交互 / 拾取 | Vue + USD | USD 里挂特征 ID，点击即定位到特征 |
| 稳定拓扑引用 | Rust 特征ID映射表 | 语义引用（`F002.face[top]`）与 Creo 内部 id 双向映射 |

## 3. FeatureSpec IR（唯一写入内核的输入）

```json
{
  "model": "base_plate",
  "type": "part",
  "features": [
    { "id": "F001", "op": "sketch",  "plane": "TOP", "profile": "rect",
      "dims": { "w": 100, "h": 60 } },
    { "id": "F002", "op": "extrude", "sketch": "F001", "depth": 10,
      "side": "symmetric" },
    { "id": "F003", "op": "hole",    "on": "F002.face[top]",
      "dia": 6.0, "pattern": { "type": "rect", "n": [2, 2], "pitch": [80, 40] } }
  ],
  "edits": [
    { "target": "F003.dia", "value": 7.0 }
  ]
}
```

- `features` 用于首次构建；`edits` 用于增量修改（**结构相同**，天然区分"建"与"改"）
- `edits` 只改参数 → 后端按 id 定位已有特征 → `Regenerate`，**不新建模型**
- 引用语法 `F002.face[top]` 由映射表解析，绝不把 face index 交给 LLM 猜

## 4. 增量编辑语义（对应目标第 3 条）

| 用户操作 | 内核动作 | 结果 |
|---|---|---|
| 新建 | `CreatePart` + 按 `features` 顺序创建特征 | 特征树增长 |
| 改参数 | 定位特征 → 改 para → `Regenerate` | 特征 id 不变，只重算下游 |
| 删除特征 | 删除节点 → `Regenerate` | 下游按依赖重算或标红 |
| 撤销 | 版本快照回滚（按提交的参数集重放或回滚文件副本） | 可回溯 |

## 5. 视图桥（Creo → USD）

Creo 不产出 USD，固定链路：

1. 每次 Regenerate 后导出 `STEP`（B-Rep）或 `VRML/STL`（网格）
2. 转成 `.usda`：B-Rep 走 NVIDIA `usd-brep` 的 `BrepArray`；拾取只需要网格时可直接用 `UsdGeomMesh`
3. 写入 `customData`：`aiCad:featureId`、`aiCad:params`、`aiCad:dependsOn`
4. 前端点击 → 读 `customData.featureId` → 回传 `{target, param}`

已验证（`tools/usd/usd_probe.py` + `usd-brep`）：写 B-Rep、读回、`tessellate` 均通，
布尔差体积误差 0.002%，`.usda` 6.7 KB。

## 6. 进程与错误模型

- **R1**：`aicad_creo.exe` 常驻。**先自己 spawn** `parametric.exe -g:no_graphics -i:rpc_input`
  得到一个无界面 Creo（无窗口、无图形栈），再用 `pfcAsyncConnection::Connect()` 挂上去，
  Rust 通过管道发指令。**不要用 `Start()`**（实测四种命令形态均失败，见
  `native/r1_otk/README.md`）。
- **R2**：`aicad_protk.dll` 由 Creo 启动时加载，Creo 作为子进程由 Tauri 拉起（可最小化/无界面参数）
- 错误分层：`pfcXToolkitError`（内核级）→ 结构化错误码 → Rust 侧重试/回滚 → 前端提示
- 崩溃隔离：Creo 进程独立，崩溃后可重启并重放版本快照

## 7. ADR 索引

| ADR | 决策 | 状态 |
|---|---|---|
| ADR-009 | 真值内核切换到客户本机 Creo（放弃 FreeCAD 作为主路线） | 已采纳 |
| ADR-010 | 内核接入优先 R2（Toolkit DLL），R1（OTK 异步）作为先导 | 已采纳 |
| ADR-011 | USD 仅作视图/拾取层，不承载参数化真值 | 已采纳 |
| ADR-012 | LLM 只输出 FeatureSpec JSON，禁止直接生成 API 调用/脚本 | 已采纳 |

## 8. 验证状态

> 阶段状态与验收标准的唯一权威为 [`ROADMAP.md`](../ROADMAP.md) §2；本节只记录架构链路的实测现状。

**已通过（P0）**：外部进程 → 无界面 Creo → `CreatePart` → `Save`（原生 `.prt` 42 KB）
→ 导出 STEP（`ISO-10303-21`，`CREO PARAMETRIC` 头）→ `Disconnect`。
环境条件：`PRO_COMM_MSG_EXE` 指向 `pro_comm_msg.exe`；`PATH` 含 `Common Files\x86e_win64\obj`；
Creo 以 `-g:no_graphics -i:rpc_input` 启动。完整链路证据见 `native/r1_otk/`（R1 OTK）；
`native/r2_toolkit/`（R2 C Toolkit）当前**仅编译通过**，运行尚未跑通（`ProEngineerConnect` 对 rpc_input 会话返回 -4）。

**已通过（离线，P1.1）**：FeatureSpec IR 在 `spec/rust/`（crate `featurespec`）落地——
严格解析与语义校验（`FS1xx`，收集全部错误）、依赖图/下游闭包、改参 `Regenerate` 计划、
语义 id ↔ Creo 内部 id 映射、构建计划与 `validate`/`plan`/`schema` CLI；`cargo test` 42/42 通过，
示例 spec（`spec/featurespec.example.json`、`spec/examples/plate_with_holes.json`）校验/规划通过。
该链路不依赖 Creo，证据与命令见 `spec/rust/README.md` 与 `CHANGELOG.md`（验证状态唯一权威仍为 ROADMAP §2）。

**待验证**：
- pfc **无逐特征创建 Instructions**（已确认）→ 特征创建必须走 C Toolkit；待验证「OTK 连接 + C API 建特征」混合模式与圆角/阵列/扫描/装配覆盖度（`ROADMAP P1.4`）
- `Regenerate` 后 `.prt` 里是否保留完整特征树（`ROADMAP P1` 的验收点）
- R2 的 `protk_unlock` 注册与 DLL 加载

