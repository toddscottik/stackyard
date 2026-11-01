import { useCallback, useEffect, useRef, useState } from "react";
import {
  api,
  engineAvailable,
  errorText,
  type CatalogItem,
  type Installed,
  type Databases,
  type Front,
  type Operation,
  type Project,
  type Service,
  type Status,
  type Task,
} from "./api";

type Section = "projects" | "components" | "databases" | "settings";

const headings: Record<Section, string> = {
  projects: "Projects",
  components: "Components",
  databases: "Databases",
  settings: "Settings",
};

export function App() {
  const [section, setSection] = useState<Section>("projects");
  const [menuOpen, setMenuOpen] = useState(false);
  const [status, setStatus] = useState<Status | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [packages, setPackages] = useState<Installed[]>([]);
  const [catalog, setCatalog] = useState<CatalogItem[]>([]);
  const [front, setFront] = useState<Front | null>(null);
  const [databases, setDatabases] = useState<Databases | null>(null);
  const [error, setError] = useState("");
  const [progress, setProgress] = useState("");
  const [desktop, setDesktop] = useState(true);
  const alertRef = useRef<HTMLParagraphElement>(null);

  const report = useCallback((err: unknown) => {
    setError(errorText(err));
  }, []);

  const refresh = useCallback(async () => {
    if (!engineAvailable()) {
      setDesktop(false);
      setError("Open the Stackyard desktop window to reach the engine.");
      return;
    }
    const [nextStatus, nextProjects, nextPackages, nextCatalog, nextFront, nextDatabases] = await Promise.all([
      api.status(),
      api.projects(),
      api.packages(),
      api.catalog(),
      api.front(),
      api.databases(),
    ]);
    setStatus(nextStatus);
    setProjects(nextProjects);
    setPackages(nextPackages);
    setCatalog(nextCatalog);
    setFront(nextFront);
    setDatabases(nextDatabases);
    setSelectedId((current) => {
      if (current === null) {
        return nextProjects[0]?.id ?? "";
      }
      if (current && !nextProjects.some((project) => project.id === current)) {
        return nextProjects[0]?.id ?? "";
      }
      return current;
    });
  }, []);

  useEffect(() => {
    refresh().catch(report);
  }, [refresh, report]);

  useEffect(() => {
    if (error) {
      alertRef.current?.focus();
    }
  }, [error]);

  const selected = projects.find((project) => project.id === selectedId) ?? null;
  const headerJob = section === "projects" && selected ? projectJob(selected) : null;

  async function runHeaderJob() {
    if (!headerJob || !selected) {
      return;
    }
    setProgress(headerJob.progress);
    try {
      const project = await headerJob.run(selected.id);
      setProjects((current) => {
        const rest = current.filter((item) => item.id !== project.id);
        return [...rest, project].sort((a, b) => a.name.localeCompare(b.name));
      });
      setSelectedId(project.id);
      setProgress("");
    } catch (err) {
      setProgress("");
      report(err);
    }
  }

  function openSection(next: Section) {
    setSection(next);
    setMenuOpen(false);
  }

  return (
    <div className="app">
      <nav id="section-nav" className={menuOpen ? "sidebar open" : "sidebar"} aria-label="Sections">
        <p className="brand">
          <Mark />
          Stackyard
        </p>
        <p className="scope">This machine</p>
        <SectionButton current={section} id="projects" onSelect={openSection} label="Projects" />
        <SectionButton current={section} id="components" onSelect={openSection} label="Components" />
        <SectionButton current={section} id="databases" onSelect={openSection} label="Databases" />
        <SectionButton current={section} id="settings" onSelect={openSection} label="Settings" />
        <button type="button" className="btn menu-close" onClick={() => setMenuOpen(false)}>
          Close
        </button>
        <p className="bottom">
          {status ? (
            <>
              Engine <span className="mono">{status.pid}</span>
            </>
          ) : (
            "Engine unreachable"
          )}
        </p>
      </nav>
      <header className="topbar">
        <button
          type="button"
          className="btn menu-toggle"
          aria-expanded={menuOpen}
          aria-controls="section-nav"
          onClick={() => setMenuOpen((open) => !open)}
        >
          Menu
        </button>
        <h1>{headings[section]}</h1>
        <div className="top-actions">
          {headerJob ? (
            <button type="button" className="btn primary" disabled={!desktop} onClick={() => void runHeaderJob()}>
              {headerJob.label}
            </button>
          ) : null}
        </div>
      </header>
      <div className="sheet">
        <main id="main" aria-busy={progress !== ""}>
          {error ? (
            <p role="alert" tabIndex={-1} ref={alertRef} className="notice">
              <span>{error}</span>
              <button type="button" className="btn ghost" onClick={() => setError("")}>
                Dismiss
              </button>
            </p>
          ) : null}
          {progress ? (
            <p role="status" className="progress">
              {progress}
            </p>
          ) : null}
          {section === "projects" ? (
            <Projects
              projects={projects}
              selected={selected}
              packages={packages}
              front={front}
              desktop={desktop}
              onFront={setFront}
              onSelect={setSelectedId}
              onError={report}
              onProgress={setProgress}
              onRefresh={() => refresh().catch(report)}
              onProject={(project) => {
                setProjects((current) => {
                  const rest = current.filter((item) => item.id !== project.id);
                  return [...rest, project].sort((a, b) => a.name.localeCompare(b.name));
                });
                setSelectedId(project.id);
              }}
            />
          ) : null}
          {section === "components" ? (
            <Components
              catalog={catalog}
              packages={packages}
              desktop={desktop}
              onError={report}
              onProgress={setProgress}
              onRefresh={() => refresh().catch(report)}
            />
          ) : null}
          {section === "databases" ? (
            <DatabasesScreen
              databases={databases}
              projects={projects}
              desktop={desktop}
              onDatabases={setDatabases}
              onError={report}
              onProgress={setProgress}
            />
          ) : null}
          {section === "settings" ? (
            <Settings desktop={desktop} onError={report} onSaved={() => setError("")} />
          ) : null}
        </main>
      </div>
    </div>
  );
}

