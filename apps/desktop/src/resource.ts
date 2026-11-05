export type ResourceSnapshot<T> = {
  value: T | undefined;
  loading: boolean;
  error: string;
};

/** One request owner. A mutation or disconnect invalidates work already in flight. */
export function createResource<T>(load: () => Promise<T>) {
  let snapshot: ResourceSnapshot<T> = { value: undefined, loading: false, error: "" };
  let enabled = false;
  let pending = false;
  let refreshAfter = false;
  let revision = 0;
  const listeners = new Set<() => void>();

  function publish(next: ResourceSnapshot<T>) {
    snapshot = next;
    for (const listener of listeners) listener();
  }

  function refresh() {
    if (!enabled || pending) return;
    pending = true;
    const requestRevision = revision;
    publish({ ...snapshot, loading: true });
    void Promise.resolve()
      .then(load)
      .then(
        (value) => {
          if (enabled && requestRevision === revision) publish({ value, loading: false, error: "" });
        },
        (error: unknown) => {
          if (enabled && requestRevision === revision) {
            publish({ ...snapshot, loading: false, error: String(error) });
          }
        },
      )
      .finally(() => {
        pending = false;
        if (refreshAfter) {
          refreshAfter = false;
          refresh();
        }
      });
  }

  function invalidate() {
    revision += 1;
    if (enabled && pending) refreshAfter = true;
    else refresh();
  }

  return {
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
    getSnapshot: () => snapshot,
    refresh,
    invalidate,
    setEnabled(next: boolean) {
      if (enabled === next) return;
      enabled = next;
      if (enabled) invalidate();
      else {
        revision += 1;
        refreshAfter = false;
        if (snapshot.loading) publish({ ...snapshot, loading: false });
      }
    },
    update(updater: (current: T | undefined) => T) {
      revision += 1;
      publish({ value: updater(snapshot.value), loading: false, error: "" });
    },
  };
}
