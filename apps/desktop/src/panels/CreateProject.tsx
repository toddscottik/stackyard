import { useEffect, useRef, useState } from "react";
import { api, errorText, type CatalogItem, type Installed, type Project, type RecipeInfo, type CreatePlan, type CreateRequest, type ComponentChoice } from "../api";
import type { ActivityController } from "./Activity";
import "./create-project.css";

type Props = {
  enabled: boolean;
  packages: Installed[];
  catalog: CatalogItem[];
  projects: Project[];
  activity: ActivityController;
  onGenerated: (project: Project) => void;
  onPackagesChanged: () => void;
  onOpenProject: (id: string, tab?: "Logs") => void;
};

type Step = "details" | "runtime" | "review" | "progress" | "ready";

const supportedRuntime = (runtime: string) => runtime.toLowerCase().includes("node") ? "node" : runtime.toLowerCase();
const slugPattern = /^[a-z][a-z0-9-]{0,47}$/;
const recipeRuntimeLabel = (runtime: string) => runtime.toLowerCase().includes("node") ? "Node.js" : runtime;
function validProjectName(value: string) {
  const name = value.trim();
  const reserved = /^(con|prn|aux|nul|default)$/.test(name) || /^(com|lpt)[0-9]$/.test(name);
  return slugPattern.test(name) && !reserved;
}

