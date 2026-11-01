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
};
export type Selection = {
  role: string;
  packageId: string;
  version: string;
  source: string;
};
export type Project = {
  id: string;
  name: string;
  root: string;
  manager: string;
  managerDetail: string;
  selections: Selection[];
  services: Service[];
};
export type Operation = {
  id: string;
  catalogId: string;
  state: string;
  bytes: number;
  message: string;
};
export type Task = { id: string; label: string; state: string; message: string };
export type Logs = { offset: number; nextOffset: number; text: string };

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

export const api = {
  status: () => invoke<Status>("engine_status"),
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
};
