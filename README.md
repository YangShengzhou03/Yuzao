# 语造 Yuzao — 自然语言驱动的 Creo 原生参数化建模

用自然语言生成/修改**真正的 Creo 特征树**（`.prt` / `.asm`），而不是生成网格、STEP 或"一次性几何"。

## 目标（验收口径 G1–G4，与 ROADMAP.md §0 一致）

| 编号 | 目标 | 验收方式 |
|---|---|---|
| G1 | 自然语言 → 原生特征树文件 | 生成的 `.prt/.asm` 打开后，模型树里能看到草图/拉伸/孔/圆角等**特征节点**（不是导入实体） |
| G2 | 画布拾取 → 自然语言改特征 | 点某个面/边 → 定位到具体特征 id → 改参数 → **只重算下游**，模型树结构不变 |
| G3 | 特征级修改，非重建 | 编辑后特征总数不减少、特征 id 不变，`Regenerate` 而非 `New` |
| G4 | 工程师原生可编辑 | 用原版 Creo 打开，能继续编辑特征参数/草图/装配约束，与人工 Creo 画图一致 |

> G4 决定了**内核必须是用户本机那份 Creo Parametric**（见 ADR-009 与 `docs/Creo路线方案对比.md`）。

## 技术栈

| 层 | 选型 | 状态 |
|---|---|---|
| 界面 | Vue 3 + Vite + Tauri 2（Rust） | ✅ 空壳构建验证通过（4.4 MB exe） |
| 调度 | Rust：FeatureSpec IR 校验/计划（`spec/rust`，P1.1 ✅）/ LLM 调用 / 任务队列 / Undo-Redo | 部分落地 |
| 真值内核 | 用户本机 Creo 12.4（TOOLKIT / Object TOOLKIT） | ✅ P0 通过：无界面 Creo 建零件并存出原生 `.prt` |
| 视图层 | USD（`.usda`）+ 特征 ID 元数据，前端渲染与拾取 | ✅ NVIDIA `usd-brep` 写入/读回/网格化已验证 |

## 关键结论

- **R2（Toolkit DLL 内嵌）能力上完整满足 G1–G4**（`protk_unlock` 待验证），是生产级主线；**R1（OTK 异步 exe）接近满足**（G2 需自建映射），作先导 / MVP 最快路径；R5（Pro/PROGRAM）作参数微调补充（路线角色见 ADR-010）。
- R3（J-Link）仅作验证替代；R4（Trail 回放）仅售前演示、**禁做产品主线**；R6（FreeCAD）仅作无 Creo 客户的**降级备胎**（G4 不成立）。
- Creo **没有 USD 导出组件**，视图层必须自建：`Creo → STEP/VRML/STL → .usda(BrepArray 或网格) + customData.featureId`。
- 所有路线都必须自建 **FeatureSpec IR**（LLM 只输出受控 JSON，不直接写 API 调用）。

## 仓库结构

```
app/                     # Tauri 2 + Vue3 前端与 Rust 调度层（空壳已构建验证）
  src/                   #   对话 UI · 特征树 · USD 画布
  src-tauri/             #   Rust：LLM 调用 / FeatureSpec 解析 / 任务队列 / Undo-Redo
native/
  r1_otk/               # 先导路线 R1：OTK 异步 pfc 探针（P0 连通性验证，probe_otk.cpp）
  r2_toolkit/           # 主线 R2：C Toolkit 元素树特征探针（P1，probe_feat.c）
  r5_program/           # 配件路线 R5：Pro/PROGRAM + 关系式 参数微调（待实现）
  creo_bridge/          # 正式内核落点：R1 aicad_creo.exe / R2 aicad_protk.dll（探针毕业后迁入）
  start_creo.bat        # 共享：拉起无界面 Creo（-g:no_graphics -i:rpc_input）
  stop_creo.bat         # 共享：清理 Creo 进程
spec/                    # FeatureSpec IR 定义与示例（LLM 的唯一输出格式，ADR-012）
  featurespec.schema.json #   JSON Schema 严格模式（语法层单一来源）
  featurespec.example.json#   最小示例
  examples/              #   可执行示例 spec（feat_build 输入）
  rust/                  #   Rust crate：类型 + 语义校验（FS1xx）+ graph/idmap/edit_plan/plan + CLI（P1.1 ✅）
tools/
  usd/                   # USD 侧探针（usd-brep 写读/网格化）
  docs/                  # 文档一致性自动校验（离线，consistency_check.py）
docs/                    # 架构、路线对比、竞品、可行性分析、ADR、文档同步规则
```

> 架构决策记录（ADR-009~012）见 `docs/adr/`；排期与近期行动见 `ROADMAP.md`；路线可行性分析见 `docs/方案路线可行性分析.md`。

## 当前状态与下一步

> 完整阶段定义、验收标准与任务分解（P0–P6 / `P{n}.{k}`）以 [`ROADMAP.md`](ROADMAP.md) 为唯一权威，下表为状态摘要。

| 阶段 | 内容 | 状态 |
|---|---|---|
| P0 | 程序 → 无界面 Creo → 存出一个 `.prt` | ✅ 已通过（`parametric.exe -g:no_graphics -i:rpc_input` + OTK `Connect`，产出 42 KB 原生 `.prt` 与 STEP） |
| P1 | FeatureSpec IR + 草图/拉伸/孔 构建 | 🔧 进行中（`P1.1` ✅ FeatureSpec Schema + Rust 校验器已落地；`P1.2`/`P1.3` 构建器代码与编译完成，特征创建运行待混合模式：C API 连不上 `rpc_input` 会话） |
| P2 | 视图导出 + 特征 ID 注入 + 前端点选回传 | 未开始 |
| P3 | 装配、圆角/阵列、多轮上下文、Undo/Redo | 未开始 |
| P4 | 2D/DWG/工程图（入场生死线，视市场窗口） | 未排期（ODA 授权门禁） |
| P5 | 真实 LLM 接入与多 Agent 协同 | 未开始 |
| P6 | 信创/国产化与云协同 | 未开始 |

P0 判定规则已兑现：async `Start()` 未通 → 用「自 spawn + Connect」绕过，P0 达成。P1 若 pfc/C API 混合模式仍打不通，直接转 R2（Toolkit DLL 内嵌），不在同一路径反复试错。

## 开发环境

- Windows 10/11 x64，Creo Parametric 12.4（客户自持授权）
- VS 2022（C++ x64 工具链，用于 Toolkit/OTK 原生模块）
- Node 20+ / pnpm（Vue + Tauri）
- Python 3.12（USD 侧工具；`usd-brep` 仅支持 CPython 3.12）

## 合规

本产品是运行在**客户已授权 Creo 之上**的伴侣程序，不包含也不分发 Creo 本体。
Toolkit 开发授权仅在开发/解锁阶段需要；`protk_unlock` 之后终端用户无需开发授权。
