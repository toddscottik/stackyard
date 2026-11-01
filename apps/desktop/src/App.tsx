import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

type Status = {
  pid: number;
  dataDir: string;
  apiVersion: number;
  schemaVersion: number;
  lastEvent: string | null;
};

type CatalogItem = { id: string; name: string; version: string; target: string };
type Installed = { id: string; source: string; version: string; path: string; launch: string };

export function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [setting, setSetting] = useState("");
  const [error, setError] = useState("");
  const [catalog, setCatalog] = useState<CatalogItem[]>([]);
  const [packages, setPackages] = useState<Installed[]>([]);
  const [existingPath, setExistingPath] = useState("");
  const [busy, setBusy] = useState("");

  async function refresh() {
    const next = await invoke<Status>("engine_status");
    const value = await invoke<string | null>("engine_setting", {
      key: "display_name",
    });
    setStatus(next);
    setSetting(value ?? "");
    setError("");
  }

  async function loadPackages() {
    const [nextCatalog, nextPackages] = await Promise.all([
      invoke<CatalogItem[]>("list_catalog"),
      invoke<Installed[]>("list_packages"),
    ]);
    setCatalog(nextCatalog);
    setPackages(nextPackages);
  }

  useEffect(() => {
    refresh()
      .then(() => loadPackages())
      .catch((err: unknown) => {
        setError(err instanceof Error ? err.message : String(err));
      });
  }, []);

  async function save() {
    try {
      await invoke("put_setting", { key: "display_name", value: setting });
      await refresh();
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : String(err));
    }
  }

  return (
    <main>
      <h1>Stackyard</h1>
      <p>This window and the CLI use the same engine. Node uses pnpm.</p>
      {error ? (
        <p role="alert">{error}</p>
      ) : null}
      {status ? (
        <dl>
          <div>
            <dt>Engine pid</dt>
            <dd>{status.pid}</dd>
          </div>
          <div>
            <dt>Data directory</dt>
            <dd>{status.dataDir}</dd>
          </div>
          <div>
            <dt>API / schema</dt>
            <dd>
              {status.apiVersion} / {status.schemaVersion}
            </dd>
          </div>
          <div>
            <dt>Last event</dt>
            <dd>{status.lastEvent ?? "none"}</dd>
          </div>
        </dl>
      ) : (
        <p>Contacting the engine…</p>
      )}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <label htmlFor="display-name">Display name</label>
        <input
          id="display-name"
          value={setting}
          onChange={(event) => setSetting(event.target.value)}
        />
        <button type="submit">Save</button>
      </form>
      <h2>Packages</h2>
      <ul>
        {packages.map((item) => (
          <li key={item.id}>
            {item.id} · {item.source} · {item.version}
            <div>{item.launch}</div>
          </li>
        ))}
      </ul>
      {packages.length === 0 ? <p>No packages recorded.</p> : null}
      <ul>
        {catalog
          .filter((item) => item.name === "node" || item.name === "python")
          .map((item) => {
            const installed = packages.find((pkg) => pkg.id === item.id);
            const manager = item.name === "node"
              ? catalog.find((pkg) => pkg.name === "pnpm")
              : catalog.find((pkg) => pkg.name === "uv");
            const managerInstalled = manager
              ? packages.find((pkg) => pkg.id === manager.id)
              : undefined;
            return (
              <li key={item.id}>
                {installed ? (
                  <span>
                    Installed {item.name} {item.version}
                    <div>{installed.launch}</div>
                  </span>
                ) : (
                  <button
                    type="button"
                    disabled={busy !== ""}
                    onClick={() => {
                      setBusy(item.id);
                      setError("");
                      void invoke<string>("install_package", { catalogId: item.id })
                        .then(() => loadPackages())
                        .catch((err: unknown) => {
                          setError(err instanceof Error ? err.message : String(err));
                        })
                        .finally(() => setBusy(""));
                    }}
                  >
                    {busy === item.id ? "Installing…" : `Install ${item.name} ${item.version}`}
                  </button>
                )}
                {manager ? (
                  <div>
                    {item.name === "node" ? "Package manager: pnpm" : "Helper: uv"}{" "}
                    {managerInstalled ? (
                      <span>installed {managerInstalled.version}</span>
                    ) : (
                      <button
                        type="button"
                        disabled={busy !== ""}
                        onClick={() => {
                          setBusy(manager.id);
                          setError("");
                          void invoke<string>("install_package", { catalogId: manager.id })
                            .then(() => loadPackages())
                            .catch((err: unknown) => {
                              setError(err instanceof Error ? err.message : String(err));
                            })
                            .finally(() => setBusy(""));
                        }}
                      >
                        {busy === manager.id ? "Installing…" : `Install ${manager.name}`}
                      </button>
                    )}
                  </div>
                ) : null}
              </li>
            );
          })}
      </ul>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          setError("");
          void invoke<string>("use_existing", { role: "python", path: existingPath })
            .then(() => loadPackages())
            .catch((err: unknown) => {
              setError(err instanceof Error ? err.message : String(err));
            });
        }}
      >
        <label htmlFor="existing-python">Existing Python executable</label>
        <input
          id="existing-python"
          value={existingPath}
          onChange={(event) => setExistingPath(event.target.value)}
          placeholder="C:\\Python\\Python312\\python.exe"
        />
        <button type="submit">Use existing Python</button>
      </form>
    </main>
  );
}
