# 贡献指南

## 分支与提交

- 主分支 `master`；每个阶段一个提交，提交信息用中文、前缀 `feat/fix/docs/chore`。
- 破坏性变更（如更换内核路线）前，必须先把当前工作树完整提交为快照，再删除旧代码。

## 代码边界

| 目录 | 语言 | 约束 |
|---|---|---|
| `native/` | C/C++ | 只放 Creo Toolkit / OTK 相关原生模块；不得混入业务逻辑 |
| `tools/` | Python | 只放离线工具（USD 转换、验证脚本）；不参与运行时 |
| `docs/` | Markdown | 架构决策写入 `docs/adr/` 下的新 ADR 文件，并登记到 `ARCHITECTURE.md` §7 的 ADR 索引 |

## 必须遵守的约定

0. **文档修改先查 `docs/文档同步规则.md` 的映射表**：阶段/任务/路线/目标/验证状态各有唯一权威来源，改权威来源后必须同步所有引用方；提交前运行 `python tools/docs/consistency_check.py`，必须 0 error（规则文件内另有自查流程与发布前审核清单）。

1. **LLM 不直接产出 API 调用或脚本**，只产出 `FeatureSpec` JSON（见 `docs/ARCHITECTURE.md`）。
2. **几何真值只有 Creo**；USD 只做视图与拾取，禁止在 USD 上做"编辑"。
3. 拓扑引用一律走语义 id（`F002.face[top]`），不允许把 face index 交给模型生成。
4. 新增能力先写探针（`native/r1_otk`、`native/r2_toolkit`、`tools/usd`）验证，再进入主流程。

## 提交前检查

- `native/r1_otk/build.bat` 与 `native/r2_toolkit/build_feat.bat` 能编译通过
- `tools/usd` 下的脚本在本机 Python 3.12 环境可运行
- 文档中的"已确认/未确认"状态与实际一致（不要写成"已验证"而实际未跑）
- `python tools/docs/consistency_check.py` 通过（死链、编号命名空间、阶段状态、工期口径、路线角色）
