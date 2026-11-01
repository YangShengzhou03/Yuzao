import { onBeforeUnmount, onMounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * 无边框窗口（`decorations: false`）的自绘标题栏控制。
 * 暴露最小化 / 最大化切换 / 全屏切换 / 关闭，并同步状态（按钮图标随状态切换）。
 * F11 为全屏快捷键；非 Tauri 环境（纯浏览器预览）下所有操作为空操作。
 */
export function useWindowFrame() {
  const win = "__TAURI_INTERNALS__" in window ? getCurrentWindow() : null;
  const maximized = ref(false);
  const fullscreen = ref(false);
  let unlistenResized: (() => void) | undefined;

  async function refresh(): Promise<void> {
    if (!win) return;
    maximized.value = await win.isMaximized();
    fullscreen.value = await win.isFullscreen();
  }

  function onKeydown(e: KeyboardEvent): void {
    if (e.key === "F11") {
      e.preventDefault();
      toggleFullscreen();
    }
  }

  onMounted(async () => {
    if (!win) return;
    await refresh();
    unlistenResized = await win.onResized(refresh);
    window.addEventListener("keydown", onKeydown);
  });

  onBeforeUnmount(() => {
    unlistenResized?.();
    window.removeEventListener("keydown", onKeydown);
  });

  function minimize(): void {
    void win?.minimize();
  }

  function toggleMaximize(): void {
    void win?.toggleMaximize().then(refresh).catch(() => undefined);
  }

  function toggleFullscreen(): void {
    void win?.setFullscreen(!fullscreen.value).then(refresh).catch(() => undefined);
  }

  function close(): void {
    void win?.close();
  }

  return { maximized, fullscreen, minimize, toggleMaximize, toggleFullscreen, close };
}
