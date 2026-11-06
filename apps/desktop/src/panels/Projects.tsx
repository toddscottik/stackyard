import { useRef, useState, type KeyboardEvent } from "react";
import { api, type Front, type Installed, type Project } from "../api";
import { recalled, remember } from "../preferences";
import { useAction } from "../useAction";
import { Logs } from "./Logs";
import { ProjectConfiguration, ProjectImport } from "./ProjectConfiguration";
import "./project-configuration.css";

type DependencyInput = {
  projectId: string;
  action: string;
  packageName: string;
  scope: string;
  scopeId?: string;
};

function projectState(project: Project) {
  const states = project.services.map((service) => service.state);
  for (const state of ["starting", "orphaned", "failed", "running", "invalid"]) {
    if (states.includes(state)) return state;
  }
  return project.services.length === 0 ? "empty" : "stopped";
}

function pill(state: string) {
  if (state === "running") return "pill pos";
  if (state === "starting") return "pill warn";
  if (["orphaned", "failed", "invalid"].includes(state)) return "pill neg";
  return "pill";
}

function lifecycleActions(project: Project) {
  const states = project.services.map((service) => service.state);
  if (states.includes("starting")) return [{ label: "Cancel start", invoke: api.cancelStart }];
  if (states.includes("orphaned")) return [{ label: "Stop", invoke: api.stopProject }];
  if (states.includes("running")) {
    return [{ label: "Stop", invoke: api.stopProject }, { label: "Restart", invoke: api.restartProject }];
  }
  if (states.length && !states.includes("invalid")) return [{ label: "Start", invoke: api.startProject }];
  return [];
}

type WorkspaceProps = {
  project: Project;
  packages: Installed[];
  front: Front | null;
  enabled: boolean;
  visible: boolean;
  onProject: (project: Project) => void;
  onFront: (front: Front) => void;
  refreshProjects: () => void;
  onDependency: (input: DependencyInput) => Promise<void>;
  dependencyBusy: boolean;
  onActivity: () => void;
};

type ProjectsProps = Omit<WorkspaceProps, "project" | "dependencyBusy"> & {
  projects: Project[];
  selected: Project | null;
  workspaceRequest: number;
  onSelect: (id: string) => void;
  onImport: (project: Project) => void;
  onForget: (id: string) => void;
  isDependencyBusy: (id: string) => boolean;
};

