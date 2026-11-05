import assert from "node:assert/strict";
import test from "node:test";
import { api } from "../src/api.ts";
import { createResource } from "../src/resource.ts";

const turn = () => new Promise((resolve) => setImmediate(resolve));

test("a stale project list cannot undo an action result or overlap an invalidated read", async () => {
  const requests = [];
  globalThis.window = { __TAURI_INTERNALS__: { invoke(command, args) {
    assert.equal(command, "list_projects");
    assert.deepEqual(args, {});
    return new Promise((resolve) => requests.push(resolve));
  } } };
  const projects = createResource(api.projects);
  projects.setEnabled(true);
  await turn();
  projects.refresh();
  projects.update(() => [{ id: "alpha", state: "running" }]);
  projects.invalidate();
  assert.equal(requests.length, 1);
  requests.shift()([{ id: "alpha", state: "stopped" }]);
  await turn();
  assert.equal(projects.getSnapshot().value[0].state, "running");
  assert.equal(requests.length, 1);
  requests.shift()([{ id: "alpha", state: "running" }, { id: "beta", state: "stopped" }]);
  await turn();
  assert.deepEqual(projects.getSnapshot().value.map(({ id }) => id), ["alpha", "beta"]);
});

test("offline invalidates the old response and reconnect loads a fresh snapshot", async () => {
  const requests = [];
  const connection = createResource(() => new Promise((resolve) => requests.push(resolve)));
  connection.setEnabled(true);
  await turn();
  connection.setEnabled(false);
  connection.setEnabled(true);
  requests.shift()({ pid: 10 });
  await turn();
  assert.equal(connection.getSnapshot().value, undefined);
  requests.shift()({ pid: 11 });
  await turn();
  assert.equal(connection.getSnapshot().value.pid, 11);
});

test("passive probe, activity, and log cursor use their invoke contracts", async () => {
  const calls = [];
  const chunk = { offset: 0, nextOffset: 4, generation: "after", reset: true, text: "next" };
  globalThis.window = { __TAURI_INTERNALS__: { async invoke(command, args) {
    calls.push([command, args]);
    return command === "follow_logs" ? chunk : null;
  } } };
  await api.probeStatus();
  await api.status();
  await api.activity();
  assert.equal(await api.followLogs("alpha", "web", 24, "before"), chunk);
  assert.deepEqual(calls, [
    ["probe_engine", {}],
    ["engine_status", {}],
    ["list_activity", {}],
    ["follow_logs", { projectId: "alpha", serviceId: "web", offset: 24, generation: "before" }],
  ]);
});

test("creation sends the reviewed plan and records runtimes without the default selection command", async () => {
  const request = {
    parent: "D:\\Workspace",
    name: "my-app",
    recipe: "node-http",
    manager: "npm",
    runtime: { kind: "installed", id: "existing-node" },
    tool: null,
  };
  const plan = { request, target: "D:\\Workspace\\my-app", fingerprint: "reviewed",
    files: ["stackyard.toml", "server.js"], downloads: [], install_command: ["npm", "ci"],
    launch_command: ["node", "--watch", "server.js"], notes: [] };
  const calls = [];
  globalThis.window = { __TAURI_INTERNALS__: { async invoke(command, args) {
    calls.push([command, args]);
    if (command === "plan_create") return plan;
    if (command === "record_create_runtime") return "existing-node";
    return { id: "task-1", state: "running" };
  } } };
  assert.equal(await api.recordCreateRuntime("node", "D:\\Tools\\node.exe"), "existing-node");
  assert.equal(await api.planCreate(request), plan);
  assert.equal((await api.applyCreate(plan)).id, "task-1");
  assert.deepEqual(calls, [
    ["record_create_runtime", { role: "node", path: "D:\\Tools\\node.exe" }],
    ["plan_create", { request }],
    ["apply_create", { plan }],
  ]);
});
