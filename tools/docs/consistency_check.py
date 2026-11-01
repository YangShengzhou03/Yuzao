#!/usr/bin/env python3
"""跨文档一致性自动校验（离线工具，不参与运行时）。

用法：
    python tools/docs/consistency_check.py

校验项：
1. 文档内引用的 .md 文件/章节链接是否存在（无死链）；
2. 已废止的编号/状态/工期表述不得出现在现行文档（历史文档除外）；
3. 目标 G1–G4、路线角色、阶段状态、阶段工期、资源配置、里程碑在核心文档中保持一致；
4. 执行任务编号只允许使用 ROADMAP 的 P{n}.{k} 命名空间。

退出码：0 = 全部通过；1 = 存在 error。
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

CORE_DOCS = [
    "README.md",
    "ROADMAP.md",
    "docs/ARCHITECTURE.md",
    "docs/Creo路线方案对比.md",
    "docs/方案路线可行性分析.md",
    "docs/竞品对标与功能规划.md",
    "docs/adr/ADR-009.md",
    "docs/adr/ADR-010.md",
    "docs/adr/ADR-011.md",
    "docs/adr/ADR-012.md",
    "docs/文档同步规则.md",
    "spec/README.md",
    "native/r1_otk/README.md",
    "native/r2_toolkit/README.md",
    "native/r5_program/README.md",
    "native/creo_bridge/README.md",
]

ALL_DOCS = CORE_DOCS + [
    "CHANGELOG.md",
    "CONTRIBUTING.md",
    ".workbuddy/memory/2026-11-01.md",
]

HISTORICAL_DOCS = {"CHANGELOG.md", ".workbuddy/memory/2026-11-01.md"}

# 规则说明/历史映射行允许出现「被禁止的写法」作为反例
EXEMPT_MARKERS = ("禁止", "不得", "不再", "未定义", "废止", "旧", "历史", "已清理", "悬空", "原")

errors: list[str] = []


def rel(path: str) -> str:
    return str(Path(path).as_posix())


def read(path: str) -> str:
    p = ROOT / path
    return p.read_text(encoding="utf-8") if p.exists() else ""


def error(path: str, line_no: int, msg: str) -> None:
    errors.append(f"{path}:{line_no}: {msg}")


def resolve_reference(doc: str, target: str, is_link: bool) -> Path | None:
    target = target.split("#", 1)[0].strip()
    if not target or target.startswith(("http://", "https://", "mailto:")):
        return None
    if any(ch in target for ch in ("*", "?", "{")):
        return None

    doc_path = ROOT / doc
    if target.startswith(("./", "../")):
        return (doc_path.parent / target).resolve()

    if is_link:
        # Markdown 链接按文档所在目录解析（相对路径语义）
        return (doc_path.parent / target).resolve()

    # 反引号中的路径按仓库根目录解析（如 `docs/xxx.md`、`ROADMAP.md`）
    if target.startswith(("docs/", "native/", "spec/", "tools/", "app/", ".workbuddy/")):
        return (ROOT / target).resolve()
    return (ROOT / target).resolve()


def check_dead_links() -> None:
    link_re = re.compile(r"\[[^\]]*\]\(([^)\s]+\.md(?:#[^)\s]*)?)\)")
    code_re = re.compile(r"`([^`\n]+\.md(?:#[^`\n]*)?)`")
    bare_re = re.compile(
        r"(?<![\w/])((?:docs|native|spec|tools|app|\.workbuddy)/[^\s`\"'()<>]+\.md)"
    )
    root_re = re.compile(r"(?<![\w/])((?:README|ROADMAP|CHANGELOG|CONTRIBUTING)\.md)")

    for doc in ALL_DOCS:
        if doc in HISTORICAL_DOCS:
            continue
        text = read(doc)
        if not text:
            continue
        for line_no, line in enumerate(text.splitlines(), 1):
            if any(marker in line for marker in EXEMPT_MARKERS):
                continue
            candidates: list[tuple[str, bool]] = []
            candidates += [(m.group(1), True) for m in link_re.finditer(line)]
            candidates += [(m.group(1), False) for m in code_re.finditer(line)]
            candidates += [(m.group(1), False) for m in bare_re.finditer(line)]
            candidates += [(m.group(1), False) for m in root_re.finditer(line)]

            for target, is_link in candidates:
                resolved = resolve_reference(doc, target, is_link)
                if resolved is None:
                    continue
                if resolved.exists():
                    continue
                # 反引号中的裸文件名按「仓库内同名文件」解析（如 `ARCHITECTURE.md`）
                if not is_link and "/" not in target and (ROOT / target).suffix == ".md":
                    matches = [p for p in ROOT.rglob(target) if p.is_file()]
                    if matches:
                        continue
                error(doc, line_no, f"死链：{target} -> {rel(str(resolved))}")


FORBIDDEN = [
    (re.compile(r"T[0-9]+\.[0-9]+"), "已废止的 T*.x 执行任务编号（应使用 P{n}.{k}）"),
    (re.compile(r"M[7-9]\b"), "未定义的里程碑编号（ROADMAP 仅定义 M0–M6）"),
    (re.compile(r"市场\s*P[0-9]"), "市场优先级不得使用 P0–P3 命名（应使用 1/2/3）"),
    (re.compile(r"一致性映射规则"), "引用了不存在的文件名（应为 docs/文档同步规则.md）"),
    (re.compile(r"待开始"), "状态词应为「未开始」"),
    (re.compile(r"🔄"), "状态符号应为 🔧（已通过用 ✅）"),
    (re.compile(r"3[–-]5\s*天"), "阶段工期应使用 ROADMAP §5 的周口径"),
    (re.compile(r"5[–-]7\s*天"), "阶段工期应使用 ROADMAP §5 的周口径"),
    (re.compile(r"4[–-]8\s*周"), "MVP 工期应为 6–10 周（P1 3–5 + P2 3–5）"),
    (re.compile(r"8[–-]14\s*周"), "R2 工期应为 P1–P3 约 11–17 周"),
]


def check_forbidden_terms() -> None:
    for doc in ALL_DOCS:
        if doc in HISTORICAL_DOCS:
            continue
        text = read(doc)
        for line_no, line in enumerate(text.splitlines(), 1):
            if any(marker in line for marker in EXEMPT_MARKERS):
                continue
            for pattern, msg in FORBIDDEN:
                if pattern.search(line):
                    error(doc, line_no, msg)
            # Phase N 仅允许在竞品文档的历史映射表中出现
            if doc != "docs/竞品对标与功能规划.md" and re.search(r"Phase\s+[0-9]", line):
                error(doc, line_no, "现行文档不得使用 Phase N 指代阶段")


def goal_rows(doc: str) -> list[str]:
    return [
        line.strip()
        for line in read(doc).splitlines()
        if re.match(r"\|\s*G[1-4]\s*\|", line)
    ]


def check_goal_alignment() -> None:
    docs = ["README.md", "ROADMAP.md", "docs/Creo路线方案对比.md", "docs/方案路线可行性分析.md"]
    baseline = goal_rows("ROADMAP.md")
    if len(baseline) != 4:
        errors.append("ROADMAP.md: 未找到完整的 G1–G4 目标表")
    for doc in docs:
        text = read(doc)
        for token in ("G1", "G2", "G3", "G4", "只重算下游", "Regenerate", "原生"):
            if token not in text:
                errors.append(f"{doc}: 目标口径缺少关键字「{token}」")
        rows = goal_rows(doc)
        if rows != baseline:
            errors.append(f"{doc}: G1–G4 表格与 ROADMAP §0 不一致")


ROUTE_DOCS = [
    "README.md",
    "ROADMAP.md",
    "docs/adr/ADR-010.md",
    "docs/Creo路线方案对比.md",
    "docs/方案路线可行性分析.md",
    "docs/文档同步规则.md",
]


def check_route_roles() -> None:
    for doc in ROUTE_DOCS:
        text = read(doc)
        for route in ("R1", "R2", "R3", "R4", "R5", "R6"):
            if route not in text:
                errors.append(f"{doc}: 路线定位缺少 {route}")
        if "主线" not in text or "先导" not in text or "降级" not in text:
            errors.append(f"{doc}: 路线定位缺少「主线/先导/降级」角色词")


def stage_rows(doc: str) -> dict[str, str]:
    rows: dict[str, str] = {}
    for line in read(doc).splitlines():
        m = re.match(r"\|\s*(P[0-6])\s*\|", line)
        if not m:
            continue
        # §5 资源表也以 Pn 开头；只保留含阶段状态词的行
        if any(token in line for token in ("✅", "🔧", "未开始", "未排期", "待定")):
            rows[m.group(1)] = line
    return rows


def check_stage_status() -> None:
    expected = {
        "P0": "✅ 已通过",
        "P1": "🔧 进行中",
        "P2": "未开始",
        "P3": "未开始",
        "P4": "未排期",
        "P5": "未开始",
        "P6": "未开始",
    }
    conflicting = {
        "P0": ["🔧", "未开始", "未排期"],
        "P1": ["✅ 已通过", "未开始", "未排期"],
        "P2": ["✅ 已通过", "🔧", "未排期"],
        "P3": ["✅ 已通过", "🔧", "未排期"],
        "P4": ["✅ 已通过", "🔧", "未开始"],
        "P5": ["✅ 已通过", "🔧", "未排期"],
        "P6": ["✅ 已通过", "🔧", "未排期"],
    }
    for doc in ("README.md", "ROADMAP.md"):
        rows = stage_rows(doc)
        for stage, token in expected.items():
            if stage not in rows:
                errors.append(f"{doc}: 缺少阶段 {stage} 的状态行")
                continue
            if token not in rows[stage]:
                errors.append(f"{doc}: {stage} 状态应为「{token}」")
            for bad in conflicting[stage]:
                if bad in rows[stage]:
                    errors.append(f"{doc}: {stage} 出现冲突状态词「{bad}」")


def check_estimates() -> None:
    for doc in ("ROADMAP.md", "docs/方案路线可行性分析.md", "docs/Creo路线方案对比.md"):
        text = read(doc)
        for token in ("3–5 周", "5–7 周", "6–10 周"):
            if token not in text:
                errors.append(f"{doc}: 阶段工期口径缺少「{token}」")


def check_resources() -> None:
    tokens = ("1–1.5 FTE", "0.5 FTE（稀缺专家）", "2–4 周")
    for doc in ("ROADMAP.md", "docs/方案路线可行性分析.md"):
        text = read(doc)
        for token in tokens:
            if token not in text:
                errors.append(f"{doc}: 资源配置口径缺少「{token}」（以 ROADMAP §5 为准）")


def check_milestones() -> None:
    text = read("ROADMAP.md")
    for index in range(7):
        if f"**M{index}**" not in text:
            errors.append(f"ROADMAP.md: 未找到里程碑 M{index}（关键节点必须在 §6 定义 M0–M6）")


def check_execution_ids() -> None:
    for doc in ALL_DOCS:
        text = read(doc)
        for line_no, line in enumerate(text.splitlines(), 1):
            if re.search(r"T[0-9]+\.[0-9]+", line) and doc not in HISTORICAL_DOCS:
                error(doc, line_no, "执行任务编号必须使用 P{n}.{k}")


def main() -> int:
    check_dead_links()
    check_forbidden_terms()
    check_goal_alignment()
    check_route_roles()
    check_stage_status()
    check_estimates()
    check_resources()
    check_milestones()
    check_execution_ids()

    if errors:
        print(f"[FAIL] 一致性校验发现 {len(errors)} 个问题：")
        for item in errors:
            print(" -", item)
        return 1

    print(f"[OK] 一致性校验通过：{len(ALL_DOCS)} 份文档，0 error。")
    return 0


if __name__ == "__main__":
    sys.exit(main())
