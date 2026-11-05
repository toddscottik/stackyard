import { useState } from "react";
import { api, type CatalogItem, type Installed, type UpdateNotice } from "../api";

export function Components({
  catalog,
  packages,
  desktop,
  onInstall,
  installing,
  onRefresh,
}: {
  catalog: CatalogItem[];
  packages: Installed[];
  desktop: boolean;
  onInstall: (item: CatalogItem) => Promise<void>;
  installing: (catalogId: string) => boolean;
  onRefresh: () => void;
}) {
  const [role, setRole] = useState("python");
  const [path, setPath] = useState("");
  const [installErrors, setInstallErrors] = useState<Record<string, string>>({});
  const [actionError, setActionError] = useState("");
  const [busyAction, setBusyAction] = useState(false);
  const [notices, setNotices] = useState<UpdateNotice[]>([]);
  const finding =
    packages.length === 0
      ? "No runtime is recorded on this machine."
      : `${packages.length} ${packages.length === 1 ? "tool" : "tools"} recorded.`;

  async function install(item: CatalogItem) {
    setInstallErrors((current) => ({ ...current, [item.id]: "" }));
    try { await onInstall(item); onRefresh(); }
    catch (err) { setInstallErrors((current) => ({ ...current, [item.id]: err instanceof Error ? err.message : String(err) })); }
  }

  async function runAction(action: () => Promise<unknown>) {
    if (busyAction) return;
    setBusyAction(true); setActionError("");
    try { await action(); onRefresh(); }
    catch (err) { setActionError(err instanceof Error ? err.message : String(err)); }
    finally { setBusyAction(false); }
  }

  return (
    <>
      <p className="finding">{finding}</p>
      <div className="pair">
        <section className="pane" aria-labelledby="recorded-heading">
          <h2 id="recorded-heading">Recorded</h2>
          {packages.length === 0 ? <p className="empty">No runtimes or tools are recorded yet.</p> : null}
          <ul className="slips">
            {packages.map((item) => (
              <li key={item.id} className="slip" aria-hidden="false">
                <span className="slip-name">{item.name}</span>
                <span className="pill">{item.source}</span>
                <span className="slip-sub">
                  <span className="mono">{item.version}</span>
                  <span className="mono">{item.launch}</span>
                </span>
              </li>
            ))}
          </ul>
        </section>
        <section className="pane">
          <h2>Install</h2>
          {catalog.length === 0 ? (
            <p className="empty">No managed packages are published for this operating system. Use an existing Node or Python.</p>
          ) : (
            <div className="row">
              {catalog.map((item) => {
                const installed = packages.some((pkg) => pkg.id === item.id);
                return installed ? (
                  <span key={item.id} className="pill pos">
                    Installed {item.name} {item.version}
                  </span>
                ) : (
                  <button
                    key={item.id}
                    type="button"
                    className="btn"
                    disabled={!desktop || installing(item.id)}
                    onClick={() => void install(item)}
                  >
                    Install {item.name} {item.version}
                  </button>
                );
              })}
            </div>
          )}
          {catalog.map((item) => installErrors[item.id] ? <p key={`${item.id}-error`} className="error" role="alert">{item.name}: {installErrors[item.id]}</p> : null)}
          <h2>Use existing</h2>
          <p className="hint">Recording does not copy or upgrade that installation.</p>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void runAction(() => api.useExisting(role, path.trim()).then(() => setPath("")));
            }}
          >
            <div className="field">
              <label htmlFor="existing-role">Runtime or tool</label>
              <select id="existing-role" className="input" value={role} onChange={(event) => setRole(event.target.value)}>
                <option value="node">Node.js</option>
                <option value="python">Python</option>
                <option value="pnpm">pnpm</option>
                <option value="uv">uv</option>
                <option value="apache">Apache</option>
                <option value="mysql">MySQL</option>
                <option value="postgres">PostgreSQL</option>
              </select>
            </div>
            <div className="field">
              <label htmlFor="existing-path">Executable path</label>
              <input id="existing-path" className="input" value={path} onChange={(event) => setPath(event.target.value)} placeholder="Path to the executable" />
            </div>
            <button type="submit" className="btn primary" disabled={!desktop || path.trim() === "" || busyAction}>
              Use existing
            </button>
          </form>
        </section>
      </div>
      <section className="block" aria-label="Updates">
        <h2>Updates</h2>
        <p className="hint">
          A runtime update installs beside the current one. Switching a project is separate, and the other projects keep their selection. Dependency updates stay on the project. Application updates are not downloaded here. News is not a package.
        </p>
        {desktop ? null : <p className="empty">Update notices appear when this window can reach the engine.</p>}
        <div className="row">
          <button
            type="button"
            className="btn"
            disabled={!desktop || busyAction}
            onClick={() => {
              void runAction(async () => setNotices(await api.updates()));
            }}
          >
            Check updates
          </button>
          <button
            type="button"
            className="btn"
            disabled={!desktop || busyAction}
            onClick={() => {
              void runAction(async () => setNotices(await api.cleanupUnused()));
            }}
          >
            Remove unused packages
          </button>
        </div>
        {actionError ? <p className="error" role="alert">{actionError}</p> : null}
        {notices.length === 0 ? <p className="empty">No update notice yet.</p> : null}
        <ul className="slips">
          {notices.map((notice) => (
            <li key={`${notice.kind}:${notice.subject}`} className="slip">
              <span className="slip-name">{notice.subject}</span>
              <span className="pill">{notice.kind}</span>
              <span className="slip-sub">{notice.detail}</span>
            </li>
          ))}
        </ul>
      </section>
      <section className="block">
        <h2>External connections</h2>
        <p className="hint">
          External databases are recorded on Databases. Detach removes the connection and leaves that server running.
        </p>
      </section>
    </>
  );
}
