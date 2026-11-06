import { useEffect, useRef, useState } from "react";
import { api, errorText, type ConfigDiagnostic, type ConfigDraft, type ConfigForm, type ConfigInspection, type ConfigReview, type Project } from "../api";

type Props = { project: Project; enabled: boolean; onProject: (project: Project) => void; onForget: () => void };

export function ProjectImport({ enabled, onImport }: { enabled: boolean; onImport: (project: Project) => void }) {
  const [root, setRoot] = useState("");
  const [inspection, setInspection] = useState<ConfigInspection | null>(null);
  const [form, setForm] = useState<ConfigForm | null>(null);
  const [review, setReview] = useState<ConfigReview | null>(null);
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const sequence = useRef(0);
  function invalidate() { sequence.current++; setReview(null); setBusy(""); }
  async function inspect(path = root) {
    if (!path.trim() || !enabled) return;
    invalidate(); const current = ++sequence.current; setBusy("inspect"); setError("");
    try { const result = await api.inspectConfig(path.trim()); if (current === sequence.current) { setInspection(result); setForm(result.form); setRoot(result.root); } }
    catch (err) { if (current === sequence.current) setError(errorText(err)); }
    finally { if (current === sequence.current) setBusy(""); }
  }
  async function browse() {
    setBusy("browse"); setError("");
    try { const path = await api.pickProjectParent(); if (path) { setRoot(path); setInspection(null); setForm(null); invalidate(); await inspect(path); } }
    catch (err) { setError(errorText(err)); }
    finally { setBusy(""); }
  }
  function draft(): ConfigDraft | null { return inspection ? { root: inspection.root, revision: inspection.revision, form } : null; }
  async function preview() {
    const value = draft(); if (!value || !enabled) return;
    const current = ++sequence.current; setBusy("preview"); setError(""); setReview(null);
    try { const result = await api.previewConfig(value); if (current === sequence.current) setReview(result); }
    catch (err) { if (current === sequence.current) setError(errorText(err)); }
    finally { if (current === sequence.current) setBusy(""); }
  }
  async function save() {
    const value = draft(); if (!value || !review || !enabled) return;
    const current = ++sequence.current; setBusy("save"); setError("");
    try {
      const result = await api.applyConfig(value, review.fingerprint);
      if (current !== sequence.current) return;
      setReview(null); setRoot(""); setInspection(null); setForm(null); onImport(result);
    }
    catch (err) { if (current === sequence.current) setError(errorText(err)); }
    finally { if (current === sequence.current) setBusy(""); }
  }
  const reviewOnly = inspection?.existing && !inspection.project_id && !inspection.form;
  return <section className="config-card" aria-label="Import project">
    <h3>Import a project</h3><p className="hint">Choose a folder. Stackyard inspects its files and shows the evidence behind each suggestion.</p>
    <div className="config-path"><label className="sr-only" htmlFor="import-root">Project folder</label>
      <input id="import-root" className="input" value={root} disabled={busy !== ""} onChange={e => { setRoot(e.target.value); setInspection(null); setForm(null); invalidate(); }} placeholder="Path to project folder" />
      <button className="btn" type="button" disabled={!enabled || busy !== ""} onClick={() => void browse()}>Browse…</button>
      <button className="btn" type="button" disabled={!enabled || !root.trim() || busy !== ""} onClick={() => void inspect()}>Inspect folder</button>
    </div>
    {inspection && <>
      <p className="path mono">{inspection.root}</p>
      <Evidence items={inspection.evidence} />
      <Diagnostics items={inspection.diagnostics} />
      {reviewOnly ? <p className="hint">This folder has a legacy manifest. Review and import it without editing advanced manifest fields here.</p> :
        form ? <fieldset disabled={busy !== ""} className="config-controls"><ConfigEditor form={form} registered={Boolean(inspection.project_id)} original={inspection.form} onChange={next => { setForm(next); invalidate(); }} /></fieldset> :
          <p className="hint">No safe service suggestion is available. Open this folder in an editor to configure it manually.</p>}
      {form || reviewOnly ? <>
        <div className="row"><button className="btn primary" type="button" disabled={!enabled || busy !== ""} onClick={() => void preview()}>{busy === "preview" ? "Preparing…" : "Preview changes"}</button></div>
        {review && <Review review={review} onSave={save} saving={busy === "save"} enabled={enabled} label="Save and import" />}
      </> : null}
    </>}
    {busy === "inspect" || busy === "browse" ? <p role="status" className="hint">{busy === "inspect" ? "Inspecting folder…" : "Opening folder picker…"}</p> : null}
    {error && <p role="alert" className="action-error">{error}<button className="btn" type="button" onClick={() => void (inspection ? preview() : inspect())}>Retry</button></p>}
  </section>;
}

