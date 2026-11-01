import { computed, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { ElMessage, ElMessageBox } from "element-plus";
import { Viewport3D, viewLabel, type ViewName } from "../viewport/scene3d";
import { DEFAULT_SPEC, buildModelFromSpec, type FeatureNode } from "../model/model";
import { useWindowFrame } from "../frame/useWindowFrame";

export interface MenuItem {
  label: string;
  shortcut?: string;
  active?: boolean;
  action?: () => void;
}

export interface MenuGroup {
  name: string;
  items: MenuItem[];
}

export interface Message {
  role: "user" | "assistant" | "system";
  text: string;
  time: string;
}

export interface TreeNodeData {
  id: string;
  label: string;
  children?: TreeNodeData[];
}

export interface EditableParam {
  target: string;
  label: string;
  value: number;
}

const BLANK_SPEC = `{\n  "model": "untitled",\n  "type": "part",\n  "features": []\n}`;
const VIEW_BUTTONS: ViewName[] = ["iso", "front", "top", "right"];
const UNIT_OPTIONS: Array<{ value: "mm" | "inch"; label: string }> = [
  { value: "mm", label: "毫米 mm" },
  { value: "inch", label: "英寸 inch" },
];
const WELCOME =
  "语造已就绪。输入自然语言描述零件，或粘贴 FeatureSpec JSON 后回车校验。输入 help 查看命令。";

/**
 * 全部共享状态与业务逻辑的单例。各 UI 组件调用 `useCad()` 拿到同一份对象，
 * 避免 prop/emit 在七层组件间来回透传。生命周期（视口、全局快捷键）由
 * `mountViewport` / `boot` 挂载，`unmountViewport` / `shutdown` 卸载。
 */
function createCad() {
  const { fullscreen, minimize, toggleMaximize, toggleFullscreen, close } = useWindowFrame();

  let viewport: Viewport3D | null = null;

  // ---- 视口与显示 ----
  const canvasEl = ref<HTMLCanvasElement | null>(null);
  const canvasWrap = ref<HTMLDivElement | null>(null);
  const view = ref<ViewName>("iso");
  const wireframe = ref(false);
  const showGrid = ref(true);
  const showAxes = ref(true);
  const unit = ref<"mm" | "inch">("mm");
  const viewportError = ref<string | null>(null);
  const cursor = ref<{ x: number; y: number } | null>(null);
  /** 选中特征在画布上的像素锚点，用于悬浮 AI 工具条定位。 */
  const featureAnchor = ref<{ x: number; y: number } | null>(null);

  // ---- FeatureSpec 与特征树 ----
  const specText = ref(DEFAULT_SPEC);
  const features = ref<FeatureNode[]>([]);
  const selected = ref<FeatureNode | null>(null);
  const modelName = ref("plate_with_holes");
  const featureCount = ref(0);
  const specHistory = ref<string[]>([DEFAULT_SPEC]);
  const specIndex = ref(0);
  /** 宿主组件注册的「打开文件选择器」实现（组件持有隐藏的 input 元素）。 */
  let openFilePicker: (() => void) | null = null;

  // ---- 侧栏与会话 ----
  const sideTab = ref<"tree" | "props" | "log">("tree");
  const sideOpen = ref(true);
  const messages = ref<Message[]>([]);
  const input = ref("");
  const history: string[] = [];
  let historyIndex = -1;

  // ---- 悬浮 AI 工具条 ----
  const aiPrompt = ref("");

  // ---- 校验 / 首选项 ----
  const running = ref(false);
  const prefOpen = ref(false);

  // ---- 全屏顶栏收起/展开 ----
  const menubarOpen = ref(false);
  const hoveringTopbar = ref(false);
  const openMenus = ref(0);
  let collapseTimer: number | undefined;

  function now(): string {
    return new Date().toLocaleTimeString("zh-CN", { hour12: false });
  }

  /** 追加一条会话记录。 */
  function say(text: string, role: Message["role"] = "assistant"): void {
    messages.value.push({ role, text, time: now() });
  }

  function flash(text: string): void {
    ElMessage.info(text);
    say(text, "system");
  }

  function loadModel(json: string): void {
    if (!viewport) return;
    const built = buildModelFromSpec(json);
    viewport.setModel(built.group);
    features.value = built.features;
    featureCount.value = built.features.length;
    modelName.value = built.name;
    // 重建会换掉整批 FeatureNode 对象；按 id 重新绑定，否则属性面板仍显示旧参数。
    if (selected.value) {
      const keepId = selected.value.id;
      selected.value = built.features.find((f) => f.id === keepId) ?? null;
      viewport.setHighlight(selected.value?.id ?? null);
    }
  }

  // ---- FeatureSpec 文件操作与编辑历史 ----
  function pushSpec(next: string): void {
    if (next === specHistory.value[specIndex.value]) return;
    specHistory.value = specHistory.value.slice(0, specIndex.value + 1);
    specHistory.value.push(next);
    specIndex.value = specHistory.value.length - 1;
  }

  function applySpec(next: string): void {
    specText.value = next;
    loadModel(next);
  }

  function undo(): void {
    if (specIndex.value <= 0) return flash("没有可撤销的操作");
    specIndex.value -= 1;
    applySpec(specHistory.value[specIndex.value]);
    flash("已撤销");
  }

  function redo(): void {
    if (specIndex.value >= specHistory.value.length - 1) return flash("没有可重做的操作");
    specIndex.value += 1;
    applySpec(specHistory.value[specIndex.value]);
    flash("已重做");
  }

  function newModel(): void {
    pushSpec(BLANK_SPEC);
    applySpec(BLANK_SPEC);
    selectFeature(null);
    say("已新建空白零件。请在「属性」页的 FeatureSpec 输入中定义特征。", "system");
    flash("已新建空白零件");
  }

  /** 宿主组件（TopBar）挂载时注册，卸载时传 null 注销。 */
  function onFilePickerReady(fn: (() => void) | null): void {
    openFilePicker = fn;
  }

  function openModel(): void {
    openFilePicker?.();
  }

  function onFilePicked(e: Event): void {
    const target = e.target as HTMLInputElement;
    const file = target.files?.[0];
    target.value = "";
    if (!file) return;
    const reader = new FileReader();
    reader.onload = () => {
      const text = String(reader.result ?? "");
      let parsed: { features?: unknown };
      try {
        parsed = JSON.parse(text) as { features?: unknown };
      } catch {
        ElMessage.error("文件不是合法的 JSON");
        return;
      }
      if (!Array.isArray(parsed.features)) {
        ElMessage.error("不是有效的 FeatureSpec：缺少 features 数组");
        return;
      }
      pushSpec(text);
      applySpec(text);
      selectFeature(null);
      say(`已打开 ${file.name}`);
      flash(`已打开 ${file.name}`);
    };
    reader.readAsText(file);
  }

  function saveModel(): void {
    const url = URL.createObjectURL(new Blob([specText.value], { type: "application/json" }));
    const a = document.createElement("a");
    a.href = url;
    a.download = `${modelName.value || "model"}.json`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 0);
    flash("已导出 FeatureSpec JSON");
    say("已导出 FeatureSpec JSON；写入 Creo 原生 .prt 需连接内核（ROADMAP P2）。");
  }

  function openPrefs(): void {
    prefOpen.value = true;
  }

  function resetPrefs(): void {
    unit.value = "mm";
    wireframe.value = false;
    showGrid.value = true;
    showAxes.value = true;
    flash("首选项已恢复默认");
  }

  // ---- 特征选中与高亮 ----
  function selectFeature(id: string | null): void {
    const node = id ? (features.value.find((f) => f.id === id) ?? null) : null;
    selected.value = node;
    viewport?.setHighlight(node?.id ?? null);
    if (node) sideTab.value = "props";
  }

  function opLabel(op: string): string {
    if (op === "sketch") return "草图";
    if (op === "extrude") return "拉伸";
    if (op === "hole") return "孔";
    return op;
  }

  // ---- 视图 ----
  function applyView(v: ViewName): void {
    view.value = v;
    flash(`视图：${viewLabel(v)}`);
  }

  function fitView(): void {
    viewport?.fit();
    flash("已适应窗口");
  }

  // ---- 校验与重建 ----
  async function validateSpec(): Promise<void> {
    running.value = true;
    try {
      const result = await invoke<{
        model: string;
        type: string;
        features: number;
        edits: number;
      }>("validate_spec", { json: specText.value });
      say(
        `校验通过：model=${result.model}  type=${result.type}  特征 ${result.features} 项，编辑 ${result.edits} 项`
      );
      ElMessage.success("FeatureSpec 校验通过");
      sideTab.value = "log";
      loadModel(specText.value);
    } catch (e) {
      const list = Array.isArray(e) ? (e as string[]) : [String(e)];
      say(`校验失败，共 ${list.length} 项：\n${list.join("\n")}`);
      ElMessage.error(`校验失败，共 ${list.length} 项`);
      sideTab.value = "log";
    } finally {
      running.value = false;
    }
  }

  function rebuild(): void {
    loadModel(specText.value);
    flash("预览模型已按当前 FeatureSpec 重建。");
  }

  // ---- 参数编辑 ----
  /** 改参即写入 FeatureSpec 的 edits（G3 语义：后端按 target 定位后 Regenerate，不重建特征树）。 */
  function onParamEdit(target: string, value: number | undefined): void {
    if (value == null || !(value > 0)) return;
    try {
      const spec = JSON.parse(specText.value) as { edits?: Array<{ target: string; value: number }> };
      spec.edits = (spec.edits ?? []).filter((e) => e.target !== target);
      spec.edits.push({ target, value });
      const next = JSON.stringify(spec, null, 2);
      specText.value = next;
      pushSpec(next);
      loadModel(next);
      say(`已修改 ${target} = ${value}，预览已按 edits 重建（内核侧对应 Regenerate）。`);
    } catch {
      // FeatureSpec 当前不是合法 JSON（编辑中），等用户点「校验」时统一报错
    }
  }

  /**
   * 悬浮条里的「AI 修改」：LLM 未接入前，本地按关键字 + 数字落到对应参数。
   * ponytail: 关键词匹配是最小可用实现，P5 接入 LLM 后换成 FeatureSpec 语义解析。
   */
  function applyFeaturePrompt(): void {
    const text = aiPrompt.value.trim();
    if (!text || !selected.value) return;
    const params = editableParams.value;
    if (!params.length) {
      flash(`${selected.value.id} 没有可编辑参数`);
      return;
    }
    const num = Number(text.match(/-?\d+(?:\.\d+)?/)?.[0]);
    if (!Number.isFinite(num) || num <= 0) {
      flash("LLM 尚未接入（ROADMAP P5）；当前可识别数字改参，例如「孔径改为 12」");
      return;
    }
    const lower = text.toLowerCase();
    const rules: Array<[string, string[]]> = [
      [".dia", ["孔", "直径", "dia", "大小"]],
      [".depth", ["深", "厚", "depth"]],
      [".dims.w", ["宽", "width"]],
      [".dims.h", ["高", "height"]],
    ];
    const hit = rules.find(([, words]) => words.some((w) => lower.includes(w)));
    const param = (hit && params.find((p) => p.target.endsWith(hit[0]))) || params[0];
    aiPrompt.value = "";
    onParamEdit(param.target, num);
  }

  // ---- 命令行 ----
  function submitCommand(raw: string): void {
    const text = raw.trim();
    if (!text) return;
    history.push(text);
    historyIndex = history.length;
    messages.value.push({ role: "user", text, time: now() });
    runCommand(text);
    input.value = "";
  }

  function runCommand(text: string): void {
    const lower = text.toLowerCase();
    const viewMap: Array<[string, ViewName]> = [
      ["等轴测", "iso"],
      ["前视", "front"],
      ["后视", "back"],
      ["左视", "left"],
      ["右视", "right"],
      ["顶视", "top"],
      ["底视", "bottom"],
      ["iso", "iso"],
    ];
    for (const [key, v] of viewMap) {
      if (lower === key) {
        applyView(v);
        return;
      }
    }
    if (lower === "help" || lower === "帮助") {
      say(
        [
          "视图：等轴测 / 前视 / 后视 / 左视 / 右视 / 顶视 / 底视 / 适应",
          "显示：线框 / 着色 / 网格 / 坐标轴",
          "文件：新建 Ctrl+N / 打开 Ctrl+O / 保存 Ctrl+S",
          "编辑：撤销 Ctrl+Z / 重做 Ctrl+Y",
          "特征：校验（粘贴 JSON 后回车） / 重建 / 单位 mm|inch",
          "快捷键：F 适应 · 1/2/3/4 等轴测/前视/顶视/右视 · W 线框 · G 网格 · X 坐标轴 · Esc 取消选中",
          "其他：清空 / 帮助",
        ].join("\n")
      );
      return;
    }
    if (lower === "清空" || lower === "clear") {
      messages.value = [];
      return;
    }
    if (lower === "线框" || lower === "wireframe") {
      wireframe.value = true;
      flash("已切换为线框显示");
      return;
    }
    if (lower === "着色" || lower === "shaded") {
      wireframe.value = false;
      flash("已切换为着色显示");
      return;
    }
    if (lower === "网格" || lower === "grid") {
      showGrid.value = !showGrid.value;
      flash(showGrid.value ? "已显示网格" : "已隐藏网格");
      return;
    }
    if (lower === "坐标轴" || lower === "axes") {
      showAxes.value = !showAxes.value;
      flash(showAxes.value ? "已显示坐标轴" : "已隐藏坐标轴");
      return;
    }
    if (lower === "适应" || lower === "fit") {
      fitView();
      return;
    }
    if (lower.startsWith("单位")) {
      const next = lower.endsWith("inch") || lower.endsWith("英寸") ? "inch" : "mm";
      unit.value = next;
      flash(`单位：${next}`);
      return;
    }
    if (lower === "校验" || lower === "validate") {
      void validateSpec();
      return;
    }
    if (lower === "重建" || lower === "rebuild") {
      rebuild();
      return;
    }
    if (text.startsWith("{")) {
      specText.value = text;
      pushSpec(text);
      void validateSpec();
      return;
    }
    say(`已记录指令「${text}」。自然语言转 FeatureSpec 的 LLM 调度尚未接入（ROADMAP P5）；当前支持上述命令与 JSON 校验。`);
  }

  function onInputKey(e: KeyboardEvent): void {
    if (e.key === "Enter") {
      submitCommand(input.value);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (historyIndex > 0) input.value = history[--historyIndex] ?? "";
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (historyIndex < history.length - 1) input.value = history[++historyIndex] ?? "";
      else {
        historyIndex = history.length;
        input.value = "";
      }
    }
  }

  /**
   * 全局快捷键（CAD 习惯）。输入框聚焦时不拦截，仅 Esc 用于失焦。
   * 带修饰键（Ctrl/Alt/Meta）时忽略，避免与浏览器/系统快捷键冲突。
   */
  function onGlobalKey(e: KeyboardEvent): void {
    const el = document.activeElement;
    const typing =
      el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement;
    if (e.key === "Escape") {
      if (typing) (el as HTMLElement).blur();
      else selectFeature(null);
      return;
    }
    if ((e.ctrlKey || e.metaKey) && !e.altKey) {
      if (typing) return; // 输入框内交给原生撤销/重做
      const ck = e.key.toLowerCase();
      if (ck === "z") {
        e.preventDefault();
        undo();
      } else if (ck === "y") {
        e.preventDefault();
        redo();
      } else if (ck === "n") {
        e.preventDefault();
        newModel();
      } else if (ck === "o") {
        e.preventDefault();
        openModel();
      } else if (ck === "s") {
        e.preventDefault();
        saveModel();
      }
      return;
    }
    if (typing || e.altKey) return;
    const k = e.key.toLowerCase();
    if (k === "f") fitView();
    else if (k === "1") applyView("iso");
    else if (k === "2") applyView("front");
    else if (k === "3") applyView("top");
    else if (k === "4") applyView("right");
    else if (k === "w") {
      wireframe.value = !wireframe.value;
      flash(wireframe.value ? "线框显示（W）" : "着色显示（W）");
    } else if (k === "g") {
      showGrid.value = !showGrid.value;
      flash(showGrid.value ? "已显示网格（G）" : "已隐藏网格（G）");
    } else if (k === "x") {
      showAxes.value = !showAxes.value;
      flash(showAxes.value ? "已显示坐标轴（X）" : "已隐藏坐标轴（X）");
    }
  }

  function onClose(): void {
    void ElMessageBox.confirm("确定要退出语造吗？", "退出", {
      confirmButtonText: "退出",
      cancelButtonText: "取消",
      type: "warning",
    })
      .then(close)
      .catch(() => undefined);
  }

  function openTree(): void {
    sideOpen.value = true;
    sideTab.value = "tree";
  }

  function openLog(): void {
    sideOpen.value = true;
    sideTab.value = "log";
  }

  function onNodeClick(data: { id: string }): void {
    if (data.id === "__model__") return;
    viewport?.clearAnchor(); // 树选不落在画布上，避免悬浮条停在旧拾取点
    selectFeature(data.id);
  }

  // ---- 全屏顶栏 ----
  function revealMenubar(): void {
    if (!fullscreen.value) return;
    window.clearTimeout(collapseTimer);
    menubarOpen.value = true;
  }

  function scheduleCollapse(): void {
    if (!fullscreen.value) return;
    window.clearTimeout(collapseTimer);
    collapseTimer = window.setTimeout(() => {
      if (!openMenus.value && !hoveringTopbar.value) menubarOpen.value = false;
    }, 220);
  }

  function onTopbarEnter(): void {
    hoveringTopbar.value = true;
    revealMenubar();
  }

  function onTopbarLeave(): void {
    hoveringTopbar.value = false;
    scheduleCollapse();
  }

  function onMenuOpen(): void {
    openMenus.value += 1;
  }

  function onMenuClose(): void {
    openMenus.value = Math.max(0, openMenus.value - 1);
    scheduleCollapse();
  }

  function onMenuSelect(index: string): void {
    menuHandlers.value[index]?.action?.();
    if (fullscreen.value) menubarOpen.value = false;
  }

  // ---- 生命周期 ----
  /** 创建 3D 视口并注入首帧模型；由视口组件挂载时调用。 */
  function mountViewport(canvas: HTMLCanvasElement, wrap: HTMLElement): void {
    try {
      viewport = new Viewport3D(canvas, wrap);
    } catch (e) {
      console.error("[app] 3D 视口初始化失败：", e);
      viewportError.value = e instanceof Error ? e.message : String(e);
      return;
    }
    viewport.onError = (msg) => (viewportError.value = msg);
    viewport.onCursor = (p) => (cursor.value = p ? { x: p.x, y: p.z } : null);
    viewport.onPick = (id) => selectFeature(id);
    viewport.onAnchor = (p) => (featureAnchor.value = p);
    loadModel(specText.value);
  }

  function unmountViewport(): void {
    viewport?.dispose();
    viewport = null;
  }

  function boot(): void {
    window.addEventListener("keydown", onGlobalKey);
    say(WELCOME, "system");
  }

  function shutdown(): void {
    window.removeEventListener("keydown", onGlobalKey);
  }

  // ---- 派生数据 ----
  const viewOptions = VIEW_BUTTONS.map((v) => ({ value: v, label: viewLabel(v) }));
  const unitOptions = UNIT_OPTIONS;

  const selectedText = computed<string>(() =>
    selected.value ? `${selected.value.id} · ${selected.value.label}` : "无"
  );

  /** 悬浮工具条：贴在锚点上方居中，左右各留 210px 防出界。 */
  const toolbarStyle = computed<Record<string, string>>(() => {
    const a = featureAnchor.value;
    if (!a) return {};
    const style: Record<string, string> = {
      left: `clamp(210px, ${a.x}px, calc(100% - 210px))`,
      top: `${Math.max(a.y, 64)}px`,
    };
    return style;
  });

  /** 从当前 FeatureSpec 解析选中特征的可编辑参数（sketch→dims, extrude→depth, hole→dia），已含 edits 覆盖值。 */
  const editableParams = computed<EditableParam[]>(() => {
    if (!selected.value) return [];
    try {
      const spec = JSON.parse(specText.value) as {
        features?: Array<{ id: string; op: string; dims?: { w?: number; h?: number }; depth?: number; dia?: number }>;
        edits?: Array<{ target: string; value: number }>;
      };
      const f = spec.features?.find((x) => x.id === selected.value!.id);
      if (!f) return [];
      const applied = (target: string, base: number): number =>
        spec.edits?.filter((e) => e.target === target).slice(-1)[0]?.value ?? base;
      const out: EditableParam[] = [];
      if (f.op === "sketch") {
        if (f.dims?.w != null) out.push({ target: `${f.id}.dims.w`, label: "宽度 w", value: applied(`${f.id}.dims.w`, f.dims.w) });
        if (f.dims?.h != null) out.push({ target: `${f.id}.dims.h`, label: "高度 h", value: applied(`${f.id}.dims.h`, f.dims.h) });
      } else if (f.op === "extrude" && f.depth != null) {
        out.push({ target: `${f.id}.depth`, label: "深度", value: applied(`${f.id}.depth`, f.depth) });
      } else if (f.op === "hole" && f.dia != null) {
        out.push({ target: `${f.id}.dia`, label: "直径", value: applied(`${f.id}.dia`, f.dia) });
      }
      return out;
    } catch {
      return [];
    }
  });

  const treeData = computed<TreeNodeData[]>(() => [
    {
      id: "__model__",
      label: modelName.value,
      children: features.value.map((f) => ({ id: f.id, label: f.label })),
    },
  ]);

  const menuGroups = computed<MenuGroup[]>(() => [
    {
      name: "文件",
      items: [
        { label: "新建", shortcut: "Ctrl+N", action: newModel },
        { label: "打开…", shortcut: "Ctrl+O", action: openModel },
        { label: "保存 FeatureSpec", shortcut: "Ctrl+S", action: saveModel },
        { label: "导出 STEP", action: () => flash("导出 STEP 需连接 Creo 内核（ROADMAP P2.1）") },
        { label: "退出", action: onClose },
      ],
    },
    {
      name: "编辑",
      items: [
        { label: "撤销", shortcut: "Ctrl+Z", action: undo },
        { label: "重做", shortcut: "Ctrl+Y", action: redo },
        { label: "首选项…", action: openPrefs },
      ],
    },
    {
      name: "视图",
      items: [
        { label: "等轴测", shortcut: "1", active: view.value === "iso", action: () => applyView("iso") },
        { label: "前视", shortcut: "2", active: view.value === "front", action: () => applyView("front") },
        { label: "后视", active: view.value === "back", action: () => applyView("back") },
        { label: "左视", active: view.value === "left", action: () => applyView("left") },
        { label: "右视", shortcut: "4", active: view.value === "right", action: () => applyView("right") },
        { label: "顶视", shortcut: "3", active: view.value === "top", action: () => applyView("top") },
        { label: "底视", active: view.value === "bottom", action: () => applyView("bottom") },
        { label: "适应窗口", shortcut: "F", action: fitView },
        { label: "线框", shortcut: "W", active: wireframe.value, action: () => (wireframe.value = true) },
        { label: "着色", active: !wireframe.value, action: () => (wireframe.value = false) },
        { label: "网格", shortcut: "G", active: showGrid.value, action: () => (showGrid.value = !showGrid.value) },
        { label: "坐标轴", shortcut: "X", active: showAxes.value, action: () => (showAxes.value = !showAxes.value) },
      ],
    },
    {
      name: "特征",
      items: [
        { label: "校验 FeatureSpec", action: () => void validateSpec() },
        { label: "重建预览模型", action: rebuild },
        { label: "模型树", active: sideOpen.value && sideTab.value === "tree", action: openTree },
      ],
    },
    {
      name: "工具",
      items: [
        { label: "单位 mm", active: unit.value === "mm", action: () => { unit.value = "mm"; flash("单位：mm"); } },
        { label: "单位 inch", active: unit.value === "inch", action: () => { unit.value = "inch"; flash("单位：inch"); } },
        { label: "清空会话", action: () => { messages.value = []; } },
      ],
    },
    {
      name: "帮助",
      items: [
        { label: "命令列表", action: () => runCommand("help") },
        { label: "关于语造", action: () => flash("语造 · 自然语言驱动的 Creo 原生参数化建模 · v0.1.0") },
      ],
    },
  ]);

  const menuHandlers = computed<Record<string, MenuItem>>(() => {
    const map: Record<string, MenuItem> = {};
    for (const g of menuGroups.value) for (const it of g.items) map[it.label] = it;
    return map;
  });

  // ---- 响应式副作用 ----
  watch(wireframe, (on) => viewport?.setWireframe(on));
  watch(showGrid, (on) => viewport?.setGrid(on));
  watch(showAxes, (on) => viewport?.setAxes(on));
  watch(view, (v) => viewport?.setView(v));
  watch(fullscreen, (on) => {
    if (on) return;
    window.clearTimeout(collapseTimer);
    menubarOpen.value = false;
    openMenus.value = 0;
  });

  return {
    // 窗口框架
    fullscreen, minimize, toggleMaximize, toggleFullscreen, close,
    // 视口与显示
    canvasEl, canvasWrap, view, wireframe, showGrid, showAxes, unit, unitOptions,
    viewportError, cursor, featureAnchor, viewOptions, viewLabel,
    // FeatureSpec 与特征树
    specText, features, selected, modelName, featureCount,
    // 侧栏与会话
    sideTab, sideOpen, messages, input,
    // 悬浮 AI 工具条
    aiPrompt, toolbarStyle,
    // 校验 / 首选项
    running, prefOpen,
    // 全屏顶栏
    menubarOpen, onTopbarEnter, onTopbarLeave, onMenuOpen, onMenuClose, onMenuSelect, revealMenubar,
    // 派生数据
    selectedText, editableParams, treeData, menuGroups,
    // 动作
    newModel, openModel, onFilePicked, onFilePickerReady, saveModel, undo, redo, openPrefs, resetPrefs,
    selectFeature, opLabel, onNodeClick, applyView, fitView, validateSpec, rebuild,
    onParamEdit, applyFeaturePrompt, submitCommand, runCommand, onInputKey, onGlobalKey,
    onClose, openTree, openLog, say, flash,
    // 生命周期
    mountViewport, unmountViewport, boot, shutdown,
  };
}

type Cad = ReturnType<typeof createCad>;

let shared: Cad | null = null;

/**
 * 返回全局唯一的 CAD 状态。首次调用（App 根组件 setup）构建，
 * 之后各子组件拿到同一份 refs 与方法。
 */
export function useCad(): Cad {
  if (!shared) shared = createCad();
  return shared;
}
