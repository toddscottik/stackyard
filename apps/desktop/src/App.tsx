import { useCallback, useEffect, useRef, useState } from "react";
import { api, engineAvailable, type Project } from "./api";
import { Shell, type Section } from "./Shell";
import { Projects } from "./panels/Projects";
import { Components } from "./panels/Components";
import { DatabasesScreen } from "./panels/Databases";
import { CreateProject } from "./panels/CreateProject";
import { Settings } from "./panels/Settings";
import { Activity, useActivity } from "./panels/Activity";
import { recalled, remember } from "./preferences";
import { useResource } from "./useResource";
import { useAction } from "./useAction";

function ResourceNotice({ name, resource }: {
  name: string;
  resource: { value: unknown; error: string; loading: boolean; refresh: () => void };
}) {
  if (resource.error) {
    return <div className="notice" role="alert">
      <span>Could not load {name}. {resource.error}</span>
      <button type="button" className="btn" disabled={resource.loading} onClick={resource.refresh}>Retry</button>
    </div>;
  }
  return resource.loading && resource.value === undefined
    ? <p className="hint" role="status">Loading {name}…</p>
    : null;
}

export function App() {
  const [section, setSection] = useState<Section>("projects");
  const [selectedId, setSelectedId] = useState(() => recalled("project"));
  const selection = useRef(selectedId);
  const [workspaceRequest, setWorkspaceRequest] = useState(0);
  const desktop = engineAvailable();
  const probe = useResource(api.probeStatus, { enabled: desktop, pollMs: 2500 });
  const { run, states } = useAction();
  const setStatus = probe.update;
  const startEngine = useCallback(() => run("connect", async () => {
    const status = await api.status();
    setStatus(() => status);
  }), [run, setStatus]);
  useEffect(() => {
    if (desktop) void startEngine();
  }, [desktop, startEngine]);

  const enabled = desktop && Boolean(probe.value) && !probe.error;
  const engineId = probe.value?.pid ?? null;
  const projects = useResource(api.projects, { enabled, identity: engineId, pollMs: 1200 });
  const packages = useResource(api.packages, { enabled, identity: engineId });
  const catalog = useResource(api.catalog, { enabled, identity: engineId });
  const front = useResource(api.front, { enabled, identity: engineId });
  const databases = useResource(api.databases, { enabled, identity: engineId });
  const activity = useActivity({ enabled, engineId, onInstalled: packages.refresh });

  const chooseProject = useCallback((id: string) => {
    selection.current = id;
    setSelectedId(id);
    remember("project", id);
  }, []);
  const known = projects.value;
  useEffect(() => {
    if (known?.length && selectedId !== "" && !known.some((project) => project.id === selectedId)) {
      chooseProject(known[0].id);
    }
  }, [known, selectedId, chooseProject]);

  function updateProject(project: Project) {
    projects.update((items) => [
      ...(items ?? []).filter((item) => item.id !== project.id),
      project,
    ].sort((a, b) => a.name.localeCompare(b.name)));
  }
  function refreshPackagesAndDatabases() {
    packages.refresh();
    databases.refresh();
  }

  const selected = known?.find((project) => project.id === selectedId) ?? null;

  return (
    <Shell section={section} onSelect={setSection} engineId={enabled ? engineId : null}>
      {!desktop ? <p className="notice" role="status">Open the Stackyard desktop window to reach the engine.</p> : null}
      {desktop && !enabled ? (
        <div className="notice" role="status">
          <span>{states.connect?.pending ? "Connecting to the engine…" : "Engine offline. Retry to reconnect."}</span>
          {states.connect?.error ? <span>{states.connect.error}</span> : null}
          <button type="button" className="btn" disabled={states.connect?.pending} onClick={() => void startEngine()}>
            Retry connection
          </button>
        </div>
      ) : null}
      <div hidden={section !== "projects"}>
        {enabled ? <ResourceNotice name="projects" resource={projects} /> : null}
        {enabled ? <ResourceNotice name="Apache status" resource={front} /> : null}
        <Projects
          projects={known ?? []}
          selected={selected}
          workspaceRequest={workspaceRequest}
          onSelect={chooseProject}
          onImport={(project) => {
            updateProject(project);
            if (selection.current === selectedId) chooseProject(project.id);
          }}
          onProject={updateProject}
          packages={packages.value ?? []}
          enabled={enabled}
          visible={section === "projects"}
          front={front.value ?? null}
          onFront={(value) => front.update(() => value)}
          refreshProjects={projects.refresh}
          onDependency={activity.startDependency}
          isDependencyBusy={activity.dependencyBusy}
          onActivity={() => setSection("activity")}
        />
      </div>
      <div hidden={section !== "create"}>
        <CreateProject enabled={enabled} packages={packages.value ?? []} catalog={catalog.value ?? []}
          projects={known ?? []} activity={activity} onGenerated={updateProject} onPackagesChanged={packages.refresh}
          onOpenProject={(id, tab) => {
            if (tab) remember(`project-tab.${id}`, tab);
            chooseProject(id);
            setWorkspaceRequest((current) => current + 1);
            setSection("projects");
          }} />
      </div>
      <div hidden={section !== "components"}>
        {enabled ? <ResourceNotice name="installed components" resource={packages} /> : null}
        {enabled ? <ResourceNotice name="component catalog" resource={catalog} /> : null}
        <Components catalog={catalog.value ?? []} packages={packages.value ?? []} desktop={enabled}
          onInstall={activity.startInstall} installing={activity.installing} onRefresh={packages.refresh} />
      </div>
      <div hidden={section !== "databases"}>
        {enabled ? <ResourceNotice name="databases" resource={databases} /> : null}
        {enabled ? <ResourceNotice name="installed components" resource={packages} /> : null}
        <DatabasesScreen databases={databases.value ?? null} packages={packages.value ?? []} catalog={catalog.value ?? []}
          projects={known ?? []} desktop={enabled} onDatabases={(value) => databases.update(() => value)}
          onInstall={activity.startInstall} installing={activity.installing} onRefresh={refreshPackagesAndDatabases} />
      </div>
      {section === "activity" ? <Activity controller={activity} /> : null}
      <div hidden={section !== "settings"}>
        <Settings desktop={enabled} engineId={engineId} />
      </div>
    </Shell>
  );
}