export function ProjectConfiguration({ project, enabled, onProject, onForget }: Props) {
  const [inspection, setInspection] = useState<ConfigInspection | null>(null);
  const [form, setForm] = useState<ConfigForm | null>(null);
  const [review, setReview] = useState<ConfigReview | null>(null);
  const [busy, setBusy] = useState(""); const [error, setError] = useState(""); const [confirm, setConfirm] = useState(false);
  const sequence = useRef(0);
  const canForget = !project.services.some(s => ["starting", "running", "orphaned"].includes(s.state) || s.pid !== null);
  function invalidate() { sequence.current++; setReview(null); setBusy(""); }
  async function reload() {
    if (!enabled) return;
    const current = ++sequence.current; setBusy("reload"); setError(""); setReview(null);
    try {
      const fresh = await api.reloadProject(project.id);
      if (current !== sequence.current) return;
      onProject(fresh);
      const inspected = await api.inspectConfig(project.root);
      if (current === sequence.current) { setInspection(inspected); setForm(inspected.form); }
    }
    catch (err) { if (current === sequence.current) setError(errorText(err)); }
    finally { if (current === sequence.current) setBusy(""); }
  }
  async function inspectOnly() {
    if (!enabled) return;
    const current = ++sequence.current; setBusy("inspect"); setError("");
    try { const inspected = await api.inspectConfig(project.root); if (current === sequence.current) { setInspection(inspected); setForm(inspected.form); } }
    catch (err) { if (current === sequence.current) setError(errorText(err)); }
    finally { if (current === sequence.current) setBusy(""); }
  }
  useEffect(() => { void inspectOnly(); return () => { sequence.current++; }; }, [project.id, enabled]);
  async function preview() {
    if (!inspection || !form || !enabled) return;
    const current = ++sequence.current; setBusy("preview"); setReview(null); setError("");
    try { const result = await api.previewConfig({ root: inspection.root, revision: inspection.revision, form }); if (current === sequence.current) setReview(result); }
    catch (err) { if (current === sequence.current) setError(errorText(err)); }
    finally { if (current === sequence.current) setBusy(""); }
  }
  async function save() {
    if (!inspection || !form || !review || !enabled) return;
    const current = ++sequence.current; setBusy("save"); setError("");
    try {
      const result = await api.applyConfig({ root: inspection.root, revision: inspection.revision, form }, review.fingerprint);
      if (current !== sequence.current) return;
      onProject(result); setReview(null);
      const refreshed = await api.inspectConfig(project.root);
      if (current === sequence.current) { setInspection(refreshed); setForm(refreshed.form); }
    }
    catch (err) { if (current === sequence.current) setError(errorText(err)); }
    finally { if (current === sequence.current) setBusy(""); }
  }
  async function forget() {
    if (!canForget || !confirm || !enabled) return;
    setBusy("forget"); setError("");
    try { await api.forgetProject(project.id); onForget(); }
    catch (err) { setError(errorText(err)); setConfirm(false); }
    finally { setBusy(""); }
  }
  async function openEditor() {
    setBusy("editor"); setError("");
    try { await api.openProject(project.id, "editor", ""); }
    catch (err) { setError(errorText(err)); }
    finally { setBusy(""); }
  }
  return <section className="config-card" aria-label="Project configuration">
    <h3>Configuration</h3>
    <div className="row"><button type="button" className="btn" disabled={!enabled || busy !== ""} onClick={() => void openEditor()}>Open in editor</button>
      <button type="button" className="btn" disabled={!enabled || busy !== ""} onClick={() => void reload()}>Reload configuration</button></div>
    {inspection?.form && form ? <><Evidence items={inspection.evidence} /><Diagnostics items={inspection.diagnostics} />
      <fieldset disabled={busy !== ""} className="config-controls"><ConfigEditor form={form} registered original={inspection.form} onChange={next => { setForm(next); invalidate(); }} /></fieldset>
      <button type="button" className="btn primary" disabled={!enabled || busy !== ""} onClick={() => void preview()}>Preview changes</button>
      {review && <Review review={review} enabled={enabled} saving={busy === "save"} onSave={save} label="Save configuration" />}
    </> : inspection && <p className="hint">This project has legacy or unsupported configuration. Open it in your editor; reload after saving.</p>}
    {busy && <p role="status" className="hint">{busy === "reload" ? "Reloading configuration…" : busy === "save" ? "Saving configuration…" : "Forgetting project…"}</p>}
    {error && <p role="alert" className="action-error">{error}<button type="button" className="btn" onClick={() => void reload()}>Retry</button></p>}
    <div className="config-danger"><h4>Forget project</h4><p className="hint">Detaches this project from Stackyard. Source files, secrets, and database data remain on disk.</p>
      {!canForget && <p className="hint">Stop all project services before forgetting it.</p>}
      {!confirm ? <button type="button" className="btn danger" disabled={!enabled || busy !== "" || !canForget} onClick={() => setConfirm(true)}>Forget project…</button> :
        <div className="row"><span role="alert">Forget {project.name}? Files and database data will be preserved.</span>
          <button type="button" className="btn danger" disabled={!enabled || busy !== "" || !canForget} onClick={() => void forget()}>Confirm forget</button>
          <button type="button" className="btn" disabled={busy !== ""} onClick={() => setConfirm(false)}>Cancel</button></div>}
    </div>
  </section>;
}

