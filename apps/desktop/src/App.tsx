import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

type Status = {
  pid: number;
  dataDir: string;
  apiVersion: number;
  schemaVersion: number;
  lastEvent: string | null;
};

export function App() {
  const [status, setStatus] = useState<Status | null>(null);
  const [setting, setSetting] = useState("");
  const [error, setError] = useState("");

  async function refresh() {
    const next = await invoke<Status>("engine_status");
    const value = await invoke<string | null>("engine_setting", {
      key: "display_name",
    });
    setStatus(next);
    setSetting(value ?? "");
    setError("");
  }

  useEffect(() => {
    refresh().catch((err: unknown) => {
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
      <h1>Local Dev</h1>
      <p>Placeholder name. This window and the CLI use the same engine.</p>
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
    </main>
  );
}