function Mark() {
  return (
    <svg className="mark" viewBox="0 0 18 14" aria-hidden="true">
      <rect x="0" y="0" width="18" height="2.5" rx="1" />
      <rect x="0" y="5.75" width="13" height="2.5" rx="1" />
      <rect x="0" y="11.5" width="8" height="2.5" rx="1" />
    </svg>
  );
}

function SectionButton({
  current,
  id,
  label,
  onSelect,
}: {
  current: Section;
  id: Section;
  label: string;
  onSelect: (section: Section) => void;
}) {
  return (
    <button
      type="button"
      className="nav-item"
      aria-current={current === id ? "page" : undefined}
      onClick={() => onSelect(id)}
    >
      {label}
    </button>
  );
}

function Projects({
  projects,
  selected,
  packages,
  front,
  desktop,
  onSelect,
  onFront,
  onError,
  onProgress,
  onRefresh,
  onProject,
}: {
  projects: Project[];
  selected: Project | null;
  packages: Installed[];
  front: Front | null;
  desktop: boolean;
  onSelect: (id: string) => void;
  onFront: (front: Front) => void;
  onError: (err: unknown) => void;
  onProgress: (text: string) => void;
  onRefresh: () => void;
  onProject: (project: Project) => void;
}) {
  const [path, setPath] = useState("");
  const [logs, setLogs] = useState("");
  const [serviceId, setServiceId] = useState("");
  const [packageName, setPackageName] = useState("");
  const [scope, setScope] = useState("");
  const [task, setTask] = useState<Task | null>(null);

  useEffect(() => {
    if (!selected) {
      return;
    }
    const running = selected.services.some((service) =>
      ["starting", "running", "orphaned"].includes(service.state),
    );
    if (!running && !task) {
      return;
    }
    const timer = window.setInterval(() => {
      api
        .project(selected.id)
        .then(onProject)
        .catch(onError);
      if (task && task.state === "running") {
        api
          .task(task.id)
          .then((next) => {
            setTask(next);
            onProgress(next.state === "running" ? `${next.label}…` : "");
          })
          .catch(onError);
      }
    }, 1000);
    return () => window.clearInterval(timer);
  }, [selected, task, onError, onProgress, onProject]);

  useEffect(() => {
    if (!selected) {
      setLogs("");
      return;
    }
    const nextService = selected.services.find((service) => service.id === serviceId) ?? selected.services[0];
    if (!nextService) {
      return;
    }
    if (nextService.id !== serviceId) {
      setServiceId(nextService.id);
    }
    api
      .logs(selected.id, nextService.id, 0)
      .then((chunk) => setLogs(chunk.text))
      .catch(onError);
  }, [selected, serviceId, onError]);

  async function run(label: string, action: () => Promise<void>) {
    onProgress(label);
    try {
      await action();
      onProgress("");
    } catch (err) {
      onProgress("");
      onError(err);
    }
  }

  const runningCount = projects.reduce(
    (count, project) => count + project.services.filter((service) => service.state === "running").length,
    0,
  );
  const finding =
    projects.length === 0
      ? "Nothing is running. Import a folder to start one."
      : runningCount === 0
        ? `${projects.length} ${projects.length === 1 ? "project" : "projects"}, none running.`
        : `${runningCount} ${runningCount === 1 ? "service" : "services"} running.`;
  const job = selected ? projectJob(selected) : null;

  return (
    <>
      <p className="finding">{finding}</p>
      {front ? (
        <section className="block" aria-label="Apache">
          <h2>Apache</h2>
          <p className="hint">
            Apache is {front.state}. HTTP <span className="mono">{front.httpPort}</span>, HTTPS{" "}
            <span className="mono">{front.httpsPort}</span>. {front.restartImpact}
          </p>
          {front.resolvesLocalhost ? null : (
            <p className="hint">
              This machine does not resolve names under .localhost. A hosts entry is required before a browser can open a site.
            </p>
          )}
          {front.sites.length > 0 ? (
            <button
              type="button"
              className="btn"
              disabled={!desktop}
              onClick={() =>
                void run("Restarting Apache…", async () => {
                  onFront(await api.restartFront());
                })
              }
            >
              Restart Apache
            </button>
          ) : null}
        </section>
      ) : (
        <section className="block" aria-label="Apache">
          <h2>Apache</h2>
          <p className="hint">
            One Apache serves every published site. Restarting it interrupts every site. Status appears when this window can reach the engine.
          </p>
        </section>
      )}
      <div className={selected ? "split picked" : "split"}>
        <section className="pane list-pane" aria-label="Project list">
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void run("Importing…", async () => {
                const project = await api.importProject(path.trim());
                onProject(project);
                setPath("");
              });
            }}
          >
            <div className="field">
              <label htmlFor="project-path">Project folder</label>
              <input
                id="project-path"
                className="input"
                value={path}
                disabled={!desktop}
                onChange={(event) => setPath(event.target.value)}
                placeholder="Path to a folder with stackyard.toml"
              />
            </div>
            <button type="submit" className={selected ? "btn" : "btn primary"} disabled={!desktop || path.trim() === ""}>
              Import
            </button>
          </form>
          {projects.length === 0 ? (
            <p className="empty">No projects yet. Import a folder that contains stackyard.toml.</p>
          ) : (
            <ul className="slips">
              {projects.map((project) => {
                const rollup = projectRollup(project);
                return (
                  <li key={project.id}>
                    <button
                      type="button"
                      className="slip"
                      aria-current={project.id === selected?.id ? "true" : undefined}
                      onClick={() => onSelect(project.id)}
                    >
                      <span className="slip-name">{project.name}</span>
                      <span className={rollup.tone ? `pill ${rollup.tone}` : "pill"}>{rollup.label}</span>
                      <span className="slip-sub">
                        <span>{project.services.length} {project.services.length === 1 ? "service" : "services"}</span>
                        <span>{project.manager || "no manager"}</span>
                      </span>
                    </button>
                  </li>
                );
              })}
            </ul>
          )}
        </section>
        <section className="pane detail-pane" aria-labelledby="detail-heading">
          <button type="button" className="btn back" onClick={() => onSelect("")}>
            All projects
          </button>
          {selected ? (
            <ProjectDetail
              project={selected}
              packages={packages}
              logs={logs}
              serviceId={serviceId || selected.services[0]?.id || ""}
              packageName={packageName}
              scope={scope}
              task={task}
              desktop={desktop}
              front={front}
              primaryLabel={job?.label ?? ""}
              onFront={onFront}
              onService={setServiceId}
              onPackageName={setPackageName}
              onScope={setScope}
              onTask={setTask}
              onRun={run}
              onRefresh={onRefresh}
              onProject={onProject}
              onProgress={onProgress}
              onError={onError}
            />
          ) : (
            <EmptyDetail />
          )}
        </section>
      </div>
      {projects.length === 0 ? (
        <section className="block">
          <h2>The manifest</h2>
          <p className="hint">
            Import reads stackyard.toml from that folder. The command is an argument list, and {"{port}"} is the
            loopback port Stackyard binds.
          </p>
          <pre className="output">{MANIFEST_SHAPE}</pre>
        </section>
      ) : null}
    </>
  );
}

