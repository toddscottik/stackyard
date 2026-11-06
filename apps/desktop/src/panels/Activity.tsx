import { useEffect, useRef, useState } from "react";
import { api, errorText, formatProgress, type CatalogItem, type CreatePlan, type Operation, type Task } from "../api";

type DependencyInput = {
  projectId: string;
  action: string;
  packageName: string;
  scope: string;
  scopeId?: string;
};

type Request =
  | { kind: "install"; item: CatalogItem }
  | { kind: "task"; input: DependencyInput }
  | { kind: "create"; plan: CreatePlan };
type Entry = {
  id: string;
  kind: "install" | "task";
  label: string;
  state: string;
  message: string;
  engineId: number | null;
  operation?: Operation;
  request?: Request;
  cancelling?: boolean;
  loadingOutput?: boolean;
  output?: string;
  error?: string;
};
type InstallWaiter = (entry: Entry) => void;

const pollInterval = 750;
const historyLimit = 128;

function operationEntry(operation: Operation, engineId: number | null): Entry {
  return {
    id: operation.id,
    kind: "install",
    label: operation.catalogId,
    state: operation.state,
    message: operation.message,
    engineId,
    operation,
  };
}

function taskEntry(task: Task, engineId: number | null): Entry {
  return {
    id: task.id,
    kind: "task",
    label: task.label,
    state: task.state,
    message: task.message,
    engineId,
  };
}

function requestKey(request: Request): string {
  switch (request.kind) {
    case "install": return `install:${request.item.id}`;
    case "task": return `deps:${request.input.projectId}`;
    case "create": return `create:${request.plan.request.name}`;
  }
}

function dependencyProject(entry: Entry): string | undefined {
  if (entry.request?.kind === "task") {
    return entry.request.input.projectId;
  }
  return entry.kind === "task" ? /^deps \S+ ([^/ ]+)/.exec(entry.label)?.[1] : undefined;
}

function mergeEntry(previous: Entry | undefined, incoming: Entry): Entry {
  if (previous && incoming.state === "running" && previous.state !== "running") {
    return { ...previous, request: incoming.request ?? previous.request };
  }
  return {
    ...previous,
    ...incoming,
    cancelling: incoming.state === "running" && previous?.cancelling,
  };
}

async function entryStatus(entry: Entry, engineId: number | null): Promise<Entry> {
  if (entry.kind === "install") {
    return operationEntry(await api.installStatus(entry.id), engineId);
  }
  return taskEntry(await api.task(entry.id), engineId);
}

