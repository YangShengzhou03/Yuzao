# featurespec — FeatureSpec IR / 校验 / 计划（Rust）

> 阶段与任务口径一律以 [`ROADMAP.md`](../../ROADMAP.md) 为准；本 crate 落地 `ROADMAP P1.1`
> （FeatureSpec IR 固化为 Rust 类型 + JSON Schema），并为 `P1.2`–`P2.3` 提供调度层原语。

LLM 只输出 [`../featurespec.example.json`](../featurespec.example.json) 定义的 JSON
（ADR-012）。本 crate 是这份 JSON 的 Rust 权威：反序列化、语义校验、拓扑引用解析、
依赖图与下游重算闭包。它**不依赖 Creo**，可离线测试；真正调用内核的代码在 `native/`。

## 模块

| 模块 | 职责 |
|---|---|
| `lib.rs` | `FeatureSpec` / `sketch` / `extrude` / `hole` / `pattern` 强类型与语义校验（错误码 `FS1xx`，收集全部错误） |
| `graph` | 依赖 DAG、传递下游闭包（G2/G3 的“只重算下游”） |
| `edit_plan` | 参数编辑计划：按 id 定位 → 改参 → 输出 `Regenerate` 集合；禁止结构变更 |
| `idmap` | 语义 id ↔ Creo 内部 id 双向映射（可 JSON 持久化，供 `.usda` sidecar） |
| `plan` | 首次构建的有序步骤与依赖 |

## 使用

```bash
# 校验（结构化错误码 + 路径定位，CI 可直接用退出码）
cargo run --offline -- validate ..\featurespec.example.json

# 构建步骤 + 编辑重算集合（不触碰 Creo）
cargo run --offline -- plan ..\examples\plate_with_holes.json
cargo run --offline -- plan ..\examples\plate_with_holes.json --json

# 打印内嵌 JSON Schema（与 spec/featurespec.schema.json 单一来源）
cargo run --offline -- schema

# 测试
cargo test --offline
```

退出码：`0` 成功；`1` 校验/规划失败；`2` 用法或 IO 错误。

## 错误码（`FS1xx`）

| 码 | 含义 |
|---|---|
| `FS100` | JSON 语法/结构不合法（含未知字段、超出 f64 范围的字面量） |
| `FS101` / `FS102` | `model` 名 / `type` 不合法 |
| `FS110` / `FS111` | 特征 id 语法不合法 / 重复 id |
| `FS120` / `FS121` / `FS122` | 缺少必填字段 / 携带不属于该 op 的字段 / 未知 op |
| `FS123` | `features` 为空（改参文档也必须保留原 features） |
| `FS130`–`FS134` | 引用语法不合法 / 引用不存在 / 类型不匹配 / 前向引用 |
| `FS140` | 尺寸、间距、编辑值必须为有限正数 |
| `FS141` / `FS142` | 阵列实例数 < 2 / 不支持的阵列类型 |
| `FS150`–`FS153` | 拓扑引用语法 / 引用不存在 / 缺少面名 / 落点不是材料特征 |
| `FS160`–`FS164` | 编辑 target 语法 / 目标不存在 / 参数与 op 不匹配 / 重复目标 / 参数不可读 |

## 硬约束（与验收口径对应）

- `features` 只允许引用**更早**的特征；未知/前向引用、重复 id 均为 error（G3 特征 id 稳定）。
- `edits` 只允许改参数；`sketch`/`on` 等结构字段一律拒绝（ADR-012、G2）。
- 数值必须有限且为正；pattern 的 `n ≥ 2`、`pitch > 0`。
- hole 必须落在材料特征（v1 仅 `extrude`）并给出面名（`F002.face[top]`）。
- 任何 error 都会阻止内核调用。

## 当前边界

- `assembly` 已在契约中但 P1 内核不执行（`ROADMAP P3.1`）。
- 圆角/倒角/阵列独立特征、扫描/抽壳/拔模尚未进入 IR（`ROADMAP P3.2`），
  避免提交无法执行的半成品契约；届时按同一模式扩展 `Feature` 枚举与校验。
- Schema 为手写并 `include_str!` 内嵌；Rust 类型与该 Schema 必须同步修改（见 `CONTRIBUTING.md`）。