const MANIFEST_SHAPE = `schema = 1
name = "site"

[[service]]
id = "web"
runtime = "node"
command = ["pnpm", "run", "dev"]
port = 3000
ready_path = "/"

[site]
service = "web"
host = "site.localhost"`;

function EmptyDetail() {
  return (
    <>
      <h2 id="detail-heading">No project selected</h2>
      <p className="empty-copy">Import a folder, then start it. Stop affects only that project.</p>
      <div className="block">
        <h3>Services</h3>
        <p className="empty">Each service in stackyard.toml shows its state, port, and pid here.</p>
      </div>
      <div className="block">
        <h3>Dependencies</h3>
        <p className="empty">Install, add, and update stay inside the project. They do not change the Node or Python runtime.</p>
      </div>
      <div className="block">
        <h3>Logs</h3>
        <p className="empty">The selected service log is read from this machine. Nothing has been written yet.</p>
      </div>
    </>
  );
}

function projectRollup(project: Project): { label: string; tone: string } {
  const states = project.services.map((service) => service.state);
  if (states.includes("running")) {
    return { label: "Running", tone: "pos" };
  }
  if (states.includes("starting")) {
    return { label: "Starting", tone: "warn" };
  }
  if (states.includes("orphaned")) {
    return { label: "Orphaned", tone: "neg" };
  }
  if (states.includes("failed")) {
    return { label: "Failed", tone: "neg" };
  }
  if (states.length === 0) {
    return { label: "No services", tone: "" };
  }
  return { label: "Stopped", tone: "" };
}

