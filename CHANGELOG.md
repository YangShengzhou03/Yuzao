# 变更日志

## 2026-11-02 — 项目更名为「语造 Yuzao」

- 品牌层更名：`AI-CAD` → `语造 / Yuzao`。`productName` → `Yuzao`，`identifier` → `com.yuzao.app`，窗口标题 → `语造`，`package.json` name → `yuzao`。
- 文档/文案同步：`README.md`、`app/README.md`、`app/index.html`、`docs/竞品对标与功能规划.md`、`docs/方案路线可行性分析.md`、`LICENSE`、`tools/usd/usd_probe.py` 注释。
- **有意保留**内部标识符：crate 名 `aicad-tauri-probe` / `aicad_tauri_probe_lib`、`aicad_creo.exe` / `aicad_protk.dll`、`[aicad]` 协议前缀、`aiCad:*` USD 属性名、`.aicad.json` 格式名、schema `$id`。

## 2026-11-01 — P1.1–P1.3 代码落地：FeatureSpec IR + FeatureSpec 驱动构建器

**P1.1（`ROADMAP P1.1`，✅ 已完成）FeatureSpec IR 固化**
- 新增 `spec/featurespec.schema.json`：JSON Schema（draft 2020-12，严格 `additionalProperties:false`），
  覆盖 sketch/extrude/hole/edits 语法约束；编译期内嵌于 Rust crate 保持单一来源。
- 新增 `spec/rust/`（crate `featurespec`）：serde 严格解析 + 跨字段语义校验
  （重复 id、未知/前向引用、引用类型不匹配、拓扑引用语法 `F002.face[top]`、hole 仅可落在材料面、
  op 组合合法性、edits 目标解析与去重、非有限/非正值拒绝），
  错误码体系 `FS1xx`，**收集全部错误**供 LLM 一次修正；
  `cargo test` **42/42 通过**；`validate` / `plan` CLI 校验示例 spec 通过。
- 调度层原语（为 `P1.2`–`P2.3` 提供，离线可测）：
  `graph`（依赖 DAG + 传递下游闭包）、`edit_plan`（改参 → `Regenerate` 集合，保证 G3 不重建）、
  `idmap`（语义 id ↔ Creo 内部 id 双向映射，可 JSON 持久化）、`plan`（首次构建有序步骤）。
- `featurespec plan <spec.json> [--json]`：打印构建顺序/依赖与改参重算闭包；退出码 0/1/2 便于 CI。
- `spec/featurespec.schema.json` 与 Rust 语义同步：pattern `n ≥ 2`、hole `on` 必须带面名、
  编辑目标唯一；`spec/rust/src/main.rs` 读取时剥离 UTF-8 BOM（Windows 编辑器兼容）。
- 删除重复的临时实现 `crates/aicad-spec`，FeatureSpec 只保留 `spec/rust` 单一权威实现。
- `app/src-tauri`：接入 `featurespec` crate，新增 `validate_spec` Tauri command；`cargo check` 通过。

**P1.2 / P1.3（`ROADMAP P1.2`/`P1.3`，🔧 代码落地，待实机验证）FeatureSpec 驱动构建器**
- 新增 `native/r2_toolkit/feat_build.c`（约 1050 行）：内嵌严格 JSON 解析器
  （拒绝重复键/尾随逗号/非法转义/超深）、FeatureSpec 校验（与 Rust 规则对齐的防御子集）、
  首特征 sketch+extrude 元素树（对称/单向深度）、直孔元素树 + 矩形阵列多实例、
  `Regenerate` 后特征树回读校验（实体特征数与 spec 预期一致，不符即 `FS214`）、结构化错误码 `FS2xx`。
- 元素树依据**本机 Creo 12.4 官方资料**逐项核对（非凭记忆）：
  `ptu_featcreat/UgSktFirstFeatureCreate.c`（首特征，probe_feat.c 已验证同构）、
  `ptu_featcreat/UgHoleCreate.c`（直孔 `PRO_E_HLE_COM`/`PRO_E_HLE_PLACEMENT`）；
  放置参考由官方示例的 `ProSelect`（UI）改为程序化解析（`ProSolidSurfaceVisit` + 平面几何 + datum 按名查找）。