export function Projects({
  projects, selected, workspaceRequest, onSelect, onImport, onForget, isDependencyBusy, ...workspace
}: ProjectsProps) {
  const { states, run } = useAction();
  const [importOpen, setImportOpen] = useState(false);
  const { enabled, front, onFront } = workspace;
  const lifecycleKey = `lifecycle.${selected?.id ?? ""}`;
  const running = projects.reduce((count, project) =>
    count + project.services.filter((service) => service.state === "running").length, 0);
  const showImport = importOpen || projects.length === 0;
  return (
    <>
      <p className="finding">{projects.length
        ? `${running} ${running === 1 ? "service" : "services"} running`
        : "Import a project to manage its services."}</p>
      {showImport ? <div className="project-import-panel">
        {projects.length > 0 && <button type="button" className="btn" onClick={() => setImportOpen(false)}>Back to projects</button>}
        <ProjectImport enabled={enabled} onImport={(project) => {
          onImport(project);
          setImportOpen(false);
        }} />
      </div> : <div className={selected ? "split picked" : "split"}>
        <section className="pane list-pane" aria-label="Project list">
          <button type="button" className="btn" onClick={() => setImportOpen(true)}>Import project</button>
            <ul className="slips">
              {projects.map((project) => {
                const state = projectState(project);
                return <li key={project.id}>
                  <button type="button" className="slip" aria-current={project.id === selected?.id ? "true" : undefined}
                    onClick={() => onSelect(project.id)}>
                    <span className="slip-name">{project.name}</span>
                    <span className={pill(state)}>{state === "empty" ? "No services" : state}</span>
                    <span className="slip-sub">
                      <span>{project.services.length} {project.services.length === 1 ? "service" : "services"}</span>
                      <span>{project.manager || (project.scopes.length > 1 ? "Multiple scopes" : "No manager")}</span>
                    </span>
                  </button>
                </li>;
              })}
            </ul>
        </section>
        <section className="pane detail-pane" aria-labelledby="detail-heading">
          <button type="button" className="btn back" onClick={() => onSelect("")}>All projects</button>
          {selected ? <Workspace key={`${selected.id}:${workspaceRequest}`} project={selected} onForget={onForget} {...workspace}
            dependencyBusy={isDependencyBusy(selected.id)} lifecycle={states[lifecycleKey]}
            onLifecycle={(invoke) => run(lifecycleKey, async () => {
              try { workspace.onProject(await invoke(selected.id)); }
              finally { workspace.refreshProjects(); }
            })} /> : (
            <><h2 id="detail-heading">No project selected</h2><p className="empty">Choose a project to see its services.</p></>
          )}
        </section>
      </div>}
      {projects.length === 0 ? (
        <section className="block" aria-label="Manifest example">
          <h2>The manifest</h2>
          <p className="hint">Import can review an existing stackyard.toml or prepare one from the selected folder.</p>
          <pre className="output">{MANIFEST_EXAMPLE}</pre>
        </section>
      ) : null}
      <section className="block" aria-label="Apache">
        <h2>Apache</h2>
        {front ? (
          <>
            <p className="hint">Apache is {front.state}. HTTP <span className="mono">{front.httpPort}</span>, HTTPS{" "}
              <span className="mono">{front.httpsPort}</span>. {front.restartImpact}</p>
            {!front.resolvesLocalhost ? <p className="hint">This machine needs a hosts entry to resolve .localhost sites.</p> : null}
            {front.sites.length ? <button type="button" className="btn" disabled={!enabled || states.apache?.pending}
              onClick={() => void run("apache", async () => onFront(await api.restartFront()))}>Restart Apache</button> : null}
            {states.apache?.error ? <p role="alert" className="action-error">{states.apache.error}</p> : null}
          </>
        ) : <p className="hint">Apache status appears when the engine is reachable.</p>}
      </section>
    </>
  );
}

const MANIFEST_EXAMPLE = `schema = 1
name = "site"

[[service]]
id = "web"
runtime = "node"
command = ["node", "server.js"]
port = 0
ready_path = "/"

[site]
service = "web"
host = "site.localhost"`;

const tabs = ["Overview", "Dependencies", "Logs", "Site", "Configuration"] as const;
type Tab = typeof tabs[number];

function Workspace({ project, packages, front, enabled, visible, onFront, onProject, refreshProjects,
  onDependency, dependencyBusy, onActivity, lifecycle, onLifecycle, onForget }: WorkspaceProps & {
  onForget: (id: string) => void;
  lifecycle?: { pending: boolean; error: string };
  onLifecycle: (invoke: (id: string) => Promise<Project>) => Promise<boolean>;
}) {
  const tabKey = `project-tab.${project.id}`;
  const [tab, setTab] = useState<Tab>(() => tabs.find((item) => item === recalled(tabKey)) ?? "Overview");
  const tabRefs = useRef<Partial<Record<Tab, HTMLButtonElement | null>>>({});
  function select(next: Tab) {
    setTab(next);
    remember(tabKey, next);
  }
  function onTabKey(event: KeyboardEvent, index: number) {
    const next = event.key === "Home" ? tabs[0]
      : event.key === "End" ? tabs[tabs.length - 1]
      : event.key === "ArrowRight" ? tabs[(index + 1) % tabs.length]
      : event.key === "ArrowLeft" ? tabs[(index + tabs.length - 1) % tabs.length] : undefined;
    if (!next) return;
    event.preventDefault();
    select(next);
    tabRefs.current[next]?.focus();
  }
  return (
    <>
      <h2 id="detail-heading">{project.name}</h2>
      <p className="path mono">{project.root}</p>
      <div className="row lifecycle">
        {lifecycleActions(project).map(({ label, invoke }, index) => (
          <button key={label} type="button" className={index === 0 ? "btn primary" : "btn"}
            disabled={!enabled || lifecycle?.pending}
            onClick={() => void onLifecycle(invoke)}>
            {label}
          </button>
        ))}
        {lifecycle?.pending ? <span role="status" className="hint">Updating project…</span> : null}
      </div>
      {lifecycle?.error ? <p role="alert" className="action-error">{lifecycle.error}</p> : null}
      <div className="workspace-tabs" role="tablist" aria-label="Project workspace">
        {tabs.map((item, index) => <button key={item} id={`tab-${item}`} type="button" role="tab" aria-selected={tab === item}
          aria-controls={`workspace-${item}`} tabIndex={tab === item ? 0 : -1}
          ref={(element) => { tabRefs.current[item] = element; }}
          onClick={() => select(item)} onKeyDown={(event) => onTabKey(event, index)}>{item}</button>)}
      </div>
      {tabs.map((item) => (
        <div key={item} id={`workspace-${item}`} role="tabpanel" aria-labelledby={`tab-${item}`} hidden={tab !== item}>
          {tab === item && item === "Overview" ? <Overview project={project} packages={packages}
            enabled={enabled} refreshProjects={refreshProjects} /> : null}
          {tab === item && item === "Dependencies" ? <Dependencies project={project} enabled={enabled}
            busy={dependencyBusy} start={onDependency} onActivity={onActivity} /> : null}
          {tab === item && item === "Logs" ? <Logs projectId={project.id} services={project.services} enabled={enabled && visible} /> : null}
          {tab === item && item === "Site" ? <Site projectId={project.id} front={front} enabled={enabled} onFront={onFront} /> : null}
          {tab === item && item === "Configuration" ? <ProjectConfiguration project={project} enabled={enabled}
            onProject={onProject} onForget={() => onForget(project.id)} /> : null}
        </div>
      ))}
    </>
  );
}