function projectJob(project: Project): {
  label: string;
  progress: string;
  run: (id: string) => Promise<Project>;
} {
  const states = project.services.map((service) => service.state);
  if (states.includes("starting")) {
    return { label: "Cancel start", progress: "Cancelling…", run: api.cancelStart };
  }
  if (states.some((state) => state === "running" || state === "orphaned")) {
    return { label: "Stop", progress: "Stopping…", run: api.stopProject };
  }
  return { label: "Start", progress: "Starting…", run: api.startProject };
}

function pillClass(state: string): string {
  if (state === "running") {
    return "pill pos";
  }
  if (state === "failed" || state === "orphaned") {
    return "pill neg";
  }
  if (state === "starting") {
    return "pill warn";
  }
  return "pill";
}

function SiteActions({
  projectId,
  front,
  desktop,
  onFront,
  onRun,
}: {
  projectId: string;
  front: Front | null;
  desktop: boolean;
  onFront: (front: Front) => void;
  onRun: (label: string, action: () => Promise<void>) => Promise<void>;
}) {
  const site = front?.sites.find((item) => item.projectId === projectId) ?? null;
  if (!front) {
    return null;
  }
  return (
    <div className="block">
      <h3>Site</h3>
      {site ? (
        <>
          <p className="hint">
            <span className="mono">{site.host}</span> · {site.mode} · {site.upstream}
          </p>
          <p className="mono">{site.httpsUrl}</p>
          <div className="row">
            <button
              type="button"
              className="btn"
              disabled={!desktop}
              onClick={() => void onRun("Opening site…", () => api.openSite(site.httpsUrl))}
            >
              Open site
            </button>
            <button
              type="button"
              className="btn"
              disabled={!desktop}
              onClick={() =>
                void onRun("Unpublishing…", async () => {
                  onFront(await api.unpublish(projectId));
                })
              }
            >
              Unpublish
            </button>
          </div>
        </>
      ) : (
        <>
          <p className="hint">Publish uses the [site] table. A proxy follows the service port. A root serves files, including .htaccess.</p>
          <button
            type="button"
            className="btn"
            disabled={!desktop}
            onClick={() =>
              void onRun("Publishing…", async () => {
                onFront(await api.publish(projectId));
              })
            }
          >
            Publish
          </button>
        </>
      )}
    </div>
  );
}