- 编译链接通过（MSVC 19.44 /W4，`build_feat.bat` 同脚本加建 `feat_build.exe`）。
- 免 Creo 冒烟实测（解析/校验路径，退出码符合设计）：
  文件缺失 `FS200`；JSON 截断 `FS201`；负孔径 `FS140`；非法基准面 `FS133`；
  合法 spec（`spec/examples/plate_with_holes.json`）校验通过后在连接步报 `FS209` 退出 3。
- 新增 `run_build.bat`（运行入口）、`spec/examples/plate_with_holes.json`（含 pattern/双孔/edits 的可执行示例）。

**边界（诚实声明，与 ROADMAP 口径一致）**
- 运行时特征创建仍受已知阻塞：C API `ProEngineerConnect` 连不上 `rpc_input` 会话（-4），
  需混合模式（OTK 连接 + C API 建特征）或普通会话，见 `docs/文档同步规则.md` 技术结论条目；
  故 `P1.2`/`P1.3` 状态为 🔧（代码+编译+校验路径落地），非 ✅。
- v1 执行器边界已在 `feat_build.c` 头注声明：仅 part、仅首特征对、hole 仅 top/bottom 通孔、
  pattern 以多实例直孔执行（原生阵列在 `P3.2`）、edits 由调度层在 `P2` 执行。

## 2026-11-01 — 全量文档一致性终审（SSOT 对齐 + 自动校验）

**统一口径（目标 / 路线 / 阶段 / 工期 / 编号）**
- `ROADMAP.md`：一致性规则引用修正为真实文件 `docs/文档同步规则.md`；任务状态统一为「未开始」；
  MVP 工期由与阶段估算冲突的 4–8 周修正为 **P1 3–5 周 + P2 3–5 周 = 6–10 周**。
- `docs/文档同步规则.md`：升级为完整映射规范——新增文档职责边界、编号命名空间
  （`G1–G4` / `P0–P6` / `P{n}.{k}` / `R1–R6` / `T0–T4` / `M0–M6`）、状态词表、自动校验入口与终审清单；
  修正 G1–G4 权威源为 ROADMAP §0、任务编号为 `P{n}.{k}`、市场层级为 `T0–T4`。
- `docs/方案路线可行性分析.md`：R1/R2 工期改为与 ROADMAP §5 同口径（MVP 6–10 周、P1–P3 11–17 周）；
  pfc 特征创建事实与 `P1.4` 探针口径统一；治理结论不再表述为「治理基线缺失」。
- `docs/Creo路线方案对比.md`：G1–G4 标明以 ROADMAP §0 为权威；R1 做法改为 spawn+Connect、
  特征创建走 C Toolkit 混合模式；P2/P3 工期由「天」修正为与 ROADMAP 一致的「周」。
- `docs/ARCHITECTURE.md` §8：区分 P0 完整链路证据（R1）与 R2「仅编译通过」；补 pfc 无特征创建
  Instructions、R2 连接 -4、FeatureSpec 未落地等现状。
- `docs/竞品对标与功能规划.md`：市场优先级改用「1/2/3」避免与阶段 `P0–P6` 冲突；旧 `Phase` 映射改为
  无现行对应或真实 `P{n}.{k}`；`P3 门禁（G3）`改为「P3 验收评审」。
- `README.md` / `spec/README.md` / `native/*/README.md`：状态符号与阶段口径统一（🔧 / 未开始 / 未排期）；
  R2 完整满足、R1 接近满足的路线定位与 ADR-010 对齐；`aicad_protk.dll` 纳入正式内核落点。

**机制（防复发）**
- 新增 `tools/docs/consistency_check.py`：校验死链、废止编号、状态词、阶段状态、目标 G1–G4、
  路线角色、阶段工期、资源配置（FTE/并行探针）、里程碑 M0–M6 定义完整性。
- `CONTRIBUTING.md`：提交前必须运行一致性校验（0 error）；ADR 落地位置明确为 `docs/adr/` + §7 索引。

**验证**：`python tools/docs/consistency_check.py` → **19 份文档，0 error**。

## 2026-11-01 — 全量文档交叉校验 + 建立文档同步机制

