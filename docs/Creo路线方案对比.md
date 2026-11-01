# 达成「AI 生成 Creo 原生特征树 + 画布拾取微调」的可行路线对比

> 目标原文（原始需求，全文引录如下）：
> 1. 用户打字/语音描述 → AI 直接生成原生带完整特征树的 Creo prt / asm
> 2. 用户在 3D 画布点选任意特征 → 自然语言微调（改圆角、改孔、改草图、改装配）
> 3. 所有修改都是 Creo 原生特征级修改，不是重绘模型、不是几何编辑
> 4. 输出文件工程师可直接打开继续参数化画图，和人工 Creo 画图 100% 一致
>
> 本文只回答一个问题：**有哪些路线能实现或接近这个效果**，以及每条路线的实测状态。

---

## 0. 把目标拆成可验收的 4 条硬指标

> 验收口径（G1–G4）的唯一权威为 [`ROADMAP.md`](../ROADMAP.md) §0；本表是原始需求的拆解复述，措辞与 ROADMAP 保持一致。

| 编号 | 目标 | 验收方式 |
|---|---|---|
| G1 | 自然语言 → 原生特征树文件 | 生成的 `.prt/.asm` 打开后，模型树里能看到草图/拉伸/孔/圆角等**特征节点**（不是导入实体） |
| G2 | 画布拾取 → 自然语言改特征 | 点某个面/边 → 定位到具体特征 id → 改参数 → **只重算下游**，模型树结构不变 |
| G3 | 特征级修改，非重建 | 编辑后特征总数不减少、特征 id 不变，`Regenerate` 而非 `New` |
| G4 | 工程师原生可编辑 | 用原版 Creo 打开，能继续编辑特征参数/草图/装配约束，与人工 Creo 画图一致 |

**结论先行**：G4 这一条直接排除了所有非 Creo 内核的路线。要 100% 满足 4 条，内核必须是用户本机那份 Creo Parametric；差异化只发生在「用什么方式驱动它」。

---

## 1. 本机已核实的事实（决定路线可行性的证据）

| 项 | 实测结果 |
|---|---|
| Creo 版本 | `D:\Program Files\PTC\Creo 12.4.0.0`，`parametric.exe` + `xtop.exe` + `nmsd.exe` + `pro_comm_msg.exe` |
| Creo 能否正常启动 | ✅ 已实测：GUI 启动后 4 个进程齐全（随后已手动结束） |
| Toolkit 开发包 | ✅ **已安装**：`Common Files\protoolkit` 下有 430 个 `pro_*.h` 头文件、`x86e_win64\obj` 下的 `protoolkit_NU.lib` / `pt_asynchronous.lib` / `protk_dll_NU.lib`、`protk.dat`、GSG 与 tkuse 文档 |
| Object TOOLKIT (C++ pfc API) | ✅ **已安装**：`Common Files\otk\otk_cpp`（头文件 + `otk_cpp.lib` + `otk_cpp_async.lib` + 预编译 `otk_async_examples.exe` + 源码示例） |
| J-Link (Java pfc) | ✅ 类文件在（`Common Files\PRO_APPT\pfc*.class`），本地带 `creo_toolkit` 帮助文档 |
| 授权特性 | ✅ 授权文件里存在 `TOOLKIT`、`ObjectToolkit`、`ObjectToolkitJava` 等特性 |
| Creo 是否带 USD 组件 | ❌ **没有**：安装目录内无任何 USD/UsdBrep 相关文件，`Creo → USD` 需要自建导出桥 |
| Creo 能否 async 无界面启动 | ✅ **已打通（绕过方案）**：自写 OTK 探针（`native/r1_otk/probe_otk.cpp`，MSVC 编译链接成功）调用 `pfcAsyncConnection::Start` 四种形态均抛 `ProEngineerConnectStart` 故障 → **改用自 spawn `parametric.exe -g:no_graphics -i:rpc_input` + `Connect()`，P0 全链路通过**；GUI 启动正常说明非安装/许可证问题 |
| USD 侧能力 | ✅ 已验证：NVIDIA `usd-brep` 可写 B-Rep 进 `.usda`、可 `import_breps` 读回、可 `tessellate` 出网格（`.usda` 6.7 KB，布尔差体积误差 0.002%） |
| 前端 | ✅ 已验证：Vue3 + Tauri 2 + Rust 空壳构建成功，产物 4.4 MB exe，可启动 |

> ⚠️ 合规提醒：当前授权文件为第三方打包（`PTC_D_SSQ` 系列特性名）。技术验证不受影响，但**面向客户正式发布时，每台机器的 Creo 与 Toolkit 授权必须由客户自身合法持有**；本产品只应作为「运行在客户已授权 Creo 之上的伴侣程序」。这条建议单独和 Owner 确认。

