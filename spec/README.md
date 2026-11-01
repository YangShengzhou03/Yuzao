# FeatureSpec IR — 唯一写入 Creo 内核的输入

规则（ADR-012）：

1. **LLM 只输出本目录定义的 JSON**，禁止直接生成 Toolkit API 调用、脚本或 trail file。
2. `features` 用于首次构建；`edits` 用于增量修改，二者结构相同。
3. `edits` 只改参数：后端按 id 定位已有特征 → 修改 → `Regenerate`，不重建模型。
4. 拓扑引用语法 `F002.face[top]` 由 Rust 特征 ID 映射表解析，LLM 永不猜 face index。

## 本目录内容（P1.1 已落地，✅）

| 文件 / 目录 | 说明 |
|---|---|
| `featurespec.schema.json` | JSON Schema（draft 2020-12 严格模式）：语法层约束的单一来源，编译期内嵌于 Rust crate |
| `featurespec.example.json` | 最小示例（sketch/extrude/hole + edits） |
| `examples/` | 可执行示例 spec（如 `plate_with_holes.json`，含 pattern/双孔/edits），可直接作为 `feat_build` 输入 |
| `rust/` | Rust crate `featurespec`：类型定义 + 跨字段语义校验（错误码 `FS1xx`，收集全部错误）+ 依赖图/改参计划/id 映射 + `validate`/`plan`/`schema` CLI；`cargo test` 42/42 通过；详见 `spec/rust/README.md` |

校验与规划入口（离线、不触碰 Creo；退出码 0/1/2 可接 CI）：

```sh
cd spec/rust
cargo run --offline -- validate ../featurespec.example.json
cargo run --offline -- plan ../examples/plate_with_holes.json --json
```

Schema 与 Rust 类型是同一契约的两面，必须同步修改：Schema 管语法层
（必填、枚举、正则、`additionalProperties:false`）；Rust 管跨字段语义
（引用存在性与顺序、hole 落材料面、编辑目标唯一、有限正值）。

执行器状态（`native/r2_toolkit/feat_build.c`，🔧 代码落地待实机验证）：
解析/校验路径已实测，特征创建运行仍受 C API 连接 `rpc_input` 会话 -4 阻塞（见 `native/r2_toolkit/README.md`）。

## op 覆盖计划（阶段/任务以 `ROADMAP.md` §2–§3 为准）

| op | 阶段 / 任务 | Creo 落点 |
|---|---|---|
| sketch / extrude / hole | P1 / `P1.1`–`P1.3` | ProFeatureWithoptionsCreate 元素树（`P1.1` ✅ Rust 类型+Schema；`P1.2`/`P1.3` 🔧 构建器编译通过，运行受 C API 连接 -4 阻塞，混合模式待验。`native/r1_otk` 仅验证连通性与 `CreatePart`，未做特征创建） |
| round / chamfer / pattern | P3 / `P3.2` | 同上，元素树扩展（v1 构建器的 rect pattern 以多实例直孔执行） |
| scan / shell / draft | P3 / `P3.2` | 同上 |
| 装配约束 / .asm | P3 / `P3.1` | ProSolidMdlnameCreate(PRO_ASSEMBLY) + ProAsmcomp |