function ProjectDetail({
  project,
  packages,
  logs,
  serviceId,
  packageName,
  scope,
  task,
  desktop,
  front,
  primaryLabel,
  onFront,
  onService,
  onPackageName,
  onScope,
  onTask,
  onRun,
  onRefresh,
  onProject,
  onProgress,
  onError,
}: {
  project: Project;
  packages: Installed[];
  logs: string;
  serviceId: string;
  packageName: string;
  scope: string;
  task: Task | null;
  desktop: boolean;
  front: Front | null;
  primaryLabel: string;
  onFront: (front: Front) => void;
  onService: (id: string) => void;
  onPackageName: (value: string) => void;
  onScope: (value: string) => void;
  onTask: (task: Task | null) => void;
  onRun: (label: string, action: () => Promise<void>) => Promise<void>;
  onRefresh: () => void;
  onProject: (project: Project) => void;
  onProgress: (text: string) => void;
  onError: (err: unknown) => void;
}) {
  const service = project.services.find((item) => item.id === serviceId) ?? project.services[0];
  const roles = [...new Set(project.services.map((item) => item.runtime).filter(Boolean))];

  async function act(label: string, action: () => Promise<Project>) {
    await onRun(label, async () => {
      onProject(await action());
    });
  }

  const extras = [
    { label: "Start", progress: "Starting…", run: () => api.startProject(project.id) },
    { label: "Restart", progress: "Restarting…", run: () => api.restartProject(project.id) },
    { label: "Stop", progress: "Stopping…", run: () => api.stopProject(project.id) },
    { label: "Cancel start", progress: "Cancelling…", run: () => api.cancelStart(project.id) },
  ].filter((item) => item.label !== primaryLabel);

  return (
    <>
      <h2 id="detail-heading">{project.name}</h2>
      <p className="path mono">{project.root}</p>
      <SiteActions projectId={project.id} front={front} desktop={desktop} onFront={onFront} onRun={onRun} />
      <div className="row lifecycle">
        <button
          type="button"
          className="btn primary phone-only"
          disabled={!desktop}
          onClick={() => void act(projectJob(project).progress, () => projectJob(project).run(project.id))}
        >
          {primaryLabel}
        </button>
        {extras.map((item) => (
          <button
            key={item.label}
            type="button"
            className="btn"
            disabled={!desktop}
            onClick={() => void act(item.progress, item.run)}
          >
            {item.label}
          </button>
        ))}
      </div>
      <div className="row">
        <button type="button" className="btn" disabled={!desktop} onClick={() => void onRun("Opening folder…", () => api.openProject(project.id, "folder", ""))}>
          Open folder
        </button>
        <button type="button" className="btn" disabled={!desktop} onClick={() => void onRun("Opening editor…", () => api.openProject(project.id, "editor", ""))}>
          Open editor
        </button>
        <button type="button" className="btn" disabled={!desktop} onClick={() => void onRun("Opening shell…", () => api.openProject(project.id, "shell", ""))}>
          Open shell
        </button>
        <button
          type="button"
          className="btn"
          disabled={!desktop || !service?.port}
          onClick={() => void onRun("Opening URL…", () => api.openProject(project.id, "url", service?.id ?? ""))}
        >
          Open URL
        </button>
      </div>
      <div className="block">
        <h3>Services</h3>
        {project.services.length === 0 ? <p className="empty">This project has no services.</p> : null}
        <ul className="slips">
          {project.services.map((item) => (
            <li key={item.id}>
              <ServiceSlip item={item} current={item.id === service?.id} onSelect={onService} />
              {item.detail ? <p className="detail-line">{item.detail}</p> : null}
            </li>
          ))}
        </ul>
      </div>
      <div className="block">
        <h3>Runtime</h3>
        {roles.length === 0 ? <p className="empty">No runtime role is declared.</p> : null}
        {roles.map((role) => (
          <RuntimeSelect
            key={role}
            role={role}
            project={project}
            packages={packages.filter((item) => item.name === role)}
            disabled={!desktop}
            onChanged={onRefresh}
            onError={onError}
          />
        ))}
      </div>
      <div className="block">
        <h3>Dependencies</h3>
        <p>
          {project.manager ? (
            <>
              Manager <strong>{project.manager}</strong>. {project.managerDetail}
            </>
          ) : (
            project.managerDetail || "No package manager is selected."
          )}
        </p>
        <p className="hint">Updating a package changes this project. It does not change the Node or Python runtime.</p>
        <form
          onSubmit={(event) => {
            event.preventDefault();
          }}
        >
          <div className="field">
            <label htmlFor="dep-scope">Workspace directory</label>
            <input id="dep-scope" className="input" value={scope} onChange={(event) => onScope(event.target.value)} placeholder="Empty uses the project root" />
          </div>
          <div className="field">
            <label htmlFor="dep-package">Package</label>
            <input id="dep-package" className="input" value={packageName} onChange={(event) => onPackageName(event.target.value)} placeholder="Required for add, remove, and update" />
          </div>
        </form>
        <div className="row">
          <button type="button" className="btn" disabled={!desktop} onClick={() => void dependency("install", project, scope, packageName, onTask, onProgress, onError)}>
            Install dependencies
          </button>
          <button type="button" className="btn" disabled={!desktop || packageName.trim() === ""} onClick={() => void dependency("add", project, scope, packageName, onTask, onProgress, onError)}>
            Add package
          </button>
          <button type="button" className="btn" disabled={!desktop || packageName.trim() === ""} onClick={() => void dependency("remove", project, scope, packageName, onTask, onProgress, onError)}>
            Remove package
          </button>
          <button type="button" className="btn" disabled={!desktop || packageName.trim() === ""} onClick={() => void dependency("update", project, scope, packageName, onTask, onProgress, onError)}>
            Update selected package
          </button>
          <button type="button" className="btn" disabled={!desktop} onClick={() => void dependency("list", project, scope, packageName, onTask, onProgress, onError)}>
            List installed
          </button>
        </div>
        {task ? (
          <pre className="output" tabIndex={0}>
            {task.label} · {task.state}
            {"\n"}
            {task.message}
          </pre>
        ) : (
          <p className="empty">Dependency output appears here after an install, add, remove, update, or list.</p>
        )}
      </div>
      <div className="block">
        <h3>Logs</h3>
        {logs ? (
          <pre className="output" tabIndex={0} aria-label="Service log">
            {logs}
          </pre>
        ) : (
          <p className="empty">No log output yet for {service?.id ?? "this service"}.</p>
        )}
      </div>
    </>
  );
}