export function useActivity({ enabled, engineId, onInstalled }: {
  enabled: boolean;
  engineId: number | null;
  onInstalled: () => void;
}) {
  const [entries, setEntries] = useState<Entry[]>([]);
  const ledger = useRef<Entry[]>([]);
  const [pending, setPending] = useState<ReadonlySet<string>>(new Set());
  const submitting = useRef(new Set<string>());
  const installPromises = useRef(new Map<string, Promise<void>>());
  const installWaiters = useRef(new Map<string, InstallWaiter>());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const refresh = useRef(() => {});
  const polling = useRef<Promise<void> | null>(null);
  const installed = useRef(onInstalled);
  const commitRef = useRef(commit);

  useEffect(() => {
    installed.current = onInstalled;
    commitRef.current = commit;
  });

  function commit(next: Entry[]) {
    const previous = new Map(ledger.current.map((entry) => [entry.id, entry]));
    const completed = next.filter((entry) => entry.state !== "running" && previous.get(entry.id)?.state === "running");
    const completedIds = new Set(completed.map((entry) => entry.id));
    let terminals = 0;
    const bounded = [...completed, ...next.filter((entry) => !completedIds.has(entry.id))]
      .filter((entry) => entry.state === "running" || ++terminals <= historyLimit);
    ledger.current = bounded;
    setEntries(bounded);
    for (const entry of completed) {
      const waiter = installWaiters.current.get(entry.id);
      if (waiter) {
        installWaiters.current.delete(entry.id);
        waiter(entry);
      }
    }
    if (bounded.some((entry) => {
      const before = previous.get(entry.id);
      return entry.kind === "install" && entry.state === "succeeded" &&
        (before ? before.state !== "succeeded" : entry.request !== undefined);
    })) {
      installed.current();
    }
  }

  function upsert(incoming: Entry, removeId?: string) {
    const current = ledger.current.filter((entry) => entry.id !== removeId);
    const previous = current.find((entry) => entry.id === incoming.id);
    const merged = mergeEntry(previous, incoming);
    commit(previous ? current.map((entry) => entry.id === incoming.id ? merged : entry) : [merged, ...current]);
  }

  function patch(id: string, value: Partial<Entry>) {
    commit(ledger.current.map((entry) => entry.id === id ? { ...entry, ...value } : entry));
  }

  useEffect(() => {
    if (!enabled) {
      setLoading(false);
      return;
    }

    let active = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    setLoading(true);

    async function readActivity() {
      const tracked = ledger.current.filter((entry) => entry.state === "running");
      try {
        const incoming = new Map<string, Entry>();
        let refreshError = "";
        try {
          const result = await api.activity();
          for (const operation of result.operations) {
            incoming.set(operation.id, operationEntry(operation, engineId));
          }
          for (const task of result.tasks) {
            incoming.set(task.id, taskEntry(task, engineId));
          }
        } catch (err) {
          refreshError = errorText(err);
        }
        if (!active) return;

        // A bounded list can omit a tracked ID. Query that ID before changing its status.
        for (const entry of tracked) {
          if (incoming.has(entry.id)) continue;
          try {
            incoming.set(entry.id, await entryStatus(entry, engineId));
          } catch (err) {
            const noun = entry.kind === "install" ? "install" : "task";
            if (!errorText(err).includes(`${noun} ${entry.id} was not found`)) {
              refreshError ||= errorText(err);
              if (!active) return;
              continue;
            }
            const restarted = engineId !== entry.engineId;
            incoming.set(entry.id, {
              ...entry,
              state: restarted ? "interrupted" : "unavailable",
              message: restarted ? "The engine restarted before reporting a result." : "The engine no longer has a result for this activity.",
              output: undefined,
            });
          }
          if (!active) return;
        }

        const existing = ledger.current;
        const updated = existing.map((entry) => {
          const latest = incoming.get(entry.id);
          if (!latest) return entry;
          incoming.delete(entry.id);
          // A cancel or status response may have resolved during this poll.
          if (entry.state !== "running" && (latest.state === "interrupted" || latest.state === "unavailable")) {
            return entry;
          }
          return mergeEntry(entry, latest);
        });
        commitRef.current([...incoming.values(), ...updated]);
        setError(refreshError);
      } catch (err) {
        if (active) setError(errorText(err));
      }
    }

    async function poll() {
      timer = undefined;
      if (polling.current) await polling.current;
      if (!active) return;
      const reading = readActivity();
      polling.current = reading;
      await reading;
      if (polling.current === reading) polling.current = null;
      if (active) {
        setLoading(false);
        timer = setTimeout(poll, pollInterval);
      }
    }

    refresh.current = () => {
      if (timer === undefined) return;
      clearTimeout(timer);
      setLoading(true);
      void poll();
    };
    timer = setTimeout(poll, 0);
    return () => {
      active = false;
      clearTimeout(timer);
      refresh.current = () => {};
    };
  }, [enabled, engineId]);

  function isRunning(request: Request) {
    return ledger.current.some((entry) => {
      if (entry.state !== "running") return false;
      switch (request.kind) {
        case "install": return entry.operation?.catalogId === request.item.id;
        case "task": return dependencyProject(entry) === request.input.projectId;
        case "create": return entry.label === `create project ${request.plan.request.name}`;
      }
    });
  }

  async function start(request: Request) {
    const key = requestKey(request);
    if (!enabled || submitting.current.has(key) || isRunning(request)) return;
    submitting.current.add(key);
    setPending(new Set(submitting.current));

    const failureId = `request:${key}`;
    try {
      let result: Entry;
      if (request.kind === "install") {
        result = operationEntry(await api.startInstall(request.item.id), engineId);
      } else if (request.kind === "create") {
        result = taskEntry(await api.applyCreate(request.plan), engineId);
      } else {
        const { projectId, scopeId, scope, action, packageName } = request.input;
        const task = scopeId
          ? await api.startDependencyScoped(projectId, scopeId, action, packageName)
          : await api.startDependency(projectId, action, packageName, scope);
        result = taskEntry(task, engineId);
      }
      upsert({ ...result, request }, failureId);
      return result;
    } catch (err) {
      const label = request.kind === "install"
        ? request.item.id
        : request.kind === "create"
          ? `create project ${request.plan.request.name}`
          : `deps ${request.input.action} ${request.input.projectId}`;
      upsert({ id: failureId, kind: request.kind === "install" ? "install" : "task", engineId, label, request, state: "request failed", message: errorText(err) });
      throw err;
    } finally {
      submitting.current.delete(key);
      setPending(new Set(submitting.current));
    }
  }

  function startInstall(item: CatalogItem): Promise<void> {
    const pendingInstall = installPromises.current.get(item.id);
    if (pendingInstall) return pendingInstall;
    if (!enabled) return Promise.reject(new Error("Connect to the engine before installing a package."));

    const install = async () => {
      const request = { kind: "install", item } as const;
      const running = ledger.current.find((entry) => entry.operation?.catalogId === item.id && entry.state === "running");
      if (running) patch(running.id, { request });
      const started = running ?? await start(request);
      if (!started) throw new Error("The engine did not report a package installation.");
      const entry = ledger.current.find((candidate) => candidate.id === started.id) ?? started;
      const final = entry.state === "running"
        ? await new Promise<Entry>((resolve) => installWaiters.current.set(entry.id, resolve))
        : entry;
      if (final.state !== "succeeded") throw new Error(final.message || final.state);
    };
    const promise = install().finally(() => installPromises.current.delete(item.id));
    installPromises.current.set(item.id, promise);
    return promise;
  }

  async function cancel(id: string) {
    const entry = ledger.current.find((candidate) => candidate.id === id);
    if (!enabled || !entry || entry.state !== "running" || entry.cancelling) return;
    patch(id, { cancelling: true, error: undefined });
    try {
      const result = entry.kind === "install"
        ? operationEntry(await api.cancelInstall(id), engineId)
        : taskEntry(await api.cancelTask(id), engineId);
      upsert(result);
    } catch (err) {
      patch(id, { cancelling: false, error: errorText(err) });
    }
  }

  async function loadOutput(id: string) {
    const entry = ledger.current.find((candidate) => candidate.id === id);
    if (!enabled || !entry || entry.loadingOutput) return;
    patch(id, { loadingOutput: true, error: undefined });
    try {
      const result = await entryStatus(entry, engineId);
      upsert({ ...result, output: result.message, loadingOutput: false });
    } catch (err) {
      patch(id, { loadingOutput: false, error: errorText(err) });
    }
  }

  return {
    entries,
    enabled,
    loading,
    error,
    refresh: () => refresh.current(),
    startInstall,
    startCreate: async (plan: CreatePlan): Promise<Task> => {
      const entry = await start({ kind: "create", plan });
      if (!entry) throw new Error("Connect to the engine or wait for the current creation operation.");
      return entry;
    },
    startScopedInstall: async (projectId: string, scopeId: string): Promise<Task> => {
      const entry = await start({
        kind: "task",
        input: { projectId, scopeId, action: "install", scope: "", packageName: "" },
      });
      if (!entry) throw new Error("Connect to the engine or wait for the current dependency operation.");
      return entry;
    },
    installing: (id: string) => pending.has(`install:${id}`) || entries.some((entry) => entry.operation?.catalogId === id && entry.state === "running"),
    startDependency: async (input: DependencyInput) => {
      await start({ kind: "task", input: { ...input, packageName: input.packageName.trim(), scope: input.scope.trim() } });
    },
    dependencyBusy: (projectId: string) => pending.has(`deps:${projectId}`) || entries.some((entry) => dependencyProject(entry) === projectId && entry.state === "running"),
    dependencyEntry: (projectId: string) => entries.find((entry) => dependencyProject(entry) === projectId && entry.state === "running") ??
      entries.find((entry) => dependencyProject(entry) === projectId),
    cancel,
    loadOutput,
    retry: async (request: Request) => {
      try {
        if (request.kind === "install") await startInstall(request.item);
        else await start(request);
      } catch {
        // The request or terminal error is already visible in the activity history.
      }
    },
    retrying: (request: Request) => pending.has(requestKey(request)) || isRunning(request),
  };
}