function Overview({ project, packages, enabled, refreshProjects }: Pick<WorkspaceProps, "project" | "packages" | "enabled" | "refreshProjects">) {
  const [serviceId, setServiceId] = useState("");
  const { states, run } = useAction();
  const service = project.services.find((item) => item.id === serviceId) ?? project.services[0];
  const roles = [...new Set(project.services.map((item) => item.runtime).filter(Boolean))];
  return (
    <>
      <div className="row">
        {["folder", "editor", "shell", "url"].map((action) => (
          <button key={action} type="button" className="btn" disabled={!enabled || states.open?.pending ||
            (action === "url" && (service?.endpoint?.protocol !== "http" || !service.endpoint.resolvedPort))}
            onClick={() => void run("open", () => api.openProject(project.id, action, service?.id ?? ""))}>
            Open {action === "url" ? "URL" : action}
          </button>
        ))}
      </div>
      {states.open?.error ? <p role="alert" className="action-error">{states.open.error}</p> : null}
      <div className="block">
        <h3>Services</h3>
        {project.services.length === 0 ? <p className="empty">This project has no services.</p> : null}
        <ul className="slips">
          {project.services.map((item) => (
            <li key={item.id}>
              <button type="button" className="slip" aria-current={item.id === service?.id ? "true" : undefined}
                onClick={() => setServiceId(item.id)}>
                <span className="slip-name">{item.id}</span><span className={pill(item.state)}>{item.state}</span>
                <span className="slip-sub">
                  <span>{item.runtime || "service"}</span>
                  <span className="mono">{item.port ? `127.0.0.1:${item.port}` : "No port"}</span>
                  {item.pid ? <span className="mono">PID {item.pid}</span> : null}
                </span>
              </button>
              {item.detail ? <p className="detail-line">{item.detail}</p> : null}
            </li>
          ))}
        </ul>
      </div>
      <div className="block">
        <h3>Runtime</h3>
        {roles.map((role) => {
          const choices = packages.filter((item) => item.name === role);
          const selected = project.selections.find((item) => item.role === role);
          return <div key={role}>
            <label className="runtime">{role}
              <select className="input" aria-label={`${role} runtime`} value={selected?.packageId ?? ""}
                disabled={!enabled || choices.length === 0 || states[role]?.pending}
                onChange={(event) => {
                  const id = event.target.value;
                  if (id) void run(role, async () => { await api.selectPackage(project.id, id); refreshProjects(); });
                }}>
                <option value="">{choices.length ? `Choose ${role}` : `No ${role} recorded`}</option>
                {choices.map((item) => <option key={item.id} value={item.id}>{item.version} · {item.source}</option>)}
              </select>
              {selected ? <span className="meta">Using {selected.version} ({selected.source})</span> : null}
            </label>
            {states[role]?.error ? <p role="alert" className="action-error">{states[role].error}</p> : null}
          </div>;
        })}
      </div>
    </>
  );
}