function ServiceSlip({
  item,
  current,
  onSelect,
}: {
  item: Service;
  current: boolean;
  onSelect: (id: string) => void;
}) {
  return (
    <button type="button" className="slip" aria-current={current ? "true" : undefined} onClick={() => onSelect(item.id)}>
      <span className="slip-name">{item.id}</span>
      <span className={pillClass(item.state)}>{item.state}</span>
      <span className="slip-sub">
        <span>{item.runtime || "service"}</span>
        {item.port ? <span className="mono">127.0.0.1:{item.port}</span> : <span>no port</span>}
        {item.pid ? <span className="mono">pid {item.pid}</span> : <span>-</span>}
      </span>
    </button>
  );
}

function RuntimeSelect({
  role,
  project,
  packages,
  disabled,
  onChanged,
  onError,
}: {
  role: string;
  project: Project;
  packages: Installed[];
  disabled: boolean;
  onChanged: () => void;
  onError: (err: unknown) => void;
}) {
  const selected = project.selections.find((item) => item.role === role);
  return (
    <label className="runtime">
      {role}
      <select
        className="input"
        aria-label={`${role} runtime`}
        disabled={disabled || packages.length === 0}
        value={selected?.packageId ?? ""}
        onChange={(event) => {
          const packageId = event.target.value;
          if (!packageId) {
            return;
          }
          void api.selectPackage(project.id, packageId).then(onChanged).catch(onError);
        }}
      >
        <option value="">{packages.length === 0 ? `No ${role} recorded` : `Choose ${role}`}</option>
        {packages.map((item) => (
          <option key={item.id} value={item.id}>
            {item.version} · {item.source}
          </option>
        ))}
      </select>
      {selected ? (
        <span className="meta">
          Using {selected.version} ({selected.source})
        </span>
      ) : null}
    </label>
  );
}

