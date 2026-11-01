# native/creo_bridge — 正式内核落点（R1 `aicad_creo.exe` / R2 `aicad_protk.dll`）

P0/P1 探针在 [`../r1_otk/`](../r1_otk/README.md)（R1 OTK，先导）与 [`../r2_toolkit/`](../r2_toolkit/README.md)（R2 C Toolkit，主线）。
路线角色见 ADR-010、阶段状态见 `ROADMAP.md`。探针验证通过后，正式产物落在本目录：

- **R1 先导**：`aicad_creo.exe`（常驻外部进程）
- **R2 主线**：`aicad_protk.dll`（由 Creo 启动加载，配 `protk.dat`）

共同职责：

1. 建立无界面 Creo 会话：R1 自 spawn `parametric.exe -g:no_graphics -i:rpc_input` + `pfcAsyncConnection::Connect()`；R2 由 Creo 加载 DLL
2. 从 stdin（或命名管道）读 FeatureSpec JSON
3. 按序创建特征（sketch → extrude → hole → ...），`Regenerate`，保存 `.prt`
4. `edits`：按特征 id 定位 → 改参 → `Regenerate`（不重建）
5. 导出 STEP/VRML 供视图桥转 `.usda`
6. 结构化错误输出（`pfcXToolkitError` → 错误码 JSON）

Rust 调度层只与本内核对话，不直接触碰 Toolkit。