export type ActivityController = ReturnType<typeof useActivity>;

export function Activity({ controller }: { controller: ActivityController }) {
  const { entries, enabled, loading, error } = controller;
  const sorted = [...entries].sort((a, b) => Number(b.state === "running") - Number(a.state === "running"));
  return (
    <section className="pane" aria-labelledby="activity-heading">
      <h2 id="activity-heading">Activity</h2>
      {!enabled && <p className="meta">Connect to the engine to refresh activity.</p>}
      {loading && <p className="meta" role="status">Loading activity…</p>}
      {error && (
        <p role="alert">{error} <button className="btn" disabled={!enabled || loading} onClick={controller.refresh}>Refresh</button></p>
      )}
      {enabled && !loading && !error && entries.length === 0 && <p className="meta">No activity yet.</p>}
      {sorted.map((entry) => {
        const running = entry.state === "running";
        const operation = entry.operation;
        const retry = !running && entry.state !== "succeeded" ? entry.request : undefined;
        return (
          <article key={entry.id} className="block">
            <h3>{entry.label}</h3>
            <p className="detail-line">
              <span className="mono">{entry.id}</span> · {entry.cancelling ? "cancelling" : entry.state}
              {operation && (running || operation.bytes > 0) && ` · ${formatProgress(operation.bytes, operation.total)}`}
            </p>
            {operation && running && (
              <progress
                aria-label={`Download progress for ${entry.label}`}
                value={operation.total > 0 ? Math.min(operation.bytes, operation.total) : undefined}
                max={operation.total || undefined}
              />
            )}
            {(entry.output ?? entry.message) && <pre className="output">{entry.output ?? entry.message}</pre>}
            {entry.error && <p role="alert">{entry.error}</p>}
            {running && (
              <button className="btn" disabled={!enabled || entry.cancelling} onClick={() => void controller.cancel(entry.id)}>Cancel</button>
            )}
            {retry && (
              <button className="btn" disabled={!enabled || controller.retrying(retry)} onClick={() => void controller.retry(retry)}>Retry</button>
            )}
            {!running && entry.message.startsWith("…") && entry.output === undefined && (
              <button className="btn" disabled={!enabled || entry.loadingOutput} onClick={() => void controller.loadOutput(entry.id)}>
                {entry.loadingOutput ? "Loading output…" : "Load full output"}
              </button>
            )}
          </article>
        );
      })}
    </section>
  );
}