---

## 2. 六条路线总览

```
                        ┌────────────── 谁在做几何 ──────────────┐
   #1 OTK 异步进程   #2 Toolkit DLL   #3 J-Link   #4 Trail 回放   #5 Pro/PROGRAM   #6 非 Creo
        Creo            Creo 内部        Creo          Creo           Creo         FreeCAD
```

| # | 路线 | 驱动方式 | 能否满足 G1~G4 | 开发难度 | 风险 |
|---|---|---|---|---|---|
| R1 | **Object TOOLKIT（C++，异步 exe）** | 外部 exe 拉起无界面 Creo | 接近全满足（G1/G3/G4 路径成立，G2 需自建映射；特征创建需 C Toolkit 混合模式，P1 待验） | 中 | `Start()` 已弃用，已用自 spawn + `Connect()` 绕过，P0 通过 |
| R2 | **TOOLKIT（C，protk DLL，内嵌）** | Creo 启动时加载我们的 DLL | **理论全满足**（特性 API 最全，参数级/重建级都有；待 `protk_unlock` 验证） | 高 | 需要 protk 注册与 unlock，调试周期长 |
| R3 | **J-Link（Java pfc）** | 外部 Java 程序 async，或 Creo 内加载 | 接近（同 pfc 面，特性创建覆盖少于 C Toolkit） | 中 | 需 JRE/桥接，性能与部署更重 |
| R4 | **Trail file (.txf) 录制回放 + DBatch** | 录制 UI 操作 → 批量重放 | G1 弱、G2/G3 不成立（G4 ✅） | 低 | 字符串录制脆弱，改参数靠"点选回放"，不适合交互 |
| R5 | **Pro/PROGRAM + 关系式 + 族表** | 模型内 BASIC 程序 / 参数 | 只能满足 G4 与"参数微调"，G1/G2 不足 | 低 | 只能在已有特征树里改参数，不能凭空造特征 |
| R6 | **FreeCAD + usd-brep（非 Creo 内核）** | Python 脚本 / 自建服务 | G1~G3 可近似满足，**G4 不成立**（产出 STEP/FCStd，不是原生 prt） | 中 | 与目标 4 有本质差距 |

---

## 3. 逐条路线详述

### R1 — Object TOOLKIT（C++ pfc API，异步 exe）

**做法**：写一个独立的 `aicad_creo.exe`（`probe_otk` 骨架），**自己 spawn**
`parametric.exe -g:no_graphics -i:rpc_input`，再用 `pfcAsyncConnection::Connect()` 挂上
（**不要用 `Start()`**），然后：

1. `session->CreatePart()` / `CreateAssembly()` → 新建模型
2. 特征创建走 C Toolkit 元素树（pfc 无逐特征创建 Instructions），与 R2 共享同一套建特征代码
3. `model->Regenerate()`（只重算变更项）
4. `model->Save()` → 得到原生 `.prt/.asm`（G1/G3/G4）
5. `model->Export(step/vrml)` → 交给 USD 视图层

**优点**：
- 外部进程模型，最像"常驻后台服务"，和 Rust/Tauri 分层最干净（IPC 即可）
- 与 R2 共用同一套 `pfc` 语义，代码可迁移
- 官方示例本身就是 async 模式，设计意图就是这个用法

**缺点/现状**：
- `Start()` **已弃用**：四种命令形态均抛 `ProEngineerConnectStart` 一般故障（GUI 能启动，说明不是许可证/安装问题）；已改用「自 spawn + `Connect()`」，P0 全链路通过
- pfc 的"特征创建"面比 C Toolkit 窄，且 pfc 无逐特征创建 Instructions；复杂特征（扫描、阵列变体）必须落到 C Toolkit（R2）

**已具备**：头文件、库、可编译工程（`native/r1_otk/probe_otk.cpp`）
**下一步（P1）**：不再排障 `Start()`；按 `ROADMAP P1.4` 验证「OTK 连接 + C API 建特征」混合模式，覆盖度不足则直接切 R2。

---

### R2 — TOOLKIT（C，protk DLL 内嵌）

**做法**：编译 `aicad_protk.dll`，用 `protk.dat` 注册（`EXEC_FILE`/`TEXT_DIR`/`STARTUP`），
Creo 启动即加载；DLL 内部用 `ProFeatureCreate` / `ProSolidRegenerate` / `ProParamvalueSet` 等全量 API，
并通过命名管道/本地 socket 与 Rust 主进程通信。

**优点**：
- **唯一能完整实现 G1~G4 的路线**（PTC 自己的"参数化"能力就是这套 API 暴露的）
- 特征 id / 参数 / 依赖关系都可直接读写，G2、G3 天然成立
- 交付形态干净：用户装好的 Creo + 一个 DLL + 一个注册项

