import { invoke } from "@tauri-apps/api/core";

export type Status = {
  pid: number;
  dataDir: string;
  apiVersion: number;
  schemaVersion: number;
  lastEvent: string | null;
};

export type CatalogItem = { id: string; name: string; version: string; target: string };
export type Installed = {
  id: string;
  name: string;
  source: string;
  version: string;
  path: string;
  launch: string;
};
export type Service = {
  id: string;
  runtime: string;
  state: string;
  pid: number | null;
  port: number | null;
  detail: string;
  scopeId: string;
  readinessState: string;
  readiness: Readiness | null;
  httpStatus: number | null;
  endpoint: { protocol: "http" | "tcp" | "fastcgi"; requestedPort: number; resolvedPort: number | null } | null;
};
export type Scope = { id: string; directory: string; runtime: string; manager: string; managerDetail: string; selections: Selection[] };
export type Selection = {
  role: string;
  packageId: string;
  version: string;
  source: string;
  origin: "scope" | "project" | "global";
};
export type Project = {
  id: string;
  name: string;
  root: string;
  manager: string;
  managerDetail: string;
  selections: Selection[];
  services: Service[];
  scopes: Scope[];
};
export type Operation = {
  id: string;
  catalogId: string;
  state: string;
  bytes: number;
  total: number;
  message: string;
};

export function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const digits = unit === 0 || value >= 100 ? 0 : value >= 10 ? 1 : 2;
  return `${value.toFixed(digits)} ${units[unit]}`;
}

export function formatProgress(bytes: number, total: number): string {
  if (total > 0) {
    return `${formatSize(bytes)} / ${formatSize(total)}`;
  }
  return formatSize(bytes);
}
export type Task = { id: string; label: string; state: string; message: string };
export type FrontSite = {
  projectId: string;
  host: string;
  mode: string;
  upstream: string;
  httpUrl: string;
  httpsUrl: string;
};
export type Front = {
  state: string;
  pid: number | null;
  httpPort: number;
  httpsPort: number;
  restartImpact: string;
  caPath: string;
  resolvesLocalhost: boolean;
  sites: FrontSite[];
};
export type Logs = { offset: number; nextOffset: number; text: string };
export type UpdateNotice = { kind: string; subject: string; detail: string };
export type DbInstance = {
  id: string;
  engine: string;
  port: number;
  dataDir: string;
  state: string;
  pid: number | null;
};
export type DbBinding = {
  projectId: string;
  connectionId: string;
  instanceId: string;
  engine: string;
  host: string;
  port: number;
  databaseName: string;
  userName: string;
  external: boolean;
};
export type SqliteFile = { projectId: string; path: string };
export type Databases = {
  instances: DbInstance[];
  bindings: DbBinding[];
  sqlite: SqliteFile[];
  message: string;
};

