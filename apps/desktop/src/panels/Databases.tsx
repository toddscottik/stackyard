import { useEffect, useRef, useState } from "react";
import { api, type CatalogItem, type Installed, type Databases, type Project } from "../api";

export function DatabasesScreen({
  databases,
  packages,
  catalog,
  projects,
  desktop,
  onDatabases,
  onInstall,
  installing,
  onRefresh,
}: {
  databases: Databases | null;
  packages: Installed[];
  catalog: CatalogItem[];
  projects: Project[];
  desktop: boolean;
  onDatabases: (view: Databases) => void;
  onInstall: (item: CatalogItem) => Promise<void>;
  installing: (catalogId: string) => boolean;
  onRefresh: () => void;
}) {
  const [engine, setEngine] = useState("mysql");
  const [port, setPort] = useState("");
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const [localError, setLocalError] = useState("");
  const [projectId, setProjectId] = useState(projects[0]?.id ?? "");
  const [host, setHost] = useState("127.0.0.1");
  const [attachPort, setAttachPort] = useState("13306");
  const [database, setDatabase] = useState("");
  const [user, setUser] = useState("");
  const [password, setPassword] = useState("");
  const [sqliteProject, setSqliteProject] = useState(projects[0]?.id ?? "");
  const [sqlitePath, setSqlitePath] = useState("data/app.sqlite");

  useEffect(() => {
    if (!projectId && projects[0]) {
      setProjectId(projects[0].id);
    }
    if (!sqliteProject && projects[0]) {
      setSqliteProject(projects[0].id);
    }
  }, [projectId, sqliteProject, projects]);

  async function run(action: () => Promise<Databases>) {
    if (busyRef.current) return;
    busyRef.current = true; setBusy(true); setLocalError("");
    try { onDatabases(await action()); }
    catch (err) { setLocalError(err instanceof Error ? err.message : String(err)); }
    finally { busyRef.current = false; setBusy(false); }
  }

  const instances = databases?.instances ?? [];
  const bindings = databases?.bindings ?? [];
  const sqlite = databases?.sqlite ?? [];
  const finding =
    instances.length === 0 && bindings.length === 0 && sqlite.length === 0
      ? "No database is recorded. A managed instance keeps its files when a project stops."
      : `${instances.filter((row) => row.state === "running").length} managed ${
          instances.filter((row) => row.state === "running").length === 1 ? "instance" : "instances"
        } running.`;

  return (
    <>
      <p className="finding">{databases ? finding : "Database status appears when this window can reach the engine."}</p>
      {localError ? <p className="error" role="alert">{localError}</p> : null}
      <p className="hint">
        Stopping a project leaves a shared MySQL or PostgreSQL instance running. An external connection has no process to stop. Removing a runtime does not delete database files. Major-version migration is not available.
      </p>
      <section className="block" aria-label="Managed databases">
        <h2>Managed instances</h2>
        {instances.length === 0 ? <p className="empty">No managed MySQL or PostgreSQL instance.</p> : null}
        <ul className="slips">
          {instances.map((row) => (
            <li key={row.id} className="slip">
              <span className="slip-name">{row.engine}</span>
              <span className={row.state === "running" ? "pill pos" : "pill"}>{row.state}</span>
              <span className="slip-sub">
                <span className="mono">127.0.0.1:{row.port}</span>
                <span className="mono">{row.dataDir}</span>
              </span>
              <button
                type="button"
                className="btn"
                disabled={!desktop || busy}
                onClick={() =>
                  void run(() =>
                    row.state === "running" ? api.stopDatabase(row.id) : api.startDatabase(row.id),
                  )
                }
              >
                {row.state === "running" ? "Stop" : "Start"}
              </button>
            </li>
          ))}
        </ul>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            const chosen = port.trim() === "" ? 0 : Number(port);
            if (!Number.isInteger(chosen) || chosen < 0 || chosen > 65535) {
              setLocalError("Database port must be empty or a high port.");
              return;
            }
            void run(async () => {
              let recorded = packages.find((item) => item.name === engine);
              if (!recorded) {
                const item = catalog.find((entry) => entry.name === engine);
                if (!item) throw new Error(`No ${engine} package is published for this machine.`);
                await onInstall(item);
                recorded = { id: item.id, name: item.name, source: "managed", version: item.version, path: "", launch: "" };
              }
              await api.selectPackage("default", recorded.id);
              const view = await api.createDatabase(engine, chosen);
              onRefresh();
              return view;
            });
          }}
        >
          <div className="field">
            <label htmlFor="db-engine">Engine</label>
            <select id="db-engine" className="input" value={engine} onChange={(event) => setEngine(event.target.value)}>
              <option value="mysql">MySQL</option>
              <option value="postgres">PostgreSQL</option>
            </select>
          </div>
          <p className="hint">
            {packages.some((item) => item.name === engine)
              ? `Using the recorded ${engine} server. The new instance gets its own data directory.`
              : `Create downloads ${engine === "mysql" ? "MySQL 8.4" : "PostgreSQL 18"} into Stackyard, then starts a private instance. It does not register a Windows service.`}
          </p>
          {catalog.filter((item) => item.name === engine && installing(item.id)).map((item) => <p key={item.id} className="hint" role="status">Installing {item.name} {item.version}…</p>)}
          <div className="field">
            <label htmlFor="db-port">Port</label>
            <input
              id="db-port"
              className="input mono"
              value={port}
              onChange={(event) => setPort(event.target.value)}
              placeholder="Empty picks a free high port"
              inputMode="numeric"
            />
          </div>
          <button type="submit" className="btn primary" disabled={!desktop || busy || catalog.some((item) => item.name === engine && installing(item.id))}>
            Create instance
          </button>
        </form>
      </section>
      <section className="block" aria-label="Database connections">
        <h2>Connections</h2>
        {bindings.length === 0 ? <p className="empty">No project database or external connection.</p> : null}
        <ul className="slips">
          {bindings.map((row) => (
            <li key={`${row.projectId}:${row.connectionId}`} className="slip">
              <span className="slip-name">{row.databaseName}</span>
              <span className="pill">{row.external ? "external" : row.engine}</span>
              <span className="slip-sub">
                <span>
                  {row.projectId} · {row.userName}
                </span>
                <span className="mono">
                  {row.host}:{row.port}
                </span>
              </span>
              <button
                type="button"
                className="btn"
                disabled={!desktop || busy}
                onClick={() =>
                  void run(() => api.detachDatabase(row.projectId, row.connectionId))
                }
              >
                Detach
              </button>
            </li>
          ))}
        </ul>
        <h2>Connect existing</h2>
        <p className="hint">Loopback only. The password is stored for this user and is not shown again.</p>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            const chosen = Number(attachPort);
            if (!projectId) {
              setLocalError("Import a project before attaching a database.");
              return;
            }
            if (!Number.isInteger(chosen) || chosen < 1024 || chosen > 65535) {
              setLocalError("An external database port must be a high port.");
              return;
            }
            void run(async () => {
              const view = await api.attachDatabase({
                projectId,
                engine,
                host: host.trim(),
                port: chosen,
                database: database.trim(),
                user: user.trim(),
                password,
              });
              setPassword("");
              setDatabase("");
              return view;
            });
          }}
        >
          <div className="field">
            <label htmlFor="db-project">Project</label>
            <select id="db-project" className="input" value={projectId} onChange={(event) => setProjectId(event.target.value)}>
              {projects.length === 0 ? <option value="">No project imported</option> : null}
              {projects.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name}
                </option>
              ))}
            </select>
          </div>
          <div className="field">
            <label htmlFor="db-host">Host</label>
            <input id="db-host" className="input mono" value={host} onChange={(event) => setHost(event.target.value)} />
          </div>
          <div className="field">
            <label htmlFor="db-attach-port">Port</label>
            <input
              id="db-attach-port"
              className="input mono"
              value={attachPort}
              onChange={(event) => setAttachPort(event.target.value)}
              inputMode="numeric"
            />
          </div>
          <div className="field">
            <label htmlFor="db-name">Database</label>
            <input id="db-name" className="input" value={database} onChange={(event) => setDatabase(event.target.value)} />
          </div>
          <div className="field">
            <label htmlFor="db-user">User</label>
            <input id="db-user" className="input" value={user} onChange={(event) => setUser(event.target.value)} />
          </div>
          <div className="field">
            <label htmlFor="db-password">Password</label>
            <input
              id="db-password"
              className="input"
              type="password"
              value={password}
              autoComplete="off"
              onChange={(event) => setPassword(event.target.value)}
            />
          </div>
          <button type="submit" className="btn primary" disabled={!desktop || password === ""}>
            Connect existing
          </button>
        </form>
      </section>
      <section className="block" aria-label="SQLite">
        <h2>SQLite</h2>
        <p className="hint">A SQLite file is not a service. Backup copies the file through the engine.</p>
        {sqlite.length === 0 ? <p className="empty">No project has selected a SQLite file.</p> : null}
        <ul className="slips">
          {sqlite.map((row) => (
            <li key={row.projectId} className="slip">
              <span className="slip-name">{row.projectId}</span>
              <span className="slip-sub">
                <span className="mono">{row.path}</span>
              </span>
            </li>
          ))}
        </ul>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!sqliteProject) {
              setLocalError("Import a project before selecting a SQLite file.");
              return;
            }
            void run(() => api.useSqlite(sqliteProject, sqlitePath.trim()));
          }}
        >
          <div className="field">
            <label htmlFor="sqlite-project">Project</label>
            <select
              id="sqlite-project"
              className="input"
              value={sqliteProject}
              onChange={(event) => setSqliteProject(event.target.value)}
            >
              {projects.length === 0 ? <option value="">No project imported</option> : null}
              {projects.map((project) => (
                <option key={project.id} value={project.id}>
                  {project.name}
                </option>
              ))}
            </select>
          </div>
          <div className="field">
            <label htmlFor="sqlite-path">File inside the project</label>
            <input
              id="sqlite-path"
              className="input mono"
              value={sqlitePath}
              onChange={(event) => setSqlitePath(event.target.value)}
            />
          </div>
          <button type="submit" className="btn" disabled={!desktop || busy}>
            Use SQLite file
          </button>
        </form>
      </section>
    </>
  );
}