**交叉校验修复（六维度：目标/路线/阶段/技术事实/路径引用/验收与资源）**
- `ROADMAP.md`：补「任务分解」表（执行任务统一编号为 `P{n}.{k}`，如 `P4.4`；竞品文档此前引用的
  T4.1/T4.4/T5.5/T6.6 等旧编号已统一映射到 `P{n}.{k}`）与「§9 变更评审」节；注明执行任务 `P{n}.{k}`
  与市场层级 `T0–T4` 的区分（两套命名空间，禁止混用）。
- `docs/竞品对标与功能规划.md`：修正 4 处错误章节引用（`ARCHITECTURE.md` §7 数据交换 /
  §5.3 / 架构 M5 均不存在）改为真实出处；「P1 已落地最小闭环」改为「P1 探针进行中」。
- `docs/方案路线可行性分析.md`：4 处「PHASES.md 残留悬空引用」表述更新为「已解决」
  （竞品文档引用已在本轮统一改指 ROADMAP）。
- `docs/Creo路线方案对比.md`：目标原文出处改为「原始需求，全文引录如下」
  （`新方案.md` 已从仓库移除，避免死链）。
- `README.md`：目录树移除已不存在的 `新方案.md`，补 `docs/文档同步规则.md`。

**同步机制（防复发）**
- 新增 `docs/文档同步规则.md`：事实→唯一权威来源映射表（阶段/任务/路线/目标/验证状态/
  实测数据/市场编号共 9 类）、引用规则（禁止死链）、变更同步流程、发布前一致性审核清单。
- `CONTRIBUTING.md`：约定第 0 条——改文档先查映射表，改权威源后必须同步引用方。

## 2026-11-01 — 目录按路线重组（R1/R2/R5）+ 全文档一致性核查

**目录重组**
- `native/creo_probe/` 拆分为 `native/r1_otk/`（R1 OTK 异步探针，P0）与
  `native/r2_toolkit/`（R2 C Toolkit 特征探针，P1）；`start_creo.bat`/`stop_creo.bat`
  提升到 `native/` 共享；新增 `native/r5_program/`（R5 占位）与 `native/creo_bridge/`
  （正式内核 `aicad_creo.exe` 落点）。
- 清理重组遗留的空目录 `native/creo_probe/`。

**文档一致性核查（本轮修复）**
- `docs/Creo路线方案对比.md`：证据表、路线表、排期表中「async 未打通/启动待修」的
  过时表述，全部更新为「P0 已通过（spawn+Connect）」；§7 排障清单标记已有结论。
- `README.md`：状态表 P1 改为「进行中（混合模式待验）」；`docs/adr/`、`ROADMAP.md`、
  `native/r2_toolkit/README.md` 实体化后引用关系全部对上。
- `native/r1_otk/README.md`：标题与内部路径从 `creo_probe` 改为 `r1_otk`；
  P1 指引改指 `../r2_toolkit/probe_feat.c`；修正 start_creo 工作目录描述。
- `native/r2_toolkit/probe_feat.c`：头注释「route R1」改为「R1/R2 mixed mode」，
  修正 `protookit` 拼写。
- 结论层核对：R2 主线 / R1 先导 / R5 参数微调补充 / R6 降级，README、
  ARCHITECTURE、Creo路线方案对比、方案路线可行性分析 四份文档口径一致。

## 2026-11-01 — 仓库骨架按新架构重建 + P1 探针（C Toolkit 元素树）

**变更**
- `app/`：已验证的 Tauri 2 + Vue3 空壳（原 `D:\build\probe\aicad-tauri-probe`）迁入仓库。
- 新增 `spec/`：FeatureSpec IR 说明与示例 JSON（LLM 唯一输出格式，ADR-012）。
- 新增 `native/creo_bridge/`：正式内核 `aicad_creo.exe` 的落点说明。
- 新增 `docs/方案路线可行性分析.md`：R1–R6 深度可行性分析。
- 新增 `native/creo_probe/probe_feat.c` + `build_feat.bat` + `run_feat.bat`：
  P1 探针，改用 **C Toolkit 元素树**（`ProFeatureWithoptionsCreate`，参照
  `UgSktFirstFeatureCreate.c`）建「草图矩形 + 拉伸」第一特征。
  起因：OTK pfc C++ API 没有特征创建 Instructions（`ProExtrude.h` 的元素树
  只在 C API 侧），特征树必须走 `ProFeatureWithoptionsCreate`。