**缺点**：
- 需要 `protk_unlock` 解锁绑定（开发期需要 TOOLKIT 授权；**解锁后终端用户不需要开发授权**，这点对产品化有利）
- 调试必须跟着 Creo 进程，迭代慢
- 界面方面要接受"Creo 窗口存在"（可配合无界面/最小化启动）

**已具备**：头文件、`protk_dll_NU.lib`、`protoolkit_NU.lib`、示例 `pt_examples`、`protk.dat` 模板

---

### R3 — J-Link（Java pfc）

**做法**：Java 程序通过 pfc 类（与 R1 同一套 API）连接/启动 Creo，做同样的建模调用。

**优点**：语言门槛低、示例多、跨进程部署简单。
**缺点**：pfc 特性面依旧受限；需要 JRE 与类路径管理；相对 C/C++ 性能与部署体积无优势。
**结论**：可作为 R1 的"快速验证替代"，不宜作为长期主路线。

---

### R4 — Trail file 回放（零开发授权）

**做法**：在 Creo 里把"建草图→拉伸→倒圆角"录成 `.txf`，用 `ptcdbatch` / 命令行回放。

**优点**：不需要任何 Toolkit 授权；几分钟能出 demo。
**缺点（致命）**：
- 回放依赖 UI 坐标系与选择序列，参数是"录进去的坐标和数字"，不是参数化引用
- "把圆角改成 R1.5" 必须重新回放，等于重建（违反 G3）
- 任何界面差异/坐标系变化都会导致回放失败

**结论**：只能做"演示外壳"，不能做产品主线。

---

### R5 — Pro/PROGRAM + 关系式

**做法**：在 `.prt` 内写 Pro/PROGRAM BASIC 与关系式（`d1 = 100`），用参数驱动尺寸再 `Regenerate`。

**优点**：改动最小、完全参数化、G4 天然满足。
**缺点**：只能改**已存在特征的尺寸与关系**，不能从零构建特征树；无法满足 G1。

**定位**：作为 R1/R2 的**补充**——"参数微调"场景可优先走关系式，减少 API 调用次数。

---

### R6 — FreeCAD + usd-brep（非 Creo 内核，仅作备选）

**做法**：`FreeCADCmd`（自编译 D2 版或系统版）做 PartDesign 特征树 + recompute，
产出 STEP/`.aicad.json`，经 `occt_to_usd` 写 `BrepArray` 进 `.usda`。

**能接近的部分**：G1（参数化特征树，但是 FreeCAD 的特征）、G2/G3（改参数重算，已完成架构设计）、
视图层与拾取（usd-brep 已验证可跑）。
**无法满足的部分**：**G4**——产出不是原生 `.prt`，工程师在 Creo 里拿到的是导入实体（无特征树）。

**定位**：客户没有 Creo 时的降级方案；且 L19-22 明确要求原生 prt，因此不作为主路线。

---

## 4. 对比矩阵（按 4 条硬指标打分）

| 路线 | G1 特征树文件 | G2 拾取改特征 | G3 最小重算 | G4 原生可编辑 | 综合 |
|---|---|---|---|---|---|
| R2 Toolkit DLL | ★★★★★ | ★★★★★ | ★★★★★ | ★★★★★ | **首选** |
| R1 OTK 异步 | ★★★★☆ | ★★★★☆ | ★★★★★ | ★★★★★ | 次选（P0 已通过：spawn+Connect） |
| R3 J-Link | ★★★★☆ | ★★★☆☆ | ★★★★☆ | ★★★★★ | 验证用 |
| R4 Trail 回放 | ★★☆☆☆ | ★☆☆☆☆ | ★☆☆☆☆ | ★★★★★ | 仅 demo |
| R5 Pro/PROGRAM | ★☆☆☆☆ | ★★★☆☆ | ★★★★☆ | ★★★★★ | 补充 |
| R6 FreeCAD | ★★★☆☆（非 Creo 特征） | ★★★★☆ | ★★★★☆ | ☆☆☆☆☆ | 降级 |

---

## 5. 无论走哪条 Creo 路线，都要自建的两层

### 5.1 统一中间层：FeatureSpec IR（解决"AI 零幻觉"）

所有路线都**不要**让 LLM 直接写 API 调用或脚本，而让它输出受控 JSON：

```json
{
  "model": "base_plate",
  "type": "part",
  "features": [
    { "id": "F001", "op": "sketch", "plane": "TOP", "profile": "rect", "dims": {"w":100,"h":60} },
    { "id": "F002", "op": "extrude", "sketch": "F001", "depth": 10, "side": "symmetric" },
    { "id": "F003", "op": "hole", "on": "F002.face[top]", "dia": 6.0, "pattern": {"type":"rect","n":[2,2],"pitch":[80,40]} }
  ]
  , "edits": [ {"target": "F003.dia", "value": 7.0} ]
}
```

