// Tauri IPC 调用封装：
// - 在 Tauri 桌面环境使用 @tauri-apps/api 的 invoke
// - 在纯浏览器（vite dev 预览）环境抛出错误，由调用方降级到 JS 求解器

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
  if (!inTauri) {
    throw new Error('NOT_IN_TAURI');
  }
  const mod = await import('@tauri-apps/api/core');
  return mod.invoke<T>(cmd, args);
}
