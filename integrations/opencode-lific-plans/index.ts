// opencode-lific-plans — Lific-backed planning for OpenCode.
//
// Overrides the builtin `todowrite` tool so planning renders with the native
// todo block (the TUI name-gates that rendering to "todowrite") AND is
// persisted to a Lific plan that survives sessions + compaction.
//
// Which Lific PROJECT a folder's plans go to is set ONCE PER FOLDER via the
// `set_lific_project` tool (stored in ~/.cache/opencode/lific-plans/projects.json,
// keyed by worktree). There is deliberately no global default project — calling
// `todowrite` before `set_lific_project` fails with a clear instruction.
//
// Connection config (plugin options OR env): LIFIC_URL, LIFIC_API_KEY.
// When unconfigured, the override falls back to pure native behavior so the
// plugin is always safe to load.

import type { Plugin } from "@opencode-ai/plugin";
import { tool } from "@opencode-ai/plugin";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

interface Step {
  id: number;
  title: string;
  done: boolean;
  children: Step[];
}
interface Plan {
  id: number;
  identifier: string;
  title: string;
  status: string;
  steps: Step[];
  step_count: number;
  done_count: number;
}
interface Todo {
  content: string;
  status: string;
  priority?: string;
}
interface Cfg {
  url: string;
  apiKey: string;
}

function loadConfig(options?: Record<string, unknown>): Cfg | null {
  const pick = (k: string, env: string) =>
    (typeof options?.[k] === "string" ? (options![k] as string) : "") || process.env[env] || "";
  const url = pick("url", "LIFIC_URL").replace(/\/+$/, "");
  const apiKey = pick("apiKey", "LIFIC_API_KEY");
  if (!url || !apiKey) return null;
  return { url, apiKey };
}

const CACHE_DIR = join(homedir(), ".cache", "opencode", "lific-plans");

// ── Per-folder project map: { [worktree]: projectIdentifier } ──
const PROJECTS_FILE = join(CACHE_DIR, "projects.json");
function readProjects(): Record<string, string> {
  try {
    return JSON.parse(readFileSync(PROJECTS_FILE, "utf8")) as Record<string, string>;
  } catch {
    return {};
  }
}
function writeProjects(map: Record<string, string>) {
  try {
    mkdirSync(CACHE_DIR, { recursive: true });
    writeFileSync(PROJECTS_FILE, JSON.stringify(map, null, 2));
  } catch {
    /* best-effort */
  }
}
const folderKey = (worktree?: string, directory?: string) => worktree || directory || process.cwd();
function getFolderProject(key: string): string | undefined {
  return readProjects()[key];
}
function setFolderProject(key: string, project: string) {
  const m = readProjects();
  m[key] = project;
  writeProjects(m);
}

// ── Per-session plan store: { plans: {project: planId}, latest: planId } ──
function storePath(sessionID: string) {
  return join(CACHE_DIR, `${sessionID.replace(/[^A-Za-z0-9_-]/g, "_")}.json`);
}
type SessionStore = { plans: Record<string, number>; latest?: number };
function readStore(sessionID: string): SessionStore {
  try {
    return JSON.parse(readFileSync(storePath(sessionID), "utf8")) as SessionStore;
  } catch {
    return { plans: {} };
  }
}
function writeStore(sessionID: string, store: SessionStore) {
  try {
    mkdirSync(CACHE_DIR, { recursive: true });
    writeFileSync(storePath(sessionID), JSON.stringify(store));
  } catch {
    /* best-effort */
  }
}

const isDone = (status: string) => status === "completed" || status === "cancelled";

