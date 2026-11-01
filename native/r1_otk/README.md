# native/r1_otk — R1 Object TOOLKIT 启动通路探针（P0）

## 结论：P0 已通过 ✅

**已验证链路**：外部进程 → 无界面 Creo（`-g:no_graphics -i:rpc_input`）→ OTK `Connect()` →
`CreatePart()` → `Save()`（得到原生 `.prt`）→ 导出 STEP → `Disconnect()`（Creo 保持存活）。

产物（运行后落在 `native/work/`，已被 `.gitignore` 忽略）：

| 文件 | 大小 | 说明 |
|---|---|---|
| `aicad_probe.prt.1` | 42,855 B | Creo 原生零件（文件头 `#UGC:2 PART 2552`） |
| `aicad_probe.step` | 3,463 B | `ISO-10303-21`，`FILE_NAME` 标注 `CREO PARAMETRIC BY PTC INC, 2025102` |

## 关键结论：用「自己 spawn + Connect」，不要用 `Start()`

`pfcAsyncConnection::Start()` 在四种命令形态下均抛
`pfcExceptions::XToolkitGeneralError`（`ProEngineerConnectStart`），即使
`PRO_COMM_MSG_EXE` 已设置、`pro_comm_msg.exe` 能起来，`parametric` 也没有真正拉起。

改用文档给的非交互形态后一次成功：

```bat
rem 1) 自己启动 Creo（无窗口）
parametric.exe -g:no_graphics -i:rpc_input

rem 2) OTK 侧连接
pfcAsyncConnection::Connect(NULL, NULL, <text_dir>, 120)
```

必备环境变量（异步模式要求，Creo TOOLKIT User's Guide "Setting up an Asynchronous Creo Toolkit Application"）：

| 变量 | 值 |
|---|---|
| `PRO_COMM_MSG_EXE` | `<loadpoint>\Common Files\x86e_win64\obj\pro_comm_msg.exe` |
| `PATH` | 需包含 `Common Files\x86e_win64\obj`（否则缺 `ucore68.dll`，进程启动即 `STATUS_DLL_NOT_FOUND`） |

## 已确认环境

| 项 | 状态 |
|---|---|
| TOOLKIT / OTK 开发包 | ✅ 430 个头文件、`protoolkit_NU.lib`、`pt_asynchronous.lib`、`otk_cpp_async.lib`、`otk_222.lib` |
| 授权特性 | ✅ 含 `TOOLKIT` / `ObjectToolkit` / `ObjectToolkitJava` |
| Creo GUI 启动 | ✅ `parametric.exe` + `xtop.exe` + `nmsd.exe` + `pro_comm_msg.exe` |
| 无界面启动 | ✅ `-g:no_graphics -i:rpc_input` 全栈起来（无窗口） |
| 探针编译 | ✅ MSVC 2022 x64 |

## 构建

```bat
native\r1_otk\build.bat
```

依赖 VS 2022 x64（`vcvars64.bat`）；`set CREO_ROOT=...` 可换安装路径。

## 运行

```bat
native\start_creo.bat                :: 后台启动无界面 Creo（默认工作目录 native\work，可传参覆盖）
native\r1_otk\run.bat connect        :: 连接 → 建零件 → 保存 → 导出 STEP（在 run.bat 所在目录落盘）
native\stop_creo.bat                 :: 清理残留 Creo 进程
```

其他命令形态（仅用于排障对照）：

```bat
run.bat ni      :: Start("parametric -g:no_graphics -i:rpc_input", text)   ← 仍失败
run.bat bare    :: Start("parametric", text)                              ← 仍失败
run.bat exe     :: Start("<...>\parametric.exe", text)                    ← 仍失败
run.bat psf     :: Start("<...>\parametric.exe" "<...>\parametric.psf", text) ← 仍失败
```

## 下一步（P1）

pfc 无特征创建 Instructions，特征树必须走 C Toolkit 元素树（`ProFeatureWithoptionsCreate`）。
P1 探针已迁至 [`../r2_toolkit/probe_feat.c`](../r2_toolkit/probe_feat.c)；
遗留问题：C API `ProEngineerConnect` 连不上 `rpc_input` 会话（rc=-4），需「OTK 连接 + C API 建特征」混合模式。
