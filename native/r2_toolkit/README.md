# native/r2_toolkit — R2 主线：C Toolkit 元素树特征探针（P1）

> 路线定位：R2（Toolkit DLL / C API）是**生产级主线**，能力最全、唯一能完整满足 G1–G4。
> 当前 `probe_feat.c` 是 P1 验证探针（`ROADMAP P1.2`–`P1.4`），验证「C Toolkit 元素树建特征」通路；毕业后正式产物 `aicad_protk.dll` 与 R1 的 `aicad_creo.exe` 均落在 [`../creo_bridge`](../creo_bridge/)。

## 背景：为什么是 C Toolkit 而非 OTK pfc

P0 用 OTK pfc C++ 证明了连通性，但 pfc 没有逐特征创建 Instructions
（`ProExtrude` / `pfcExtrudeInstructions` 不存在），特征树必须用经典 C Toolkit
**元素树**（`ProFeatureWithoptionsCreate`）构建。因此特征创建路线落在 R2 的 C API。

## 验证状态（编译通过；运行未通过）

`probe_feat.c` 已编译通过，目标是用 `ProEngineerConnect` + `ProFeatureWithoptionsCreate`
建「草图矩形 + 拉伸」第一特征，`ProSolidRegenerate` 后遍历特征树并打印每个特征 id/type。

`feat_build.c`（`ROADMAP P1.2`/`P1.3`，🔧）是 FeatureSpec 驱动的构建器：以
`spec/featurespec.schema.json` 约束的 JSON 为唯一输入，建 sketch+extrude 首特征与
hole（含 rect pattern 多实例），`Regenerate` 后回读特征树校验数量，输出 `[aicad:map]`
的 spec id → Creo feature id 映射。元素树逐项对照本机官方示例
`ptu_featcreat/UgSktFirstFeatureCreate.c` 与 `UgHoleCreate.c`；已编译通过（/W4），
免 Creo 的解析/校验路径冒烟实测通过（`FS200/FS201/FS140/FS133`、合法 spec → `FS209`）。

> ❌ `ProEngineerConnect("","","",text,…)` 对 `-i:rpc_input` 会话返回 -4（PRO_TK_NOT_FOUND）：
> C API 连不上 rpc_input 模式，特征创建**尚未跑通**。当前方案：OTK `Connect` 建会话 + C API 干活
> （混合模式，`ROADMAP P1`），或改用 GUI/普通会话（`ProEngineerSessionIdGet` / `ProEngineerConnectIdExtract` 路线）。

## 构建与运行

```bat
:: 1) 拉起共享无界面 Creo（或 GUI/普通会话，视 C API 连通性定）
native\start_creo.bat

:: 2) 编译（probe_feat.exe + feat_build.exe）
native\r2_toolkit\build_feat.bat

:: 3) 运行探针（默认 part 名 aicad_feat）
native\r2_toolkit\run_feat.bat
native\r2_toolkit\run_feat.bat my_part

:: 4) 运行 FeatureSpec 构建器（免 Creo 时仅执行解析+校验并报错退出）
native\r2_toolkit\run_build.bat ..\..\spec\examples\plate_with_holes.json
```

## 文件

| 文件 | 说明 |
|---|---|
| `probe_feat.c` | R2 探针：C Toolkit 元素树建草图矩形 + 拉伸第一特征 |
| `feat_build.c` | FeatureSpec 驱动构建器（`P1.2`/`P1.3`）：严格 JSON 解析 + 校验 + sketch/extrude/hole/pattern + 特征树回读校验，错误码 `FS2xx` |
| `build_feat.bat` | MSVC 2022 x64 编译（pt_asynchronous.lib + protoolkit_NU.lib 等） |
| `run_feat.bat` | 探针运行入口，连接已启动的 Creo 并建特征 |
| `run_build.bat` | 构建器运行入口：`run_build.bat <spec.json>` |

## 下一步（P1 → P3）

1. P1：打通混合模式并完成 sketch/extrude/hole（`P1.1`–`P1.3`）；用 `P1.4` 探针验证圆角/阵列/扫描/装配覆盖度。
2. P3：圆角/倒角/阵列（`P3.2`）、扫描/抽壳/拔模（`P3.2`）、装配约束/`.asm`（`P3.1`，`ProSolidMdlnameCreate(PRO_ASSEMBLY)` + `ProAsmcomp`）。
3. 每类特征映射到 `spec/` 的 FeatureSpec `op`，由 Rust 调度层下发。
