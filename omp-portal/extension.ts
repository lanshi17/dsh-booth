/**
 * OMP frontend adapter for dsh-booth.
 *
 * OMP supplies the interaction kernel; booth owns the terminal product and the
 * local team-agent bridge. This extension only bridges the portal JSONL source
 * and hides worker bookkeeping.
 */
import { appendFile, mkdir, readFile, stat } from "node:fs/promises";
import { join } from "node:path";
import type { ExtensionAPI } from "@oh-my-pi/pi-coding-agent";
import { Text } from "@oh-my-pi/pi-tui";

type PortalReply = { id?: string; text?: string; kind?: string; thread?: string };

const stateDir = process.env.BOOTH_STATE_DIR || join(process.env.HOME || ".", ".dsh", "state", process.env.BOOTH_PROFILE || "superagent");
const inbox = join(stateDir, "booth-inbox.jsonl");
const outbox = join(stateDir, "booth-outbox.jsonl");
const conversationFile = join(stateDir, "booth-conversation");
const thread = `portal-${crypto.randomUUID()}`;
let activeThread = thread;
const apiBase = () => (process.env.DSH_SUPERAGENT_URL || `http://127.0.0.1:${process.env.BOOTH_PORT || "3082"}`).replace(/\/$/, "");

async function portalThread(): Promise<string> {

  await mkdir(stateDir, { recursive: true });
  try { activeThread = (await readFile(conversationFile, "utf8")).trim() || thread; }
  catch { await appendFile(conversationFile, thread); }
  return activeThread;
}

async function send(text: string): Promise<string> {
  const record = { id: crypto.randomUUID(), text: text.trim(), sender: "operator", thread: await portalThread() };
  await appendFile(inbox, `${JSON.stringify(record)}\n`);
  return record.id;
}