class Lific {
  constructor(private cfg: Cfg) {}
  private async req<T>(method: string, path: string, body?: unknown): Promise<T> {
    const res = await fetch(`${this.cfg.url}/api${path}`, {
      method,
      headers: { "content-type": "application/json", authorization: `Bearer ${this.cfg.apiKey}` },
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    if (!res.ok) {
      const detail = await res.text().catch(() => "");
      throw new Error(`${method} ${path} → ${res.status} ${detail}`.trim());
    }
    return (res.status === 204 ? null : await res.json()) as T;
  }
  projects() {
    return this.req<Array<{ id: number; identifier: string }>>("GET", "/projects");
  }
  getPlan(id: number) {
    return this.req<Plan>("GET", `/plans/${id}`);
  }
  createPlan(projectId: number, title: string) {
    return this.req<Plan>("POST", "/plans", { project_id: projectId, title });
  }
  setPlan(id: number, patch: Record<string, unknown>) {
    return this.req<Plan>("PUT", `/plans/${id}`, patch);
  }
  addStep(planId: number, title: string) {
    return this.req<Plan>("POST", `/plans/${planId}/steps`, { title });
  }
  setStep(planId: number, stepId: number, patch: Record<string, unknown>) {
    return this.req<unknown>("PUT", `/plans/${planId}/steps/${stepId}`, patch);
  }
  deleteStep(planId: number, stepId: number) {
    return this.req<unknown>("DELETE", `/plans/${planId}/steps/${stepId}`);
  }
}

async function syncTodos(lific: Lific, planId: number, todos: Todo[]): Promise<Plan> {
  let plan = await lific.getPlan(planId);
  const byTitle = new Map<string, Step>();
  for (const s of plan.steps) if (!byTitle.has(s.title)) byTitle.set(s.title, s);

  const desired = todos.map((t) => ({ title: t.content, done: isDone(t.status) }));
  const desiredTitles = new Set(desired.map((d) => d.title));

  for (const d of desired) {
    const existing = byTitle.get(d.title);
    if (existing) {
      if (existing.done !== d.done) await lific.setStep(planId, existing.id, { done: d.done });
    } else {
      const after = await lific.addStep(planId, d.title);
      const created = after.steps.find((s) => s.title === d.title);
      if (created && d.done) await lific.setStep(planId, created.id, { done: true });
    }
  }
  for (const s of plan.steps) {
    if (!desiredTitles.has(s.title)) await lific.deleteStep(planId, s.id);
  }

  const allDone = desired.length > 0 && desired.every((d) => d.done);
  plan = await lific.getPlan(planId);
  const target = allDone ? "done" : "active";
  if (plan.status !== target && plan.status !== "archived") plan = await lific.setPlan(planId, { status: target });
  return plan;
}

function renderPlanMarkdown(plan: Plan): string {
  const lines: string[] = [];
  const walk = (steps: Step[], depth: number) => {
    for (const s of steps) {
      lines.push(`${"  ".repeat(depth)}- [${s.done ? "x" : " "}] ${s.title}`);
      if (s.children?.length) walk(s.children, depth + 1);
    }
  };
  walk(plan.steps, 0);
  return lines.join("\n");
}

const TODOWRITE_DESCRIPTION = `Create and manage a structured task list for the current coding session, persisted as a durable Lific plan.

Keep it updated as you work: exactly one task in_progress at a time, mark tasks completed the moment they're done, add follow-ups as they appear. Use for any non-trivial multi-step work (3+ steps).

Each todo: { content, status (pending|in_progress|completed|cancelled), priority (high|medium|low) }. The whole list is replaced each call. It renders inline like the native todo list AND is mirrored to a Lific plan for this folder.

Requires the folder's Lific project to be set first via \`set_lific_project\`; if it isn't, this tool fails with instructions.`;

export const LificPlans: Plugin = async ({ client, worktree, directory }, options) => {
  let cfg = loadConfig(options);
  let lific = cfg ? new Lific(cfg) : null;
  const projectIdCache = new Map<string, number | null>();
  const mcpServerName =
    (typeof options?.mcpServer === "string" ? (options.mcpServer as string) : "") ||
    process.env.LIFIC_MCP_SERVER ||
    "lific";

  const log = (level: string, message: string) =>
    client.app.log({ body: { service: "lific-plans", level: level as never, message } }).catch(() => {});

  // No duplicated credentials: if URL/key weren't given explicitly, reuse the
  // Lific MCP server's own connection from opencode.json (its `url` minus the
  // /mcp suffix + the bearer token in its Authorization header). Explicit
  // options/env still win. Runs in the `config` hook, before any tool executes.
  function deriveFromMcp(config: unknown) {
    if (cfg) return;
    const servers = (config as { mcp?: Record<string, any> } | undefined)?.mcp;
    const m = servers?.[mcpServerName];
    if (!m) return;
    const rawUrl = typeof m.url === "string" ? m.url : "";
    const authHeader = m.headers?.Authorization ?? m.headers?.authorization ?? "";
    const token = typeof authHeader === "string" ? authHeader.replace(/^Bearer\s+/i, "").trim() : "";
    const url = rawUrl.replace(/\/mcp\/?$/i, "").replace(/\/+$/, "");
    if (url && token) {
      cfg = { url, apiKey: token };
      lific = new Lific(cfg);
    }
  }

  async function resolveProjectId(identifier: string): Promise<number | null> {
    if (projectIdCache.has(identifier)) return projectIdCache.get(identifier)!;
    const id = (await lific!.projects()).find((p) => p.identifier === identifier)?.id ?? null;
    projectIdCache.set(identifier, id);
    return id;
  }

  async function ensurePlan(sessionID: string, project: string): Promise<number> {
    const store = readStore(sessionID);
    const cached = store.plans[project];
    if (cached != null) {
      try {
        await lific!.getPlan(cached);
        return cached;
      } catch {
        /* stale — recreate */
      }
    }
    const pid = await resolveProjectId(project);
    if (pid == null) throw new Error(`project '${project}' not found in Lific`);
    const repo = (worktree || directory || "").split("/").filter(Boolean).pop() || "session";
    const short = sessionID.slice(-6);
    const plan = await lific!.createPlan(pid, `OpenCode · ${repo} · ${short}`);
    store.plans[project] = plan.id;
    store.latest = plan.id;
    writeStore(sessionID, store);
    await log("info", `created plan ${plan.identifier} (project ${project}, session ${sessionID})`);
    return plan.id;
  }

  return {
    // Pull connection details from the Lific MCP server config (once, at init).
    config: async (config) => {
      deriveFromMcp(config);
    },
    tool: {
      // Set the Lific project for THIS folder — required before planning.
      set_lific_project: tool({
        description:
          "Set the Lific project that plans for the current folder/workspace are stored under. Required once per folder before the todo tool can persist; the choice is remembered across sessions. Pass the project identifier (e.g. LIF).",
        args: {
          project: tool.schema.string().describe("Lific project identifier, e.g. LIF"),
        },
        async execute(args, context) {
          const lf = lific;
          const c = cfg;
          if (!lf || !c) {
            throw new Error(
              `Lific is not configured. Add a Lific MCP server named '${mcpServerName}' to opencode.json ` +
                `(the plugin reads its url + bearer token), or set LIFIC_URL and LIFIC_API_KEY.`,
            );
          }
          const project = args.project.trim();
          if (!project) throw new Error("project identifier is required");
          const pid = await resolveProjectId(project).catch((e) => {
            throw new Error(`couldn't reach Lific at ${c.url}: ${String(e)}`);
          });
          if (pid == null) {
            const ids = await lf
              .projects()
              .then((ps) => ps.map((p) => p.identifier))
              .catch(() => []);
            throw new Error(
              `Lific project '${project}' not found.` + (ids.length ? ` Available: ${ids.join(", ")}` : ""),
            );
          }
          const key = folderKey(context.worktree, context.directory);
          setFolderProject(key, project);
          await log("info", `folder ${key} → project ${project}`);
          return `Lific project for this folder set to '${project}'. Plans will be stored there. You can now use the todo tool.`;
        },
      }),

      // Override builtin todowrite: native render + Lific persistence.
      todowrite: tool({
        description: TODOWRITE_DESCRIPTION,
        args: {
          todos: tool.schema
            .array(
              tool.schema.object({
                content: tool.schema.string().describe("Brief description of the task"),
                status: tool.schema
                  .string()
                  .describe("Current status: pending, in_progress, completed, cancelled"),
                priority: tool.schema.string().describe("Priority: high, medium, low").optional(),
              }),
            )
            .describe("The updated todo list"),
        },
        async execute(args, context) {
          const todos = (args.todos ?? []) as Todo[];
          const incomplete = todos.filter((t) => t.status !== "completed").length;

          let footer = "";
          const lf = lific;
          const c = cfg;
          if (lf && c) {
            const key = folderKey(context.worktree, context.directory);
            const project = getFolderProject(key);
            if (!project) {
              throw new Error(
                "No Lific project set for this folder. Run set_lific_project({ project: \"<IDENTIFIER>\" }) " +
                  "first to choose where this folder's plans are stored, then retry.",
              );
            }
            try {
              const planId = await ensurePlan(context.sessionID, project);
              const plan = await syncTodos(lf, planId, todos);
              const store = readStore(context.sessionID);
              store.latest = planId;
              writeStore(context.sessionID, store);
              footer = `\n\nLific plan: ${plan.identifier} — ${plan.done_count}/${plan.step_count} done`;
            } catch (err) {
              throw new Error(`Lific planning failed — is Lific reachable at ${c.url}? (${String(err)})`);
            }
          }

          // CRITICAL: plugin tools must set rendered metadata via the RETURN
          // value, not context.metadata() — opencode's registry takes a bare
          // string return as metadata={}, which left the native todo block
          // stuck on "Updating todos…". Returning the same { title, output,
          // metadata: { todos } } shape the builtin uses makes the TUI render
          // the # Todos block (it's gated on metadata.todos). Cast because the
          // public tool() type declares a string return although the runtime
          // accepts this object (registry.ts handles both).
          return {
            title: `${incomplete} todos`,
            output: JSON.stringify(todos, null, 2) + footer,
            metadata: { todos },
          } as unknown as string;
        },
      }),
    },

    "experimental.session.compacting": async ({ sessionID }, output) => {
      const lf = lific;
      if (!lf) return;
      const planId = readStore(sessionID).latest;
      if (planId == null) return;
      try {
        const plan = await lf.getPlan(planId);
        if (plan.step_count === 0) return;
        output.context.push(
          `## Active Lific plan (${plan.identifier})\n` +
            `This session's plan lives in Lific and survives compaction. Resume from it; keep planning via the todo tool.\n\n` +
            renderPlanMarkdown(plan),
        );
      } catch {
        /* never block compaction */
      }
    },
  };
};

export default LificPlans;
