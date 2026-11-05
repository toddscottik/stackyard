import { useEffect, useMemo, useSyncExternalStore } from "react";
import { createResource } from "./resource";

export function useResource<T>(
  load: () => Promise<T>,
  { enabled, pollMs = 0, identity }: { enabled: boolean; pollMs?: number; identity?: number | null },
) {
  const controller = useMemo(() => createResource(load), [load, identity]);
  const snapshot = useSyncExternalStore(controller.subscribe, controller.getSnapshot);
  useEffect(() => {
    controller.setEnabled(enabled);
    const timer = enabled && pollMs > 0 ? window.setInterval(controller.refresh, pollMs) : undefined;
    return () => {
      if (timer !== undefined) window.clearInterval(timer);
      controller.setEnabled(false);
    };
  }, [controller, enabled, pollMs]);
  return { ...snapshot, refresh: controller.invalidate, update: controller.update };
}