async function dependency(
  action: string,
  project: Project,
  scope: string,
  packageName: string,
  onTask: (task: Task | null) => void,
  onProgress: (text: string) => void,
  onError: (err: unknown) => void,
) {
  onProgress(`${action} ${project.manager || "dependencies"}…`);
  try {
    let current = await api.startDependency(project.id, action, packageName.trim(), scope.trim());
    onTask(current);
    for (let attempt = 0; attempt < 240 && current.state === "running"; attempt += 1) {
      await delay(500);
      current = await api.task(current.id);
      onTask(current);
    }
    onProgress("");
    if (current.state === "failed" || current.state === "cancelled") {
      onError(current.message || current.state);
    }
  } catch (err) {
    onProgress("");
    onError(err);
  }
}

function DatabasesScreen({
  databases,
  projects,
  desktop,
  onDatabases,
  onError,
  onProgress,
}: {
  databases: Databases | null;
  projects: Project[];
  desktop: boolean;
  onDatabases: (view: Databases) => void;
  onError: (err: unknown) => void;
  onProgress: (text: string) => void;
}) {
  const [engine, setEngine] = useState("mysql");
  const [port, setPort] = useState("");
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

  async function run(label: string, action: () => Promise<Databases>) {
    onProgress(label);
    try {
      onDatabases(await action());
      onProgress("");
    } catch (err) {
      onProgress("");
      onError(err);
    }
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
                disabled={!desktop}
                onClick={() =>
                  void run(row.state === "running" ? "Stopping database…" : "Starting database…", () =>
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
              onError("Database port must be empty or a high port.");
              return;
            }
            void run("Creating database instance…", () => api.createDatabase(engine, chosen));
          }}
        >
          <div className="field">
            <label htmlFor="db-engine">Engine</label>
            <select id="db-engine" className="input" value={engine} onChange={(event) => setEngine(event.target.value)}>
              <option value="mysql">MySQL</option>
              <option value="postgres">PostgreSQL</option>
            </select>
          </div>
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
          <button type="submit" className="btn primary" disabled={!desktop}>
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
                disabled={!desktop}
                onClick={() =>
                  void run("Detaching connection…", () => api.detachDatabase(row.projectId, row.connectionId))
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
              onError("Import a project before attaching a database.");
              return;
            }
            if (!Number.isInteger(chosen) || chosen < 1024 || chosen > 65535) {
              onError("An external database port must be a high port.");
              return;
            }
            void run("Attaching connection…", async () => {
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
              onError("Import a project before selecting a SQLite file.");
              return;
            }
            void run("Preparing SQLite file…", () => api.useSqlite(sqliteProject, sqlitePath.trim()));
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
          <button type="submit" className="btn" disabled={!desktop}>
            Use SQLite file
          </button>
        </form>
      </section>
    </>
  );
}