export function CreateProject({ enabled, packages, catalog, projects, activity, onGenerated, onPackagesChanged, onOpenProject }: Props) {
  const [recipes, setRecipes] = useState<RecipeInfo[]>([]);
  const [recipeId, setRecipeId] = useState("");
  const [parent, setParent] = useState("");
  const [name, setName] = useState("");
  const [manager, setManager] = useState("");
  const [runtimeChoice, setRuntimeChoice] = useState("");
  const [toolChoice, setToolChoice] = useState("");
  const [existingPath, setExistingPath] = useState("");
  const [plan, setPlan] = useState<CreatePlan | null>(null);
  const [step, setStep] = useState<Step>("details");
  const [busy, setBusy] = useState("");
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [created, setCreated] = useState<Project | null>(null);
  const [dependenciesInstalled, setDependenciesInstalled] = useState(false);
  const [taskId, setTaskId] = useState("");
  const [taskKind, setTaskKind] = useState<"generate" | "install" | "" >("");
  const handled = useRef("");
  const selectedRecipe = recipes.find((item) => item.id === recipeId);
  const recipeManagers = [...new Set(recipes.filter((item) => item.id === recipeId).flatMap((item) => item.managers))];
  const selectedVariant = recipes.find((item) => item.id === recipeId && item.managers.includes(manager)) ?? selectedRecipe;
  const runtimeRole = selectedRecipe ? supportedRuntime(selectedRecipe.runtime) : "node";
  const runtimeCatalog = catalog.filter((item) => supportedRuntime(item.name) === runtimeRole);
  const runtimePackages = packages.filter((item) => supportedRuntime(item.name) === runtimeRole);
  const selectedRuntime = runtimeChoice.startsWith("catalog:")
    ? runtimeCatalog.find((item) => item.id === runtimeChoice.slice(8))
    : runtimePackages.find((item) => item.id === runtimeChoice);
  const selectedManager = recipeManagers.includes(manager) ? manager : "";
  const validName = validProjectName(name);
  const target = parent.trim() && name.trim() ? `${parent.replace(/[\\/]+$/, "")}${parent.includes("\\") ? "\\" : "/"}${name.trim()}` : "";
  const runtimeValue = selectedRuntime?.id ?? "";
  const recipeReadiness = selectedVariant ? [
    `Windows: ${selectedVariant.windows}`,
    `macOS: ${selectedVariant.macos}`,
    `Network: ${selectedVariant.network}`,
    `Hot reload: ${selectedVariant.hot_reload}`,
  ].join(" · ") : "";
  const running = taskId ? activity.entries.find((entry) => entry.id === taskId) : undefined;
  const busyTask = running?.state === "running";
  const draftLocked = Boolean(busy) || busyTask;
  const currentProject = created ? projects.find((item) => item.id === created.id) ?? created : null;
  const httpService = currentProject?.services.find((service) => service.endpoint?.protocol === "http" &&
    service.endpoint.resolvedPort !== null && service.endpoint.resolvedPort !== undefined);
  const projectStarting = currentProject?.services.some((service) => service.state === "starting") ?? false;
  const projectRunning = currentProject?.services.some((service) => service.state === "running") ?? false;
  const projectFailed = currentProject?.services.some((service) => ["failed", "invalid", "orphaned"].includes(service.state)) ?? false;
  const managerOptions = [
    ...packages.filter((item) => item.name === "pnpm" && item.version.replace(/^v/, "") === "12.16.1")
      .map((item) => ({ key: `installed:${item.id}`, choice: { kind: "installed", id: item.id } as const, label: `Recorded · ${item.version} · ${item.source}` })),
    ...catalog.filter((item) => item.name === "pnpm" && item.version === "12.16.1")
      .map((item) => ({ key: `catalog:${item.id}`, choice: { kind: "catalog", id: item.id } as const, label: `Managed · ${item.version} · ${item.target}` })),
  ];
  const selectedTool = managerOptions.find((item) => item.key === toolChoice) ?? managerOptions[0];
  const planDownloads = plan?.downloads ?? [];

  useEffect(() => {
    let active = true;
    if (!enabled) return;
    void api.recipes().then((result) => { if (active) { setRecipes(result); setRecipeId((id) => id || result[0]?.id || ""); } })
      .catch((err) => { if (active) setError(errorText(err)); });
    return () => { active = false; };
  }, [enabled]);

  useEffect(() => {
    if (!running || running.state === "running" || handled.current === running.id) return;
    handled.current = running.id;
    setBusy("");
    if (running.state !== "succeeded") {
      setError(`${taskKind === "generate" ? "Generation" : "Dependency installation"} ${running.state}: ${running.message}`);
      setStep("progress");
      return;
    }
    setError("");
    if (taskKind === "generate" && plan) {
      void showGenerated(plan.request.name);
    } else if (taskKind === "install") {
      setDependenciesInstalled(true);
      setNotice("Dependencies are installed. Start the project when ready.");
      setStep("ready");
    }
  }, [running?.state, running?.message, running?.id, taskKind, plan, created, onGenerated, onPackagesChanged]);

  useEffect(() => {
    if (projectRunning) {
      setError("");
      setNotice("Project is running. Open its URL or logs from here.");
    } else if (projectStarting) {
      setNotice("Project is starting. Its status will update here.");
    } else if (projectFailed && currentProject) {
      const detail = currentProject.services.find((service) => ["failed", "invalid", "orphaned"].includes(service.state))?.detail;
      setError(detail || "The project service failed to start. Check its logs, correct the issue, then retry Start.");
      setNotice("Startup did not complete. Review the service status and try again.");
    }
  }, [projectRunning, projectStarting, projectFailed, currentProject]);

  const tool: ComponentChoice | null = manager === "pnpm" ? selectedTool?.choice ?? null : null;
  const requests: CreateRequest | null = selectedRecipe && selectedManager && selectedRuntime && (manager !== "pnpm" || tool)
    ? { parent: parent.trim(), name: name.trim(), recipe: selectedRecipe.id, manager: selectedManager,
      runtime: { kind: runtimeChoice.startsWith("catalog:") ? "catalog" : "installed", id: runtimeValue }, tool }
    : null;

  async function showGenerated(projectId: string) {
    setBusy("opening");
    setError("");
    try {
      const project = await api.project(projectId);
      onGenerated(project);
      onPackagesChanged();
      setCreated(project);
      setDependenciesInstalled(false);
      setStep("ready");
      setNotice("Project source was generated. Install its dependencies to continue.");
    } catch (err) {
      setError(`The project was generated. Its status could not be loaded: ${errorText(err)}`);
    } finally {
      setBusy("");
    }
  }

  function clearPreview() { if (draftLocked) return; setPlan(null); if (step === "review") setStep("runtime"); }
  async function browseParent() {
    setBusy("browse"); setError("");
    try { const path = await api.pickProjectParent(); if (path) { setParent(path); clearPreview(); } }
    catch (err) { setError(errorText(err)); }
    finally { setBusy(""); }
  }
  async function review() {
    if (!enabled || !requests || !validName || !parent.trim()) return;
    setBusy("plan"); setError("");
    try { setPlan(await api.planCreate(requests)); setStep("review"); }
    catch (err) { setError(`Could not prepare this project: ${errorText(err)}`); }
    finally { setBusy(""); }
  }
  async function recordExisting() {
    if (!existingPath.trim()) return;
    setBusy("existing"); setError("");
    try {
      const id = await api.recordCreateRuntime(runtimeRole, existingPath.trim());
      onPackagesChanged();
      setRuntimeChoice(id);
      setExistingPath("");
      clearPreview();
    } catch (err) { setError(`Could not record this runtime: ${errorText(err)}`); }
    finally { setBusy(""); }
  }
  async function generate() {
    if (!plan || !enabled || busy || busyTask) return;
    setBusy("generate"); setError("");
    try { const task = await activity.startCreate(plan); setTaskId(task.id); setTaskKind("generate"); setStep("progress"); }
    catch (err) { setError(`Could not start generation: ${errorText(err)}`); }
    finally { setBusy(""); }
  }
  async function installDependencies() {
    if (!currentProject || !enabled || busy || busyTask) return;
    const scope = currentProject.scopes.find((item) => item.manager === selectedManager) ?? currentProject.scopes[0];
    if (!scope) { setError("Project was created without a dependency scope. Open its configuration before installing."); return; }
    setBusy("install"); setError("");
    try { const task = await activity.startScopedInstall(currentProject.id, scope.id); setTaskId(task.id); setTaskKind("install"); setStep("progress"); }
    catch (err) { setError(`Could not start dependency installation: ${errorText(err)}`); }
    finally { setBusy(""); }
  }
  async function startProject() {
    if (!currentProject || busy || projectStarting || projectRunning) return;
    setBusy("start"); setError("");
    try {
      const project = await api.startProject(currentProject.id);
      onGenerated(project); setCreated(project);
      if (project.services.some((service) => service.state === "running")) setNotice("Project is running. Open its URL or logs from here.");
      else if (project.services.some((service) => service.state === "starting")) setNotice("Project start requested. Waiting for service readiness…");
      else if (project.services.some((service) => ["failed", "invalid", "orphaned"].includes(service.state))) {
        const detail = project.services.find((service) => ["failed", "invalid", "orphaned"].includes(service.state))?.detail;
        setError(detail || "The project service failed to start. Check its logs, correct the issue, then retry Start.");
        setNotice("Startup did not complete. Review the service status and try again.");
      } else setNotice("Start was accepted. The project status will update here.");
    }
    catch (err) { setError(`Could not start the project: ${errorText(err)}`); }
    finally { setBusy(""); }
  }
  async function open(action: "url" | "folder" | "editor") {
    if (!currentProject || busy || (action === "url" && (!projectRunning || !httpService))) return;
    setBusy(action); setError("");
    try { await api.openProject(currentProject.id, action, action === "url" ? httpService?.id ?? "" : ""); }
    catch (err) { setError(`Could not open ${action}: ${errorText(err)}`); }
    finally { setBusy(""); }
  }
  function edit(next: () => void) {
    if (draftLocked) return;
    next(); setPlan(null); setStep("details"); setError(""); setNotice("");
    if (running && running.state !== "running") { setTaskId(""); setTaskKind(""); }
  }

  return <section className="create-project" aria-labelledby="create-project-heading">
    <p className="finding">Create a runnable starter with a reviewed file plan. Generated source and dependency installation are separate steps.</p>
    <div className="create-heading"><div><h2 id="create-project-heading">New project</h2><p className="hint">Choose a location, starter, and compatible tools.</p></div>
      <span className="create-step">{step === "details" ? "1 · Project" : step === "runtime" ? "2 · Runtime" : step === "review" ? "3 · Review" : step === "progress" ? "Working" : "Ready"}</span></div>
    {step !== "ready" && <div className="create-review-stack">
      <section className="create-card" aria-labelledby="create-location-heading">
        <h3 id="create-location-heading">Location and starter</h3>
        <div className="field"><label htmlFor="create-parent">Parent folder</label><div className="create-path-row">
          <input id="create-parent" className="input mono" value={parent} disabled={draftLocked} onChange={(event) => edit(() => setParent(event.target.value))} placeholder="Choose an absolute folder path" />
          <button type="button" className="btn" disabled={!enabled || draftLocked} onClick={() => void browseParent()}>Browse…</button>
        </div></div>
        <div className="create-two-fields">
          <div className="field"><label htmlFor="create-name">Project folder name</label><input id="create-name" className="input" value={name} disabled={draftLocked} onChange={(event) => edit(() => setName(event.target.value))} autoComplete="off" aria-invalid={Boolean(name) && !validName} aria-describedby="create-name-help" />
            <span id="create-name-help" className="meta">Use 1–48 lowercase letters, numbers, or hyphens. Start with a letter. Device names and default are reserved.</span></div>
          <div className="field"><label htmlFor="create-recipe">Starter</label><select id="create-recipe" className="input" value={recipeId} disabled={draftLocked} onChange={(event) => edit(() => { setRecipeId(event.target.value); setManager(""); setRuntimeChoice(""); })}>
            {[...new Map(recipes.map((recipe) => [recipe.id, recipe])).values()].map((recipe) => <option key={recipe.id} value={recipe.id}>{recipe.title}</option>)}
          </select></div>
        </div>
        {selectedRecipe && <p className="meta">{recipeRuntimeLabel(selectedRecipe.runtime)} · recipe v{selectedRecipe.version} · {recipeReadiness}. Recipe target support describes recorded evidence, not a guarantee for this machine.</p>}
        {target && <p className="create-target"><span className="meta">Target</span><code className="mono">{target}</code></p>}
      </section>
      <section className="create-card" aria-labelledby="create-runtime-heading">
        <h3 id="create-runtime-heading">Runtime and package manager</h3>
        <div className="create-two-fields">
          <div className="field"><label htmlFor="create-runtime">{recipeRuntimeLabel(runtimeRole)} runtime</label><select id="create-runtime" className="input" value={runtimeChoice} disabled={draftLocked} onChange={(event) => edit(() => setRuntimeChoice(event.target.value))}>
            <option value="">Choose a recorded or managed runtime</option>
            {runtimePackages.map((item) => <option key={item.id} value={item.id}>{item.source === "managed" ? "Installed" : "Existing"} · {item.version} · {item.path}</option>)}
            {runtimeCatalog.map((item) => <option key={item.id} value={`catalog:${item.id}`}>Managed · {item.version} · {item.target}</option>)}
          </select></div>
          <div className="field"><label htmlFor="create-manager">Package manager</label><select id="create-manager" className="input" value={manager} disabled={draftLocked} onChange={(event) => edit(() => setManager(event.target.value))}>
            <option value="">Choose package manager</option>{recipeManagers.map((item) => <option key={item} value={item}>{item}{item === "pnpm" ? " · 12.16.1 pinned" : ""}</option>)}
          </select></div>
        </div>
        {selectedRuntime && <p className="meta">Selected {selectedRuntime.version} from {"source" in selectedRuntime ? selectedRuntime.source : selectedRuntime.target}. Runtime is recorded for this project; it is not upgraded globally.</p>}
        {manager === "pnpm" && (managerOptions.length ? <div className="field">
          <label htmlFor="create-tool">pnpm 12.16.1 executable</label>
          <select id="create-tool" className="input" value={selectedTool?.key ?? ""} disabled={draftLocked}
            onChange={(event) => edit(() => setToolChoice(event.target.value))}>
            {managerOptions.map((item) => <option key={item.key} value={item.key}>{item.label}</option>)}
          </select>
        </div> : <p className="hint">pnpm 12.16.1 is required for this starter. No compatible executable is recorded or published for this machine.</p>)}
        <details className="create-existing"><summary>Record an existing runtime executable</summary><div className="field"><label htmlFor="create-existing-path">Absolute executable path</label><input id="create-existing-path" className="input mono" value={existingPath} disabled={draftLocked} onChange={(event) => edit(() => setExistingPath(event.target.value))} placeholder={runtimeRole === "node" ? "C:\\…\\node.exe" : "C:\\…\\python.exe"} /></div>
          <button type="button" className="btn" disabled={!enabled || !existingPath.trim() || draftLocked} onClick={() => void recordExisting()}>{busy === "existing" ? "Recording…" : "Record existing runtime"}</button></details>
      </section>
      <section className="create-card" aria-labelledby="create-review-heading">
        <h3 id="create-review-heading">Review and generate</h3>
        {plan ? <>
          <p className="meta">Stackyard checks the destination again before creating files.</p>
          <h4>Files to create</h4><ul className="create-list">{plan.files.map((file) => <li key={file}><code className="mono">{file}</code></li>)}</ul>
          <h4>Downloads</h4>{planDownloads.length ? <ul className="create-list">{planDownloads.map((item) => <li key={item.id}>{item.name} {item.version} · {item.target}</li>)}</ul> : <p className="meta">No runtime downloads in this plan.</p>}
          <h4>Commands (argument arrays)</h4><div className="create-commands"><p><strong>Install</strong><code className="mono">{JSON.stringify(plan.install_command)}</code></p><p><strong>Launch</strong><code className="mono">{JSON.stringify(plan.launch_command)}</code></p></div>
          {plan.notes.map((item, index) => <p className="hint" key={index}>{item}</p>)}
        </> : <p className="meta">Set the parent folder, valid project name, runtime and package manager to preview the exact files and commands.</p>}
        <div className="row create-actions">
          {step === "review" ? <><button type="button" className="btn" disabled={draftLocked} onClick={() => setStep("runtime")}>Back to edit</button><button type="button" className="btn primary" disabled={!enabled || !plan || draftLocked} onClick={() => void generate()}>{busy === "generate" ? "Starting…" : "Generate project"}</button></>
            : <button type="button" className="btn primary" disabled={!enabled || !validName || !parent.trim() || !requests || Boolean(busy) || busyTask} onClick={() => void review()}>{busy === "plan" ? "Preparing review…" : "Review project"}</button>}
        </div>
      </section>
    </div>}
    {step === "progress" && <section className="create-card" aria-live="polite"><h3>{taskKind === "generate" ? "Generating source" : "Installing dependencies"}</h3>
      {running ? <><p className="meta"><span className="mono">{running.id}</span> · {running.state}</p><p>{running.message}</p></> : <p role="status">Submitting the task…</p>}
      {busyTask && running && <button type="button" className="btn" disabled={!enabled} onClick={() => void activity.cancel(running.id)}>Cancel task</button>}
      {running && running.state !== "running" && running.state !== "succeeded" && taskKind === "generate" && plan &&
        <><button type="button" className="btn" disabled={!enabled || Boolean(busy)} onClick={() => void generate()}>Retry generation</button><button type="button" className="btn" disabled={Boolean(busy)} onClick={() => edit(() => {})}>Edit project details</button></>}
      {running && running.state !== "running" && running.state !== "succeeded" && taskKind === "install" && created &&
        <button type="button" className="btn" disabled={!enabled || Boolean(busy)} onClick={() => void installDependencies()}>Retry dependency install</button>}
      {running?.state === "succeeded" && taskKind === "generate" && plan && error &&
        <button type="button" className="btn" disabled={!enabled || Boolean(busy)} onClick={() => void showGenerated(plan.request.name)}>Retry project lookup</button>}
      {error && <p role="alert" className="action-error">{error}</p>}
      {taskId && <button type="button" className="btn" onClick={() => activity.refresh()}>Refresh activity</button>}
    </section>}
    {step === "ready" && <section className="create-card" aria-labelledby="create-ready-heading"><h3 id="create-ready-heading">{currentProject?.name ?? name} is ready to continue</h3>
      {notice && <p role="status">{notice}</p>}{currentProject && <p className="path mono">{currentProject.root}</p>}
      {error && <p role="alert" className="action-error">{error}</p>}
      <div className="row create-actions">
        {currentProject && !dependenciesInstalled && <button type="button" className="btn primary" disabled={!enabled || draftLocked} onClick={() => void installDependencies()}>{busy === "install" ? "Starting…" : "Install dependencies"}</button>}
        {currentProject && dependenciesInstalled && !projectRunning && <button type="button" className="btn primary" disabled={!enabled || draftLocked || projectStarting} onClick={() => void startProject()}>{projectStarting || busy === "start" ? "Starting…" : projectFailed ? "Retry start" : "Start project"}</button>}
        {currentProject && <><button type="button" className="btn" disabled={!enabled || draftLocked || !httpService || !projectRunning} onClick={() => void open("url")}>Open URL</button><button type="button" className="btn" onClick={() => onOpenProject(currentProject.id, "Logs")}>Open logs</button><button type="button" className="btn" disabled={!enabled || draftLocked} onClick={() => void open("folder")}>Open folder</button></>}
        <button type="button" className="btn" disabled={draftLocked} onClick={() => { setStep("details"); setPlan(null); setCreated(null); setDependenciesInstalled(false); setTaskId(""); setTaskKind(""); setNotice(""); setError(""); setName(""); }}>Create another</button>
      </div>
    </section>}
    {error && step !== "progress" && <p role="alert" className="action-error create-error">{error}</p>}
    {notice && step !== "ready" && <p role="status" className="hint">{notice}</p>}
  </section>;
}