function Dependencies({ project, enabled, busy, start, onActivity }: {
  project: Project; enabled: boolean; busy: boolean; start: WorkspaceProps["onDependency"]; onActivity: () => void;
}) {
  const [scopeId, setScopeId] = useState(project.scopes.length > 1 ? project.scopes[0].id : "");
  const [directory, setDirectory] = useState("");
  const [packageName, setPackageName] = useState("");
  const { states, run } = useAction();
  const scope = project.scopes.find((item) => item.id === scopeId);
  const actions = [
    ["install", "Install dependencies"], ["add", "Add package"], ["remove", "Remove package"],
    ["update", "Update selected package"], ["list", "List installed"],
  ];
  return (
    <section aria-label="Dependencies">
      <p className="hint">{scope?.managerDetail || project.managerDetail || "Choose a dependency scope or workspace directory."}</p>
      {project.scopes.length ? <div className="field">
        <label htmlFor="scope-choice">Dependency scope</label>
        <select id="scope-choice" className="input" value={scope?.id ?? ""} onChange={(event) => setScopeId(event.target.value)}>
          <option value="">Workspace directory</option>
          {project.scopes.map((item) => <option key={item.id} value={item.id}>{item.id} · {item.manager || item.runtime}</option>)}
        </select>
      </div> : null}
      {!scope ? <div className="field">
        <label htmlFor="dep-scope">Workspace directory</label>
        <input id="dep-scope" className="input" value={directory} onChange={(event) => setDirectory(event.target.value)}
          placeholder="Empty uses the project root" />
      </div> : null}
      <div className="field">
        <label htmlFor="dep-package">Package</label>
        <input id="dep-package" className="input" value={packageName} onChange={(event) => setPackageName(event.target.value)}
          placeholder="Required for add, remove, and update" />
      </div>
      <div className="row">
        {actions.map(([action, label]) => <button key={action} type="button" className="btn"
          disabled={!enabled || busy || states.dependency?.pending ||
            (!["list", "install"].includes(action) && !packageName.trim())}
          onClick={() => void run("dependency", () => start({
            projectId: project.id, action, packageName: packageName.trim(), scope: directory.trim(), scopeId: scope?.id,
          }))}>{label}</button>)}
        <button type="button" className="btn ghost" onClick={onActivity}>View activity</button>
      </div>
      {busy ? <p role="status" className="hint">Dependency operation in progress.</p> : null}
      {states.dependency?.error ? <p role="alert" className="action-error">{states.dependency.error}</p> : null}
    </section>
  );
}

function Site({ projectId, front, enabled, onFront }: { projectId: string; front: Front | null; enabled: boolean; onFront: WorkspaceProps["onFront"] }) {
  const site = front?.sites.find((item) => item.projectId === projectId);
  const { states, run } = useAction();
  if (!front) return <p className="empty">Site controls appear when Apache status is available.</p>;
  return (
    <section aria-label="Site">
      {site ? (
        <>
          <p className="hint"><span className="mono">{site.host}</span> · {site.mode} · {site.upstream}</p>
          <p className="path mono">{site.httpsUrl}</p>
          <div className="row">
            <button type="button" className="btn" disabled={!enabled || states.site?.pending}
              onClick={() => void run("site", () => api.openSite(site.httpsUrl))}>Open site</button>
            <button type="button" className="btn" disabled={!enabled || states.site?.pending}
              onClick={() => void run("site", async () => onFront(await api.unpublish(projectId)))}>Unpublish</button>
          </div>
        </>
      ) : (
        <>
          <p className="hint">Publish uses the [site] table in stackyard.toml.</p>
          <button type="button" className="btn" disabled={!enabled || states.site?.pending}
            onClick={() => void run("site", async () => onFront(await api.publish(projectId)))}>Publish</button>
        </>
      )}
      {states.site?.error ? <p role="alert" className="action-error">{states.site.error}</p> : null}
    </section>
  );
}
