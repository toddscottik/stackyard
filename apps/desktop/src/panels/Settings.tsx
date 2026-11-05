import { useEffect, useState } from "react";
import { api } from "../api";
import { useAction } from "../useAction";
import { useResource } from "../useResource";

async function loadSettings() {
  const [name, editor, background] = await Promise.all([
    api.setting("display_name"),
    api.setting("editor_command"),
    api.setting("background_mode"),
  ]);
  return { name: name ?? "", editor: editor || "code", background: background === "on" };
}

export function Settings({
  desktop,
  engineId,
}: {
  desktop: boolean;
  engineId: number | null;
}) {
  const resource = useResource(loadSettings, { enabled: desktop, identity: engineId });
  const { states, run } = useAction();
  const [name, setName] = useState("");
  const [editor, setEditor] = useState("code");
  const [background, setBackground] = useState(false);
  const [saved, setSaved] = useState(false);
  const [report, setReport] = useState("");
  const settings = resource.value;
  const editable = desktop && settings !== undefined && !resource.loading && !states.save?.pending;

  useEffect(() => {
    if (!settings) return;
    setName(settings.name);
    setEditor(settings.editor);
    setBackground(settings.background);
  }, [settings]);

  return (
    <>
      <p className="finding">
        {background ? "Quit leaves managed projects running." : "Quit stops managed projects."}
      </p>
      {resource.loading ? <p role="status" className="hint">Loading settings…</p> : null}
      {resource.error ? <div className="notice" role="alert">
        <span>{resource.error}</span>
        <button type="button" className="btn" disabled={!desktop || resource.loading} onClick={resource.refresh}>Retry settings</button>
      </div> : null}
      <form
        onSubmit={(event) => {
          event.preventDefault();
          if (!editable) return;
          setSaved(false);
          const next = { name, editor: editor.trim() || "code", background };
          void run("save", async () => {
            await Promise.all([
              api.putSetting("display_name", next.name),
              api.putSetting("editor_command", next.editor),
              api.putSetting("background_mode", next.background ? "on" : "off"),
            ]);
            resource.update(() => next);
            setSaved(true);
          });
        }}
      >
        <div className="settings-row">
          <div>
            <h2>Display name</h2>
            <p className="hint">The name this machine shows for the Stackyard window.</p>
          </div>
          <div className="settings-panel">
            <div className="field">
              <label htmlFor="display-name">Name</label>
              <input id="display-name" className="input" value={name} disabled={!editable} onChange={(event) => setName(event.target.value)} />
            </div>
          </div>
        </div>
        <div className="settings-row">
          <div>
            <h2>Editor</h2>
            <p className="hint">Open editor runs this program with the project folder. It is not a shell command.</p>
          </div>
          <div className="settings-panel">
            <div className="field">
              <label htmlFor="editor-command">Command</label>
              <input id="editor-command" className="input" value={editor} disabled={!editable} onChange={(event) => setEditor(event.target.value)} />
            </div>
          </div>
        </div>
        <div className="settings-row">
          <div>
            <h2>Quit</h2>
            <p className="hint">Closing the window hides Stackyard in the tray. Quit stops managed projects unless this is on.</p>
          </div>
          <div className="settings-panel">
            <label className="switch-row">
              <span className="switch-title">Leave projects running when quitting</span>
              <input
                type="checkbox"
                className="switch"
                checked={background}
                disabled={!editable}
                onChange={(event) => setBackground(event.target.checked)}
              />
            </label>
          </div>
        </div>
        <div className="row">
          <button type="submit" className="btn primary" disabled={!editable}>
            Save
          </button>
          {saved ? <span className="hint">Saved</span> : null}
          {states.save?.error ? <span className="action-error" role="alert">{states.save.error}</span> : null}
        </div>
      </form>
      <section className="block" aria-label="Diagnostics">
        <h2>Diagnostics</h2>
        <p className="hint">
          The report lists projects, packages, and databases. Passwords, private keys, and setting values are left out. It does not change a running project.
        </p>
        <button
          type="button"
          className="btn"
          disabled={!desktop || states.diagnostics?.pending}
          onClick={() => void run("diagnostics", async () => setReport(await api.diagnostics()))}
        >
          Create diagnostics report
        </button>
        {states.diagnostics?.error ? <p className="action-error" role="alert">{states.diagnostics.error}</p> : null}
        {report ? (
          <pre className="report mono" aria-label="Diagnostics report">
            {report}
          </pre>
        ) : (
          <p className="empty">No report yet.</p>
        )}
      </section>
    </>
  );
}