export default function boothPortal(pi: ExtensionAPI) {
  let offset = 0;
  let polling = false;
  let teamState: "ready" | "working" | "waiting" = "ready";
  const pending = new Map<string, { sent: number; warned: boolean }>();
  let ui: {
    setStatus(key: string, value: string | undefined): void;
    setTitle(title: string): void;
    setHeader?: (factory: unknown) => void;
  } | undefined;

  // OMP supplies the editor and terminal mechanics; booth owns the product
  // identity shown in the terminal title.
  function renderHeader(theme: { fg: (name: string, text: string) => string }) {
    const state = teamState === "working" ? "working" : teamState === "waiting" ? "waiting" : "ready";
    return new Text(theme.fg("accent", " booth ") + theme.fg("muted", `  team agents · ${state}`));
  }

  function brandBooth() {
    ui?.setTitle("booth");
    ui?.setStatus("booth-brand", `booth  ·  team agents  ·  ${teamState}`);
    // OMP's built-in header carries its own product name. Replace it with the
    // booth identity while leaving the editor, palette, history and shortcuts
    // supplied by the interaction kernel intact.
    ui?.setHeader?.(((_tui: unknown, theme: { fg: (name: string, text: string) => string }) => renderHeader(theme)) as unknown);
  }

  function status() {
    teamState = pending.size ? "working" : "ready";
    brandBooth();
    updatePendingRail();
  }

  function updatePendingRail() {
    if (!pending.size) {
      ui?.setStatus("booth-portal", "团队服务已连接");
      return;
    }
    const oldest = Math.min(...[...pending.values()].map(item => item.sent));
    const seconds = Math.max(0, Math.floor((Date.now() - oldest) / 1000));
    ui?.setStatus("booth-portal", `team · working · ${pending.size} messages · ${seconds}s`);
  }

  async function refreshTaskStatus(): Promise<void> {
    try {
      const response = await fetch(`${apiBase()}/orchestrator/api/portal?offset=0&limit=20`, {
        headers: { "x-orchestrator-request": "1" }, signal: AbortSignal.timeout(3000),
      });
      if (!response.ok) return;
      const view = await response.json() as { items?: Array<{ tasks?: Array<{ ticket?: { state?: string } }> }> };
      const tasks = (view.items || []).flatMap(item => item.tasks || []);
      const waiting = tasks.filter(task => (task.ticket?.state || "").includes("person") || (task.ticket?.state || "").includes("waiting")).length;
      const active = tasks.filter(task => {
        const state = task.ticket?.state || "";
        return state.includes("working") || state.includes("evaluating") || state.includes("accepted");
      }).length;
      if (pending.size) teamState = "working";
      else if (waiting) teamState = "waiting";
      else teamState = "ready";
      brandBooth();
      ui?.setStatus("booth-tasks", tasks.length
        ? `team · ${tasks.length} tasks · ${active} working · ${waiting} waiting`
        : "team · no delegated tasks");
    } catch {
      // The local transcript remains usable while the optional task summary is unavailable.
    }
  }

  async function taskCards(): Promise<string> {
    const response = await fetch(`${apiBase()}/orchestrator/api/portal?offset=0&limit=20`, {
      headers: { "x-orchestrator-request": "1" }, signal: AbortSignal.timeout(3000),
    });
    if (!response.ok) throw new Error(`任务视图暂时不可用（HTTP ${response.status}）`);
    const view = await response.json() as { items?: Array<{ tasks?: Array<{ id?: string; instruction?: string; ticket?: { title?: string; state?: string } }> }> };
    const tasks = (view.items || []).flatMap(item => item.tasks || []);
    if (!tasks.length) return "team tasks\n  no delegated tasks";
    const cardState = (state: string) => state.includes("person") || state.includes("waiting") ? "waiting"
      : state.includes("failed") || state.includes("blocked") ? "blocked"
      : state.includes("completed") || state.includes("merged") || state.includes("ended") ? "done"
      : state.includes("working") || state.includes("evaluating") || state.includes("accepted") ? "working" : "planned";
    const icon = (state: string) => state === "waiting" ? "◌" : state === "working" ? "●" : state === "blocked" ? "!" : state === "done" ? "✓" : "○";
    return ["team tasks", ...tasks.map(task => {
      const state = cardState(task.ticket?.state || "planned");
      const title = task.ticket?.title || task.instruction || task.id || "task";
      return `${icon(state)}  ${state.padEnd(8)}  [${task.id || "task"}]  ${title}`;
    })].join("\n");
  }

  void stat(outbox).then(s => { offset = s.size; }).catch(() => {});

  async function readiness(): Promise<string | undefined> {
    const base = apiBase();
    try {
      const response = await fetch(`${base.replace(/\/$/, "")}/orchestrator/api/state`, {
        headers: { "x-orchestrator-request": "1" }, signal: AbortSignal.timeout(3000),
      });
      if (!response.ok) return `管理服务暂时不可用（HTTP ${response.status}），请检查 booth logs。`;
      const state = await response.json() as { setup?: { problem?: { detail?: string } }; paused?: boolean; health?: Record<string, { state?: string; detail?: string }> };
      if (state.setup?.problem) return `团队服务尚未完成配置：${state.setup.problem.detail || "配置不完整"}。请使用 /booth-setup owner/repo（需要已登录 gh）。`;
      if (state.paused) return "团队服务已暂停，请先运行 booth resume。";
      const local = state.health?.local;
      if (local && local.state !== "ready") return `本地对话入口尚未就绪：${local.detail || local.state}。请检查 booth status 和 booth logs。`;
      const runtime = state.health?.runtime;
      if (runtime && runtime.state !== "ready") return `团队服务尚未就绪：${runtime.detail || runtime.state}。请检查 booth status 和 booth logs。`;
      return undefined;
    } catch (error) {
      return `无法连接团队服务：${error instanceof Error ? error.message : String(error)}。请检查 booth status 和 booth logs。`;
    }
  }

  async function autoSetup(): Promise<void> {
    try {
      await pi.exec(
        process.env.BOOTH_BINARY || "booth",
        ["--url", apiBase(), "setup"],
        { timeout: 180000 },
      );
    } catch {
      // Readiness below reports the actionable Git/GitHub error. Auto setup is
      // best effort so a non-Git working directory still opens the console.
    }
  }

  pi.on("session_start", async (_event, ctx) => {
    ui = ctx.ui;
    brandBooth();
    let problem = await readiness();
    if (problem) {
      await autoSetup();
      problem = await readiness();
    }
    if (problem) {
      teamState = "waiting";
      brandBooth();
      pi.sendMessage({ customType: "booth-portal-system", content: problem, display: true, attribution: "agent" }, { triggerTurn: false });
    } else status();
  });
  pi.on("session_switch", () => brandBooth());
  pi.on("session_clear", () => brandBooth());
  pi.on("session_branch", () => brandBooth());

  pi.registerCommand("booth-setup", {
    description: "Configure the current Git repository and local portal",
    handler: async (args, ctx) => {
      const match = args.trim().match(/^(?:([A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+))?(?:\s+(.+))?$/);
      if (!match) {
        ctx.ui.notify("用法：/booth-setup [owner/repo] [工作目录]；默认读取当前 Git 项目。", "warning");
        return;
      }
      ctx.ui.notify("正在配置团队服务，可能需要一点时间…", "info");
      try {
        const argv = ["--url", process.env.DSH_SUPERAGENT_URL || `http://127.0.0.1:${process.env.BOOTH_PORT || "3082"}`, "setup"];
        if (match[1]) argv.push(match[1]);
        if (match[2]) argv.push(match[2]);
        const result = await pi.exec(process.env.BOOTH_BINARY || "booth", argv, { timeout: 180000 });
        const output = [result.stdout, result.stderr].filter(Boolean).join("\n").trim();
        pi.sendMessage({ customType: "booth-portal-reply", content: output || (result.code === 0 ? "配置已完成。" : "配置失败，请检查 booth logs。"), display: true, attribution: "agent" }, { triggerTurn: false });
      } catch (error) {
        ctx.ui.notify(`配置失败：${error instanceof Error ? error.message : String(error)}`, "error");
      }
    },
  });

  pi.registerCommand("booth-tasks", {
    description: "Show delegated team tasks in the booth transcript",
    handler: async (_args, ctx) => {
      try {
        pi.sendMessage({ customType: "booth-task-card", content: await taskCards(), display: true, attribution: "agent" }, { triggerTurn: false });
      } catch (error) {
        ctx.ui.notify(`任务视图失败：${error instanceof Error ? error.message : String(error)}`, "error");
      }
    },
  });

  pi.on("input", async (event) => {
    const text = event.text.trim();
    if (!text || text.startsWith("/")) return;
    pi.sendMessage({ customType: "booth-portal-user", content: text, display: true, attribution: "user" }, { triggerTurn: false });
    const problem = await readiness();
    if (problem) {
      pi.sendMessage({ customType: "booth-portal-reply", content: `${problem}\n\n这条消息尚未发送，完成配置后请重新发送。`, display: true, attribution: "agent" }, { triggerTurn: false });
      return { handled: true };
    }
    try {
      const id = await send(text);
      pending.set(id, { sent: Date.now(), warned: false });
      status();
    }
    catch (error) {
      pi.sendMessage({ customType: "booth-portal-reply", content: `发送失败：${error instanceof Error ? error.message : String(error)}。请检查后重新发送。`, display: true, attribution: "agent" }, { triggerTurn: false });
    }
    return { handled: true };
  });

  pi.registerMessageRenderer("booth-portal-user", (message, _options, theme) =>
    new Text(theme.fg("accent", `you  ›  ${String(message.content)}`)));
  pi.registerMessageRenderer("booth-portal-system", (message, _options, theme) =>
    new Text(theme.fg("muted", `booth  ·  ${String(message.content)}`)));
  pi.registerMessageRenderer("booth-task-card", (message, _options, theme) =>
    new Text(theme.fg("accent", String(message.content))));
  pi.registerMessageRenderer("booth-portal-reply", (message, _options, theme) => {
    const kind = String((message as { kind?: unknown }).kind || "reply");
    const label = kind === "worker_report" || kind === "worker_attention" ? "team" : "booth";
    return new Text(theme.fg(label === "team" ? "accent" : "text", `${label}  ›  ${String(message.content)}`));
  });

  const poll = async () => {
    if (polling) return;
    polling = true;
    try {
      const data = await readFile(outbox, "utf8").catch(() => "");
      if (Buffer.byteLength(data) < offset) offset = 0;
      const bytes = Buffer.from(data);
      const fresh = bytes.subarray(offset).toString("utf8");
      const complete = fresh.lastIndexOf("\n");
      if (complete >= 0) {
        offset += Buffer.byteLength(fresh.slice(0, complete + 1));
        for (const line of fresh.slice(0, complete).split("\n")) {
          try {
            const reply = JSON.parse(line) as PortalReply;
            if (reply.text && (!reply.thread || reply.thread === activeThread)) {
              pi.sendMessage({ customType: "booth-portal-reply", content: reply.text, kind: reply.kind, display: true, attribution: "agent" }, { triggerTurn: false });
              pending.clear();
              status();
            }
          } catch { /* source may be writing a malformed/partial record */ }
        }
      }
      for (const item of pending.values()) {
        if (!item.warned && Date.now() - item.sent >= 30000) {
          item.warned = true;
          pi.sendMessage({ customType: "booth-portal-reply", content: "团队暂时还没有回复。请检查 DSH 中的模型和凭证配置；消息仍在处理中。", display: true, attribution: "agent" }, { triggerTurn: false });
        }
      }
    } finally { polling = false; }
  };
  let taskTick = 0;
  const interval = setInterval(() => {
    void poll();
    updatePendingRail();
    if (++taskTick % 2 === 0) void refreshTaskStatus();
  }, 1000);
  pi.on("session_shutdown", () => clearInterval(interval));
}
