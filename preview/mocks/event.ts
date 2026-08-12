// A one-file replacement for Tauri's event bus.
type Handler = (event: { payload: unknown }) => void;

const listeners = new Map<string, Set<Handler>>();

export async function listen<T>(
  name: string,
  handler: (event: { payload: T }) => void,
): Promise<() => void> {
  const set = listeners.get(name) ?? new Set();
  set.add(handler as Handler);
  listeners.set(name, set);
  return () => set.delete(handler as Handler);
}

export function emit(name: string, payload: unknown) {
  listeners.get(name)?.forEach((handler) => handler({ payload }));
}

export type UnlistenFn = () => void;