- 生成用 `features`，微调用 `edits`（**同一份结构**，天然区分"建"与"改"）
- `edits` 只改参数 → 后端按 id 定位已有特征 → `Regenerate`（满足 G3）
- 拓扑引用要稳定：`F002.face[top]` 这类语义引用由我们维护映射，不让 LLM 猜 face index

### 5.2 视图层：Creo → USD（解决 G2 的画布拾取）

Creo 没有 USD 导出，所以固定链路：

```
Creo(真值 .prt) ──导出──> STEP / VRML / STL ──转换──> .usda（BrepArray 或网格）
                                        └─ 写入 customData: aiCad:featureId / aiCad:paramSet
```

- 前端渲染 `.usda`，点选面 → 读 `customData.featureId` → 回传 `{"target":"F003","param":"dia"}`
- 特征 id 映射表由后端维护（Creo 的 `ProModelitem.id` / 特征序号 ↔ 我们的 `Fxxx`）
- 该环节已在本机用 `usd-brep` 验证可行（写读、网格化均通）

---

## 6. 推荐实施顺序

**主路线：R2（Toolkit DLL）为真值内核，R1 作为先导验证；R5 处理参数微调；R6 作为无 Creo 客户降级。**

| 阶段 | 内容 | 产出 | 卡点 |
|---|---|---|---|
| P0 | 打通"程序 → 无界面 Creo → 存一个原生 prt" | `probe_otk.exe` + `aicad_probe.prt` | ✅ **已通过**（`Start()` 四种形态失败 → 自 spawn `-g:no_graphics -i:rpc_input` + `Connect()` 绕过） |
| P1（3–5 周） | FeatureSpec IR + `sketch/extrude/hole` 三类特征的构建 | 自然语言生成 `base_plate.prt` | **已确认特征创建必须走 C Toolkit 元素树**（pfc 无特征创建 Instructions）；C API `ProEngineerConnect` 连不上 `rpc_input` 会话，需混合模式（OTK 连接 + C API 建特征） |
| P2（3–5 周） | 导出视图 + 特征 id 注入 + 前端点选回传 | 点面 → 改孔直径 → 刷新 | Creo 无 USD，需自建 STEP→usd 桥 |
| P3（5–7 周） | 装配 `.asm`、圆角/阵列、Undo/Redo、多轮上下文 | 端到端 demo | 装配约束 API 在 pfc 中较薄，倾向 R2 |

> 周期口径与 [`ROADMAP.md`](../ROADMAP.md) §5 一致（指示性、非承诺）；MVP = P1+P2 = 6–10 周。

**风险控制**：P0 已通过（spawn+Connect）。P1 若 C Toolkit 混合模式打不通或特征覆盖不足，直接切 R2（DLL 内嵌），不要在同一条路线上反复试错。

---

## 7. 需要立即确认/补齐的事项

1. **async 启动排障清单**（R1）— ✅ 已有结论
   - `Start()` 四种命令形态（裸名 / 全路径 / 带参 / 带 psf）**全部失败**，不再排查
   - 环境变量要求已确认：`PRO_COMM_MSG_EXE` 必设；`PATH` 必含 `Common Files\x86e_win64\obj`
   - **采用备选方案**：自 spawn `parametric.exe -g:no_graphics -i:rpc_input` + `pfcAsyncConnection::Connect()`，P0 全链路通过
   - ⚠️ 遗留：C API 的 `ProEngineerConnect` 连不上 `rpc_input` 会话（rc=-4），只有 OTK `Connect` 可以 → P1 用混合模式验证
2. **R2 准备**：确认 `protk_unlock` 的注册流程（开发期需要授权；解锁后终端用户不需要）
3. **前端/视图层**：决定 USD 里放 B-Rep（`BrepArray`）还是只放网格（用于拾取与显示即可）
4. **合规**：客户侧 Creo 与 Toolkit 授权归属，需与 Owner 确认后再对外发布

---

## 8. 一句话总结

- 想要 **4 条全中**：只有 **R2（Toolkit DLL 内嵌）** 是目标解（`protk_unlock` 待验证）；**R1（OTK 异步）** 是最省事的同族解，无界面启动已用 spawn+Connect 打通（P0 ✅），但特征创建需借 C Toolkit。
- **R4 Trail 回放**只能做演示，**R5 Pro/PROGRAM** 是参数微调补充，**R6 FreeCAD** 在"原生 prt"这一条上先天不达标。
- 所有 Creo 路线都必须自建两件东西：**FeatureSpec IR**（约束 LLM 输出）与 **Creo→USD 视图桥**（承载画布拾取）。