- 删除 `.screenshots/`、探针中间产物（obj/exp/lib）。

**验证状态**
- C API 探针编译通过（`pt_asynchronous.lib` + `protoolkit_NU.lib`，`ProProcessHandle` 为 `PeerIdx` 非
  指针）。
- ⚠️ `ProEngineerConnect("",NULL,NULL,text,…)` 对 `-i:rpc_input` 会话返回 -4（PRO_TK_NOT_FOUND），
  即 C API 连不上 rpc_input 模式的会话；OTK 的 `pfcAsyncConnection::Connect` 可以。
  下一步：探针改回「OTK Connect 建会话 + C API 干活」或验证 C API 只能连
  GUI/普通会话（`ProEngineerSessionIdGet` / `ProEngineerConnectIdExtract` 路线）。

## 2026-11-01 — 内核路线切换：FreeCAD → 客户本机 Creo（Toolkit/OTK）

**变更**
- 删除 FreeCAD/Qt/Web 原型代码（`core/`、`ui/`、`app/`、`ai/`、`projects/`、根 `CMakeLists.txt`、FreeCAD 构建脚本）；
  完整历史状态保留在 git 快照 `chore: 转向 Creo/Toolkit 路线前的完整快照`。
- 文档重写：`README.md`、`docs/ARCHITECTURE.md`（ADR-009~012）、`CONTRIBUTING.md`、`.gitignore`。
- 新增 `docs/Creo路线方案对比.md`：6 条路线对比与实测证据。
- 新增 `native/creo_probe/`：R1（OTK 异步）P0 探针（`probe_otk.cpp` + `build.bat` + `run.bat`）。
- `scripts/usd_probe.py` 迁至 `tools/usd/usd_probe.py`。

**已验证**
- Creo 12.4 正常启动（`parametric.exe` / `xtop.exe` / `nmsd.exe` / `pro_comm_msg.exe`）。
- Toolkit 开发包与 Object TOOLKIT 已安装（头文件、`protoolkit_NU.lib`、`pt_asynchronous.lib`、`otk_cpp_async.lib`、`otk_222.lib`）。
- 授权特性含 `TOOLKIT` / `ObjectToolkit` / `ObjectToolkitJava`。
- 探针 MSVC 2022 x64 编译链接通过。
- `usd-brep` 写 B-Rep 进 `.usda`、读回、网格化（布尔差体积误差 0.002%）。
- Vue3 + Tauri 2 空壳构建可运行（4.4 MB exe）。

**未通过 / 阻塞**
- `pfcAsyncConnection::Start` 抛 `ProEngineerConnectStart` 一般故障；已改用「自己 spawn + `Connect()`」绕过。

## 2026-11-01 — P0 打通：无界面 Creo 建零件并落盘

**已验证**
- `parametric.exe -g:no_graphics -i:rpc_input` 可拉起全栈无界面 Creo（`parametric` / `xtop` / `nmsd` / `pro_comm_msg`）。
- OTK `pfcAsyncConnection::Connect(NULL, NULL, text, 120)` 连接成功，`CreatePart` → `Save` → `Export STEP` → `Disconnect` 全程无异常。
- 产物：`aicad_probe.prt.1`（42,855 B，文件头 `#UGC:2 PART 2552`）、`aicad_probe.step`（3,463 B，`ISO-10303-21`，来源 `CREO PARAMETRIC`）。
- 环境要点：`PRO_COMM_MSG_EXE` 必须设置；`PATH` 必须含 `Common Files\x86e_win64\obj`（否则缺 `ucore68.dll`）。

**新增**
- `native/creo_probe/start_creo.bat`、`stop_creo.bat`；`run.bat` 增加 `connect` 模式。

## 更早（已归档至 git 历史）

- C++ 原生几何内核骨架、Qt 外壳、Python AI 服务、Three.js 画布原型、FreeCAD 1.1.4 构建方案（ADR-007/008）。

