import { useCallback, useEffect, useRef, useState } from "react";
import {
  api,
  engineAvailable,
  errorText,
  type CatalogItem,
  type Installed,
  type Operation,
  type Project,
  type Status,
  type Task,
} from "./api";

type Section = "projects" | "components" | "settings";

export function App() {
  const [section, setSection] = useState<Section>("projects");
  const [status, setStatus] = useState<Status | null>(null);
  const [projects, setProjects] = useState<Project[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [packages, setPackages] = useState<Installed[]>([]);
  const [catalog, setCatalog] = useState<CatalogItem[]>([]);
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
    const [nextStatus, nextProjects, nextPackages, nextCatalog] = await Promise.all([
      api.status(),
      api.projects(),
      api.packages(),
      api.catalog(),
    ]);
    setStatus(nextStatus);
    setProjects(nextProjects);
    setPackages(nextPackages);
    setCatalog(nextCatalog);
    setSelectedId((current) => current || nextProjects[0]?.id || "");
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

  return (
    <div className="shell">
      <nav aria-label="Sections">
        <p className="brand">Stackyard</p>
        <SectionButton current={section} id="projects" onSelect={setSection} label="Projects" />
        <SectionButton current={section} id="components" onSelect={setSection} label="Components" />
        <SectionButton current={section} id="settings" onSelect={setSection} label="Settings" />
        {status ? <p className="engine">Engine {status.pid}</p> : null}
      </nav>
      <main id="main" aria-busy={progress !== ""}>
        {error ? (
          <p role="alert" tabIndex={-1} ref={alertRef} className="banner">
            {error}
            <button type="button" className="quiet" onClick={() => setError("")}>
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
            desktop={desktop}
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
        {section === "settings" ? (
          <Settings desktop={desktop} onError={report} onSaved={() => setError("")} />
        ) : null}
      </main>
    </div>
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
      className={current === id ? "nav current" : "nav"}
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
  desktop,
  onSelect,
  onError,
  onProgress,
  onRefresh,
  onProject,
}: {
  projects: Project[];
  selected: Project | null;
  packages: Installed[];
  desktop: boolean;
  onSelect: (id: string) => void;
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

  return (
    <div className="split">
      <section aria-labelledby="projects-heading">
        <h1 id="projects-heading">Projects</h1>
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
          <label htmlFor="project-path">Project folder</label>
          <input
            id="project-path"
            value={path}
            disabled={!desktop}
            onChange={(event) => setPath(event.target.value)}
            placeholder="Path to a folder with stackyard.toml"
          />
          <button type="submit" disabled={!desktop || path.trim() === ""}>
            Import
          </button>
        </form>
        {projects.length === 0 ? (
          <p className="empty">No projects yet. Import a folder that contains stackyard.toml.</p>
        ) : (
          <ul className="project-list">
            {projects.map((project) => (
              <li key={project.id}>
                <button
                  type="button"
                  className={project.id === selected?.id ? "current" : ""}
                  aria-current={project.id === selected?.id ? "true" : undefined}
                  onClick={() => onSelect(project.id)}
                >
                  <span>{project.name}</span>
                  <span className="meta">{summary(project)}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
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
      ) : null}
    </div>
  );
}

function summary(project: Project): string {
  if (project.services.length === 0) {
    return "no services";
  }
  return project.services.map((service) => `${service.id} ${service.state}`).join(", ");
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

  return (
    <section aria-labelledby="detail-heading">
      <h2 id="detail-heading">{project.name}</h2>
      <p className="path">{project.root}</p>
      <div className="row">
        <button type="button" disabled={!desktop} onClick={() => void act("Starting…", () => api.startProject(project.id))}>
          Start
        </button>
        <button type="button" className="quiet" disabled={!desktop} onClick={() => void act("Restarting…", () => api.restartProject(project.id))}>
          Restart
        </button>
        <button type="button" className="quiet" disabled={!desktop} onClick={() => void act("Stopping…", () => api.stopProject(project.id))}>
          Stop
        </button>
        <button type="button" className="quiet" disabled={!desktop} onClick={() => void act("Cancelling…", () => api.cancelStart(project.id))}>
          Cancel start
        </button>
      </div>
      <div className="row">
        <button type="button" className="quiet" disabled={!desktop} onClick={() => void onRun("Opening folder…", () => api.openProject(project.id, "folder", ""))}>
          Open folder
        </button>
        <button type="button" className="quiet" disabled={!desktop} onClick={() => void onRun("Opening editor…", () => api.openProject(project.id, "editor", ""))}>
          Open editor
        </button>
        <button type="button" className="quiet" disabled={!desktop} onClick={() => void onRun("Opening shell…", () => api.openProject(project.id, "shell", ""))}>
          Open shell
        </button>
        <button
          type="button"
          className="quiet"
          disabled={!desktop || !service?.port}
          onClick={() => void onRun("Opening URL…", () => api.openProject(project.id, "url", service?.id ?? ""))}
        >
          Open URL
        </button>
      </div>
      <h3>Services</h3>
      {project.services.length === 0 ? <p className="empty">This project has no services.</p> : null}
      <ul className="services">
        {project.services.map((item) => (
          <li key={item.id}>
            <button type="button" className={item.id === service?.id ? "current" : ""} onClick={() => onService(item.id)}>
              {item.id} · {item.state}
              {item.port ? ` · 127.0.0.1:${item.port}` : ""}
              {item.pid ? ` · pid ${item.pid}` : ""}
            </button>
            {item.detail ? <p className="detail">{item.detail}</p> : null}
          </li>
        ))}
      </ul>
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
      <p className="note">Updating a package changes this project. It does not change the Node or Python runtime.</p>
      <form
        onSubmit={(event) => {
          event.preventDefault();
        }}
      >
        <label htmlFor="dep-scope">Workspace directory</label>
        <input id="dep-scope" value={scope} onChange={(event) => onScope(event.target.value)} placeholder="Empty uses the project root" />
        <label htmlFor="dep-package">Package</label>
        <input id="dep-package" value={packageName} onChange={(event) => onPackageName(event.target.value)} placeholder="Required for add, remove, and update" />
      </form>
      <div className="row">
        <button type="button" disabled={!desktop} onClick={() => void dependency("install", project, scope, packageName, onTask, onProgress, onError)}>
          Install dependencies
        </button>
        <button type="button" className="quiet" disabled={!desktop || packageName.trim() === ""} onClick={() => void dependency("add", project, scope, packageName, onTask, onProgress, onError)}>
          Add package
        </button>
        <button type="button" className="quiet" disabled={!desktop || packageName.trim() === ""} onClick={() => void dependency("remove", project, scope, packageName, onTask, onProgress, onError)}>
          Remove package
        </button>
        <button type="button" className="quiet" disabled={!desktop || packageName.trim() === ""} onClick={() => void dependency("update", project, scope, packageName, onTask, onProgress, onError)}>
          Update selected package
        </button>
        <button type="button" className="quiet" disabled={!desktop} onClick={() => void dependency("list", project, scope, packageName, onTask, onProgress, onError)}>
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
      <h3>Logs</h3>
      {logs ? (
        <pre className="output" tabIndex={0} aria-label="Service log">
          {logs}
        </pre>
      ) : (
        <p className="empty">No log output yet for {service?.id ?? "this service"}.</p>
      )}
    </section>
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
    <section aria-labelledby="components-heading">
      <h1 id="components-heading">Components</h1>
      <p>Install a managed runtime, or record an existing one. Recording does not copy or upgrade that installation.</p>
      <h2>Recorded</h2>
      {packages.length === 0 ? <p className="empty">No runtimes or tools are recorded yet.</p> : null}
      <ul>
        {packages.map((item) => (
          <li key={item.id}>
            <strong>{item.name}</strong> {item.version} · {item.source}
            <div className="path">{item.launch}</div>
          </li>
        ))}
      </ul>
      <h2>Install</h2>
      {catalog.length === 0 ? (
        <p className="empty">No managed packages are published for this operating system. Use an existing Node or Python.</p>
      ) : (
        <ul className="actions">
          {catalog.map((item) => {
            const installed = packages.some((pkg) => pkg.id === item.id);
            return (
              <li key={item.id}>
                {installed ? (
                  <span>Installed {item.name} {item.version}</span>
                ) : (
                  <button type="button" disabled={!desktop || operation?.state === "running"} onClick={() => void install(item)}>
                    Install {item.name} {item.version}
                  </button>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {operation ? (
        <p role="status">
          {operation.state}: {operation.message} ({operation.bytes} bytes)
          {operation.state === "running" ? (
            <button type="button" className="quiet" onClick={() => void api.cancelInstall(operation.id).then(setOperation).catch(onError)}>
              Cancel install
            </button>
          ) : null}
        </p>
      ) : null}
      <h2>Use existing</h2>
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
        <label htmlFor="existing-role">Runtime or tool</label>
        <select id="existing-role" value={role} onChange={(event) => setRole(event.target.value)}>
          <option value="node">Node.js</option>
          <option value="python">Python</option>
          <option value="pnpm">pnpm</option>
          <option value="uv">uv</option>
        </select>
        <label htmlFor="existing-path">Executable path</label>
        <input id="existing-path" value={path} onChange={(event) => setPath(event.target.value)} placeholder="Path to the executable" />
        <button type="submit" disabled={!desktop || path.trim() === ""}>
          Use existing
        </button>
      </form>
      <h2>External connections</h2>
      <p className="empty">No external connections. Database and remote endpoints are added with those providers.</p>
    </section>
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

  useEffect(() => {
    if (!desktop) {
      return;
    }
    api.setting("display_name").then((value) => setName(value ?? "")).catch(onError);
    api.setting("editor_command").then((value) => setEditor(value || "code")).catch(onError);
    api.setting("background_mode").then((value) => setBackground(value === "on")).catch(onError);
  }, [desktop, onError]);

  return (
    <section aria-labelledby="settings-heading">
      <h1 id="settings-heading">Settings</h1>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void Promise.all([
            api.putSetting("display_name", name),
            api.putSetting("editor_command", editor.trim() || "code"),
            api.putSetting("background_mode", background ? "on" : "off"),
          ])
            .then(onSaved)
            .catch(onError);
        }}
      >
        <label htmlFor="display-name">Display name</label>
        <input id="display-name" value={name} disabled={!desktop} onChange={(event) => setName(event.target.value)} />
        <label htmlFor="editor-command">Editor command</label>
        <input id="editor-command" value={editor} disabled={!desktop} onChange={(event) => setEditor(event.target.value)} />
        <p className="note">Open editor runs this program with the project folder. It is not a shell command.</p>
        <label className="check">
          <input type="checkbox" checked={background} disabled={!desktop} onChange={(event) => setBackground(event.target.checked)} />
          Leave projects running when quitting
        </label>
        <p className="note">Closing the window hides Stackyard in the tray. Quit stops managed projects unless this is on.</p>
        <button type="submit" disabled={!desktop}>
          Save
        </button>
      </form>
    </section>
  );
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}