function nextId(prefix: string, rows: { id: string }[]) {
  let index = 1;
  while (rows.some((row) => row.id === `${prefix}-${index}`)) index++;
  return `${prefix}-${index}`;
}

function ConfigEditor({ form, registered, original, onChange }: { form: ConfigForm; registered: boolean; original: ConfigForm | null; onChange: (form: ConfigForm) => void }) {
  const patch = (key: keyof ConfigForm, value: unknown) => onChange({ ...form, [key]: value } as ConfigForm);
  return <div className="config-form">
    <label className="field">Project name<input className="input" value={form.name} onChange={e => patch("name", e.target.value)} /></label>
    <h4>Scopes</h4>{form.scopes.map((scope, i) => <fieldset className="config-item" key={scope.id}><legend>{scope.id}{registered ? " · existing ID" : ""}</legend>
      <div className="config-grid"><Field label="Directory" value={scope.directory} onChange={v => patch("scopes", form.scopes.map((x, n) => n === i ? { ...x, directory: v } : x))} />
        <Field label="Runtime" value={scope.runtime} disabled={Boolean(original?.scopes.some(x => x.id === scope.id))} onChange={v => patch("scopes", form.scopes.map((x, n) => n === i ? { ...x, runtime: v } : x))} />
        <Field label="Manager" value={scope.manager} onChange={v => patch("scopes", form.scopes.map((x, n) => n === i ? { ...x, manager: v } : x))} />
        <Field label="Environment" value={scope.environment} onChange={v => patch("scopes", form.scopes.map((x, n) => n === i ? { ...x, environment: v } : x))} />
        <Field label="Lockfile root" value={scope.lockfile_root} onChange={v => patch("scopes", form.scopes.map((x, n) => n === i ? { ...x, lockfile_root: v } : x))} />
      </div>{!registered || !original?.scopes.some(x => x.id === scope.id) ? <button type="button" className="btn danger" onClick={() => patch("scopes", form.scopes.filter((_, n) => n !== i))}>Remove scope</button> : null}</fieldset>)}
    <button type="button" className="btn" onClick={() => patch("scopes", [...form.scopes, { id: nextId("scope", form.scopes), directory: ".", runtime: "", manager: "", lockfile_root: "", environment: "" }])}>Add scope</button>
    <h4>Services</h4>{form.services.map((service, i) => <fieldset className="config-item" key={service.id}><legend>{service.id}{registered ? " · existing ID" : ""}</legend>
      <div className="config-grid"><Field label="Scope" value={service.scope} onChange={v => patch("services", form.services.map((x, n) => n === i ? { ...x, scope: v } : x))} />
        <Field label="Working directory" value={service.working_directory} onChange={v => patch("services", form.services.map((x, n) => n === i ? { ...x, working_directory: v } : x))} />
        <Field label="Protocol (empty = process only)" value={service.protocol} onChange={v => patch("services", form.services.map((x, n) => n === i ? { ...x, protocol: v } : x))} />
        <Field label="Port (auto or number)" value={service.port} onChange={v => patch("services", form.services.map((x, n) => n === i ? { ...x, port: v } : x))} />
      </div><label className="field">Command arguments (one argument per line)<textarea className="input" rows={4} value={service.command.join("\n")} onChange={e => patch("services", form.services.map((x, n) => n === i ? { ...x, command: e.target.value.split("\n") } : x))} /></label>
      {registered && <p className="hint">Existing service identifiers are retained.</p>}
      {!registered || !original?.services.some(x => x.id === service.id) ? <button type="button" className="btn danger" onClick={() => patch("services", form.services.filter((_, n) => n !== i))}>Remove service</button> : null}</fieldset>)}
    <button type="button" className="btn" onClick={() => patch("services", [...form.services, { id: nextId("service", form.services), scope: form.scopes[0]?.id ?? "", command: [], working_directory: ".", protocol: "", port: "auto" }])}>Add service</button>
    <h4>Site</h4>{form.site ? <div className="config-grid"><Field label="Host" value={form.site.host} onChange={v => patch("site", { ...form.site!, host: v })} />
      <Field label="Handler (proxy, static, php)" value={form.site.handler} onChange={v => patch("site", { ...form.site!, handler: v })} />
      <Field label="Service" value={form.site.service} onChange={v => patch("site", { ...form.site!, service: v })} />
      <Field label="Document root" value={form.site.root} onChange={v => patch("site", { ...form.site!, root: v })} />
      <button type="button" className="btn danger" onClick={() => patch("site", null)}>Remove site</button></div> : <button type="button" className="btn" onClick={() => patch("site", { host: "", handler: "proxy", service: form.services[0]?.id ?? "", root: "" })}>Add site</button>}
    <p className="hint">Advanced manifest fields are available in the editor. Commands stay as argument arrays; no shell splitting is used.</p>
  </div>;
}
function Field({ label, value, disabled, onChange }: { label: string; value: string; disabled?: boolean; onChange: (value: string) => void }) {
  return <label className="field">{label}<input className="input" value={value} disabled={disabled} onChange={e => onChange(e.target.value)} /></label>;
}
function Evidence({ items }: { items: ConfigInspection["evidence"] }) {
  return <section className="config-evidence" aria-label="Detection evidence"><h4>Detection evidence</h4>{items.length ? <ul>{items.map((item, i) => <li key={`${item.path}-${i}`}><span className="mono">{item.path}</span> · {item.detail} <span className="meta">({item.confidence} confidence)</span></li>)}</ul> : <p className="hint">No detection evidence was found.</p>}</section>;
}
function Diagnostics({ items }: { items: ConfigDiagnostic[] }) {
  return items.length ? <ul className="config-diagnostics">{items.map((item, i) => <li key={`${item.code}-${i}`} className={item.severity === "error" ? "negative" : ""}>{item.message}</li>)}</ul> : null;
}
function Review({ review, enabled, saving, onSave, label }: { review: ConfigReview; enabled: boolean; saving: boolean; onSave: () => void; label: string }) {
  return <section className="config-review"><h4>Preview</h4><Diagnostics items={review.preview.diagnostics} />
    <pre className="output">{review.manifest_text}</pre>
    <button type="button" className="btn primary" disabled={!enabled || saving || !review.preview.can_apply} onClick={onSave}>{saving ? "Saving…" : label}</button>
  </section>;
}