export function engineAvailable(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function errorText(err: unknown): string {
  if (typeof err === "string") {
    return err;
  }
  if (err instanceof Error) {
    return err.message;
  }
  return String(err);
}

// Preview shares the protocol's snake_case DTOs verbatim; view adapters use camelCase.
export type PhpProject = {
  identity: { version: string; target: string; architecture: string; thread_safe: boolean; compiler: string; api: string; zend_api: string; debug: boolean; extension_build: string; zend_extension_build: string };
  profile: unknown | null; inventory: unknown[]; available: unknown[]; limitations: string;
};
export type Diagnostic = { code: string; severity: "error" | "info"; field: string; scope_id?: string; service_id?: string; message: string };
export type Readiness = { kind: "http"; path: string; statuses: number[]; timeout_secs: number } | { kind: "tcp"; timeout_secs: number } | { kind: "process"; grace_ms: number };
export type PreviewRequest = { root: string; manifest_text: string; project_id?: string | null; selection_overrides?: { scope_id: string; role: string; package_id: string }[] };
export type ProjectPreview = {
  can_apply: boolean;
  scopes: { id: string; runtime: string; manager: string; runtime_package_id: string | null; tool_package_id: string | null; environment_state: string }[];
  services: { id: string; scope_id: string; endpoint: { protocol: "http" | "tcp" | "fastcgi"; port: number } | null; resolved_port: number | null; readiness: Readiness }[];
  diagnostics: Diagnostic[];
};

export const api = {
  previewProject: (draft: PreviewRequest) => invoke<ProjectPreview>("preview_project", { draft }),
  selectScopePackage: (projectId: string, scopeId: string, packageId: string) => invoke<Project>("select_scope_package", { projectId, scopeId, packageId }),
  startDependencyScoped: (projectId: string, scopeId: string, action: string, packageName = "") => invoke<Task>("start_dependency_scoped", { projectId, scopeId, action, package: packageName }),
  phpStatusScoped: (projectId: string, scopeId: string) => invoke<PhpProject>("php_status_scoped", { projectId, scopeId }),
  phpApplyProfileScoped: (projectId: string, scopeId: string, extensions: string[]) => invoke<PhpProject>("php_apply_profile_scoped", { projectId, scopeId, extensions }),
  phpInstallExtensionScoped: (projectId: string, scopeId: string, catalogId: string) => invoke<unknown>("php_install_extension_scoped", { projectId, scopeId, catalogId }),
  status: () => invoke<Status>("engine_status"),
  probeStatus: () => invoke<Status>("probe_engine"),
  activity: () => invoke<{ operations: Operation[]; tasks: Task[] }>("list_activity"),
  followLogs: (projectId: string, serviceId: string, offset: number, generation: string | null) =>
    invoke<Logs & { generation: string; reset: boolean }>("follow_logs", { projectId, serviceId, offset, generation }),
  setting: (key: string) => invoke<string | null>("engine_setting", { key }),
  putSetting: (key: string, value: string) => invoke<void>("put_setting", { key, value }),
  packages: () => invoke<Installed[]>("list_packages"),
  catalog: () => invoke<CatalogItem[]>("list_catalog"),
  startInstall: (catalogId: string) => invoke<Operation>("start_install", { catalogId }),
  installStatus: (operationId: string) => invoke<Operation>("install_status", { operationId }),
  cancelInstall: (operationId: string) => invoke<Operation>("cancel_install", { operationId }),
  useExisting: (role: string, path: string) => invoke<string>("use_existing", { role, path }),
  selectPackage: (projectId: string, packageId: string) =>
    invoke<void>("select_package", { projectId, packageId }),
  projects: () => invoke<Project[]>("list_projects"),
  importProject: (path: string) => invoke<Project>("import_project", { path }),
  project: (projectId: string) => invoke<Project>("project_status", { projectId }),
  startProject: (projectId: string) => invoke<Project>("start_project", { projectId }),
  stopProject: (projectId: string) => invoke<Project>("stop_project", { projectId }),
  restartProject: (projectId: string) => invoke<Project>("restart_project", { projectId }),
  cancelStart: (projectId: string) => invoke<Project>("cancel_start", { projectId }),
  logs: (projectId: string, serviceId: string, offset: number) =>
    invoke<Logs>("read_logs", { projectId, serviceId, offset }),
  startDependency: (projectId: string, action: string, packageName: string, scope: string) =>
    invoke<Task>("start_dependency", { projectId, action, package: packageName, scope }),
  task: (operationId: string) => invoke<Task>("task_status", { operationId }),
  cancelTask: (operationId: string) => invoke<Task>("cancel_task", { operationId }),
  openProject: (projectId: string, action: string, serviceId: string) =>
    invoke<void>("open_project", { projectId, action, serviceId }),
  front: () => invoke<Front>("front_status"),
  publish: (projectId: string) => invoke<Front>("publish_project", { projectId }),
  unpublish: (projectId: string) => invoke<Front>("unpublish_project", { projectId }),
  restartFront: () => invoke<Front>("restart_front"),
  openSite: (url: string) => invoke<void>("open_site", { url }),
  discover: () => invoke<Array<{ role: string; path: string }>>("discover_runtimes"),
  databases: () => invoke<Databases>("databases_status"),
  createDatabase: (engine: string, port: number) => invoke<Databases>("database_create", { engine, port }),
  startDatabase: (instanceId: string) => invoke<Databases>("database_start", { instanceId }),
  stopDatabase: (instanceId: string) => invoke<Databases>("database_stop", { instanceId }),
  attachDatabase: (input: {
    projectId: string;
    engine: string;
    host: string;
    port: number;
    database: string;
    user: string;
    password: string;
  }) => invoke<Databases>("database_attach", input),
  detachDatabase: (projectId: string, connectionId: string) =>
    invoke<Databases>("database_detach", { projectId, connectionId }),
  useSqlite: (projectId: string, path: string) => invoke<Databases>("sqlite_use", { projectId, path }),
  diagnostics: () => invoke<string>("export_diagnostics"),
  updates: () => invoke<UpdateNotice[]>("list_updates"),
  cleanupUnused: () => invoke<UpdateNotice[]>("cleanup_unused"),
};