function Components({
  catalog,
  packages,
  desktop,
  onError,
  onProgress,
  onRefresh,
}: {
  catalog: CatalogItem[];
  packages: Installed[];
  desktop: boolean;
  onError: (err: unknown) => void;
  onProgress: (text: string) => void;
  onRefresh: () => void;
}) {
  const [role, setRole] = useState("python");
  const [path, setPath] = useState("");
  const [operation, setOperation] = useState<Operation | null>(null);
  const finding =
    packages.length === 0
      ? "No runtime is recorded on this machine."
      : `${packages.length} ${packages.length === 1 ? "tool" : "tools"} recorded.`;

  async function install(item: CatalogItem) {
    onProgress(`Installing ${item.name} ${item.version}…`);
    try {
      let current = await api.startInstall(item.id);
      setOperation(current);
      for (let attempt = 0; attempt < 600 && current.state === "running"; attempt += 1) {
        await delay(500);
        current = await api.installStatus(current.id);
        setOperation(current);
        onProgress(`${item.name}: ${current.message || current.state}`);
      }
      onProgress("");
      if (current.state !== "succeeded") {
        onError(current.message || current.state);
      }
      onRefresh();
    } catch (err) {
      onProgress("");
      onError(err);
    }
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
                    disabled={!desktop || operation?.state === "running"}
                    onClick={() => void install(item)}
                  >
                    Install {item.name} {item.version}
                  </button>
                );
              })}
            </div>
          )}
          {operation ? (
            <p className="progress" role="status">
              <span>
                {operation.state}: {operation.message} ({operation.bytes} bytes)
              </span>
              {operation.state === "running" ? (
                <button type="button" className="btn ghost" onClick={() => void api.cancelInstall(operation.id).then(setOperation).catch(onError)}>
                  Cancel install
                </button>
              ) : null}
            </p>
          ) : null}
          <h2>Use existing</h2>
          <p className="hint">Recording does not copy or upgrade that installation.</p>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              onProgress("Recording…");
              void api
                .useExisting(role, path.trim())
                .then(() => {
                  setPath("");
                  onProgress("");
                  onRefresh();
                })
                .catch((err: unknown) => {
                  onProgress("");
                  onError(err);
                });
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
            <button type="submit" className="btn primary" disabled={!desktop || path.trim() === ""}>
              Use existing
            </button>
          </form>
        </section>
      </div>
      <section className="block">
        <h2>External connections</h2>
        <p className="hint">
          External databases are recorded on Databases. Detach removes the connection and leaves that server running.
        </p>
      </section>
    </>
  );
}

function Settings({
  desktop,
  onError,
  onSaved,
}: {
  desktop: boolean;
  onError: (err: unknown) => void;
  onSaved: () => void;
}) {
  const [name, setName] = useState("");
  const [editor, setEditor] = useState("code");
  const [background, setBackground] = useState(false);
  const [saved, setSaved] = useState(false);

  useEffect(() => {
    if (!desktop) {
      return;
    }
    api.setting("display_name").then((value) => setName(value ?? "")).catch(onError);
    api.setting("editor_command").then((value) => setEditor(value || "code")).catch(onError);
    api.setting("background_mode").then((value) => setBackground(value === "on")).catch(onError);
  }, [desktop, onError]);

  return (
    <>
      <p className="finding">
        {background ? "Quit leaves managed projects running." : "Quit stops managed projects."}
      </p>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          setSaved(false);
          void Promise.all([
            api.putSetting("display_name", name),
            api.putSetting("editor_command", editor.trim() || "code"),
            api.putSetting("background_mode", background ? "on" : "off"),
          ])
            .then(() => {
              setSaved(true);
              onSaved();
            })
            .catch(onError);
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
              <input id="display-name" className="input" value={name} disabled={!desktop} onChange={(event) => setName(event.target.value)} />
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
              <input id="editor-command" className="input" value={editor} disabled={!desktop} onChange={(event) => setEditor(event.target.value)} />
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
                disabled={!desktop}
                onChange={(event) => setBackground(event.target.checked)}
              />
            </label>
          </div>
        </div>
        <div className="row">
          <button type="submit" className="btn primary" disabled={!desktop}>
            Save
          </button>
          {saved ? <span className="hint">Saved</span> : null}
        </div>
      </form>
    </>
  );
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}
