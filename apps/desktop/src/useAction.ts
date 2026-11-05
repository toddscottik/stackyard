import { useCallback, useRef, useState } from "react";
import { errorText } from "./api";

export function useAction() {
  const pending = useRef(new Set<string>());
  const [states, setStates] = useState<Record<string, { pending: boolean; error: string }>>({});

  const run = useCallback(async (key: string, action: () => Promise<void>) => {
    if (pending.current.has(key)) return false;
    pending.current.add(key);
    setStates((current) => ({ ...current, [key]: { pending: true, error: "" } }));
    try {
      await action();
      setStates((current) => ({ ...current, [key]: { pending: false, error: "" } }));
      return true;
    } catch (error) {
      setStates((current) => ({ ...current, [key]: { pending: false, error: errorText(error) } }));
      return false;
    } finally {
      pending.current.delete(key);
    }
  }, []);

  return { states, run };
}
