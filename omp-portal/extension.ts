/**
 * OMP frontend adapter for dsh-booth.
 *
 * OMP supplies the interaction kernel; booth owns the terminal product and the
 * local team-agent bridge. This extension only bridges the portal JSONL source
 * and hides worker bookkeeping.
 */
import { appendFile, mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { ExtensionAPI } from "@oh-my-pi/pi-coding-agent";
import { Text } from "@oh-my-pi/pi-tui";

type PortalReply = { id?: string; text?: string; kind?: string; thread?: string };
type RelayPayload = {
  kind?: string;
  state?: string;
  summary?: string;
  ticket?: { ticket_id?: string; title?: string; state?: string };
};

function relayOf(text: string): RelayPayload | undefined {
  try {
    const value = JSON.parse(text) as RelayPayload;
    return value && ["worker_report", "worker_attention", "follow_up"].includes(value.kind || "") ? value : undefined;
  } catch {
    return undefined;
  }
}

function relayText(relay: RelayPayload): string {
  const title = relay.ticket?.title || relay.ticket?.ticket_id || "team task";
  if (relay.kind === "worker_attention") return `waiting for a decision · ${title}`;
  if (relay.kind === "follow_up") {
    const delivered = relay.ticket?.title || "worker task";
    return `team follow-up · ${delivered}`;
  }
  return `team update · ${title} · ${relay.state || relay.ticket?.state || "reported"}\n${relay.summary || "The team sent an update."}`;
}

const stateDir = process.env.BOOTH_STATE_DIR || join(process.env.HOME || ".", ".dsh", "state", process.env.BOOTH_PROFILE || "superagent");
const inbox = join(stateDir, "booth-inbox.jsonl");
const outbox = join(stateDir, "booth-outbox.jsonl");
const outboxCursor = join(stateDir, "booth-outbox.cursor");
const watchFile = join(stateDir, "booth-watch");
const conversationFile = join(stateDir, "booth-conversation");
const thread = `portal-${crypto.randomUUID()}`;
let activeThread = thread;
const apiBase = () => (process.env.DSH_SUPERAGENT_URL || `http://127.0.0.1:${process.env.BOOTH_PORT || "3082"}`).replace(/\/$/, "");
type BoothTask = {
  id?: string;
  instruction?: string;
  dependsOn?: string[];
  depends_on?: string[];
  ticket?: { ticket_id?: string; title?: string; state?: string };
};

async function portalTasks(maxPages = 20): Promise<BoothTask[]> {
  const tasks: BoothTask[] = [];
  let offset = 0;
  for (let page = 0; page < maxPages; page++) {
    const response = await fetch(`${apiBase()}/orchestrator/api/portal?offset=${offset}&limit=20`, {
      headers: { "x-orchestrator-request": "1" }, signal: AbortSignal.timeout(3000),
    });
    if (!response.ok) throw new Error(`任务视图暂时不可用（HTTP ${response.status}）`);
    const view = await response.json() as { items?: Array<{ tasks?: BoothTask[] }>; next_offset?: number | null };
    tasks.push(...(view.items || []).flatMap(item => item.tasks || []));
    const next = view.next_offset;
    if (typeof next !== "number" || next <= offset) break;
    offset = next;
  }
  return tasks;
}

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
  const offsetReady = (async () => {
    await mkdir(stateDir, { recursive: true });
    try {
      const saved = Number.parseInt((await readFile(outboxCursor, "utf8")).trim(), 10);
      if (Number.isSafeInteger(saved) && saved >= 0) {
        offset = saved;
        return;
      }
    } catch { /* first launch has no cursor yet */ }
    try { offset = (await stat(outbox)).size; } catch { /* outbox is created on first reply */ }
  })();
  let polling = false;
  let teamState: "ready" | "working" | "waiting" = "ready";
  const pending = new Map<string, { sent: number; warned: boolean }>();
  const taskSnapshot = new Map<string, { state: string; title: string }>();
  let taskSnapshotReady = false;
  let watchMode = true;
  const watchReady = (async () => {
    try { watchMode = (await readFile(watchFile, "utf8")).trim() !== "off"; }
    catch { /* first launch defaults to live team events */ }
  })();
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

  function updateWatchStatus() {
    ui?.setStatus("booth-watch", watchMode ? "watch · live team events" : undefined);
  }

  function status() {
    teamState = pending.size ? "working" : "ready";
    brandBooth();
    updateWatchStatus();
    updatePendingRail();
  }

  function settlePending(kind?: string) {
    // Worker relays are progress events for the portal, not answers to a local
    // message. Keep the queue active until the portal sends its reply. For a
    // portal reply, settle one message in FIFO order so concurrent requests
    // remain visible while the other ones are still running.
    if (kind === "worker_report" || kind === "worker_attention" || kind === "follow_up") return;
    const oldest = pending.keys().next();
    if (!oldest.done && oldest.value) pending.delete(oldest.value);
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

  const taskState = (state: string) => state.includes("person") || state.includes("waiting") ? "waiting"
    : state.includes("failed") || state.includes("blocked") ? "blocked"
    : state.includes("completed") || state.includes("merged") || state.includes("ended") ? "done"
    : state.includes("working") || state.includes("evaluating") || state.includes("accepted") ? "working" : "planned";

  function emitTaskEvents(tasks: Array<{ id?: string; instruction?: string; ticket?: { title?: string; state?: string } }>) {
    const current = new Map<string, { state: string; title: string }>();
    for (const task of tasks) {
      if (!task.id) continue;
      const state = taskState(task.ticket?.state || "planned");
      const title = task.ticket?.title || task.instruction || task.id;
      current.set(task.id, { state, title });
      const previous = taskSnapshot.get(task.id);
      if (watchMode && taskSnapshotReady && previous && previous.state !== state) {
        pi.sendMessage({
          customType: "booth-task-event",
          content: `team · ${task.id}\n${previous.state} → ${state}\n${title}`,
          display: true,
          attribution: "agent",
        }, { triggerTurn: false });
      }
    }
    taskSnapshot.clear();
    for (const [id, value] of current) taskSnapshot.set(id, value);
    taskSnapshotReady = true;
  }

  async function refreshTaskStatus(): Promise<void> {
    await watchReady;
    try {
      // The rail refreshes every two seconds; recent pages capture active work
      // without turning a large history into a background request storm. The
      // explicit task commands still walk every page.
      const tasks = await portalTasks(2);
      emitTaskEvents(tasks);
      const waiting = tasks.filter(task => taskState(task.ticket?.state || "planned") === "waiting").length;
      const active = tasks.filter(task => taskState(task.ticket?.state || "planned") === "working").length;
      const activeTitle = tasks.find(task => taskState(task.ticket?.state || "planned") === "working")?.ticket?.title;
      if (pending.size) teamState = "working";
      else if (waiting) teamState = "waiting";
      else teamState = "ready";
      brandBooth();
      ui?.setStatus("booth-tasks", tasks.length
        ? `team · ${tasks.length} tasks · ${active} working · ${waiting} waiting${activeTitle ? ` · ${activeTitle}` : ""}`
        : "team · no delegated tasks");
    } catch {
      // The local transcript remains usable while the optional task summary is unavailable.
    }
  }

  async function taskCards(): Promise<string> {
    const tasks = await portalTasks();
    if (!tasks.length) return "team tasks\n  no delegated tasks";
    const icon = (state: string) => state === "waiting" ? "◌" : state === "working" ? "●" : state === "blocked" ? "!" : state === "done" ? "✓" : "○";
    const order = ["working", "waiting", "blocked", "planned", "done"];
    const groups = new Map<string, BoothTask[]>();
    for (const task of tasks) {
      const state = taskState(task.ticket?.state || "planned");
      const group = groups.get(state) || [];
      group.push(task);
      groups.set(state, group);
    }
    const lines = ["team tasks"];
    for (const state of order) {
      const group = groups.get(state);
      if (!group?.length) continue;
      lines.push(`${state} · ${group.length}`);
      for (const task of group) {
        const title = task.ticket?.title || task.instruction || task.id || "task";
        lines.push(`  ${icon(state)}  [${task.id || "task"}]  ${title}`);
      }
    }
    return lines.join("\n");
  }

  async function taskDetail(taskId: string): Promise<string | undefined> {
    const task = (await portalTasks()).find(item => item.id === taskId);
    if (!task) return undefined;
    const rawState = task.ticket?.state || "planned";
    const state = taskState(rawState);
    const dependencies = task.dependsOn || task.depends_on || [];
    const lines = [
      `team task [${task.id || taskId}]`,
      `state: ${state}`,
      `title: ${task.ticket?.title || "untitled task"}`,
      `instruction: ${task.instruction || "(none)"}`,
    ];
    if (dependencies.length) lines.push(`depends on: ${dependencies.join(", ")}`);
    return lines.join("\n");
  }

  async function taskTicket(taskId: string): Promise<{ taskId: string; ticketId?: string } | undefined> {
    const task = (await portalTasks()).find(item => item.id === taskId || item.ticket?.ticket_id === taskId);
    return task?.id ? { taskId: task.id, ticketId: task.ticket?.ticket_id } : undefined;
  }

  async function boothNow(): Promise<string> {
    const lines = ["booth now", `team: ${teamState}`, `messages: ${pending.size}`];
    if (pending.size) {
      const oldest = Math.min(...[...pending.values()].map(item => item.sent));
      lines[2] += ` · oldest ${Math.max(0, Math.floor((Date.now() - oldest) / 1000))}s`;
    }
    try {
      const tasks = await portalTasks();
      const counts = new Map<string, number>();
      for (const task of tasks) {
        const state = taskState(task.ticket?.state || "planned");
        counts.set(state, (counts.get(state) || 0) + 1);
      }
      lines.push(`tasks: ${tasks.length} · ${counts.get("working") || 0} working · ${counts.get("waiting") || 0} waiting · ${counts.get("blocked") || 0} blocked · ${counts.get("planned") || 0} planned · ${counts.get("done") || 0} done`);
      const focus = tasks.filter(task => ["working", "waiting", "blocked"].includes(taskState(task.ticket?.state || "planned"))).slice(0, 4);
      if (focus.length) {
        lines.push("focus:");
        for (const task of focus) {
          const state = taskState(task.ticket?.state || "planned");
          lines.push(`  ${state} · [${task.id || "task"}] ${task.ticket?.title || task.instruction || "untitled task"}`);
        }
      } else lines.push("focus: no active team tasks");
    } catch (error) {
      lines.push(`tasks: unavailable · ${error instanceof Error ? error.message : String(error)}`);
    }
    return lines.join("\n");
  }

  async function saveOffset(): Promise<void> {
    await offsetReady;
    await writeFile(outboxCursor, `${offset}\n`);
  }

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
    await portalThread();
    await watchReady;
    ui = ctx.ui;
    brandBooth();
    updateWatchStatus();
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

  pi.registerCommand("booth-task", {
    description: "Show one delegated team task by id",
    handler: async (args, ctx) => {
      const taskId = args.trim();
      if (!taskId) {
        ctx.ui.notify("用法：/booth-task <任务 ID>；先运行 /booth-tasks 查看任务。", "warning");
        return;
      }
      try {
        const detail = await taskDetail(taskId);
        if (!detail) {
          ctx.ui.notify(`找不到任务 ${taskId}；请先运行 /booth-tasks 刷新任务列表。`, "warning");
          return;
        }
        pi.sendMessage({ customType: "booth-task-card", content: detail, display: true, attribution: "agent" }, { triggerTurn: false });
      } catch (error) {
        ctx.ui.notify(`任务详情失败：${error instanceof Error ? error.message : String(error)}`, "error");
      }
    },
  });

  pi.registerCommand("booth-now", {
    description: "Show the current booth team board",
    handler: async (_args, ctx) => {
      try {
        pi.sendMessage({ customType: "booth-now-card", content: await boothNow(), display: true, attribution: "agent" }, { triggerTurn: false });
      } catch (error) {
        ctx.ui.notify(`booth 总览失败：${error instanceof Error ? error.message : String(error)}`, "error");
      }
    },
  });

  pi.registerCommand("booth-watch", {
    description: "Toggle automatic team activity events",
    handler: async (args, ctx) => {
      await watchReady;
      const choice = args.trim().toLowerCase();
      if (choice && !["on", "off", "true", "false", "live", "mute"].includes(choice)) {
        ctx.ui.notify("用法：/booth-watch [on|off]；默认实时显示团队状态变化。", "warning");
        return;
      }
      watchMode = choice ? ["on", "true", "live"].includes(choice) : !watchMode;
      await mkdir(stateDir, { recursive: true });
      await writeFile(watchFile, `${watchMode ? "on" : "off"}\n`).catch(() => {});
      updateWatchStatus();
      pi.sendMessage({
        customType: "booth-watch-card",
        content: watchMode ? "watch · on\nteam state changes will appear automatically" : "watch · off\nteam state changes remain available in /booth-now and /booth-tasks",
        display: true,
        attribution: "agent",
      }, { triggerTurn: false });
    },
  });

  pi.registerCommand("booth-stop", {
    description: "Stop a delegated team task or ticket",
    handler: async (args, ctx) => {
      const requestedId = args.trim();
      if (!requestedId) {
        ctx.ui.notify("用法：/booth-stop <任务 ID>；先运行 /booth-tasks 查看任务。", "warning");
        return;
      }
      try {
        const task = await taskTicket(requestedId);
        if (!task) {
          ctx.ui.notify(`找不到任务或 ticket ${requestedId}；请先运行 /booth-tasks 刷新列表。`, "warning");
          return;
        }
        if (!task.ticketId) {
          ctx.ui.notify(`任务 ${task.taskId} 还没有启动 worker，无法停止。`, "warning");
          return;
        }
        const result = await pi.exec(process.env.BOOTH_BINARY || "booth", ["--url", apiBase(), "stop", task.ticketId], { timeout: 30000 });
        if (result.code !== 0) throw new Error([result.stdout, result.stderr].filter(Boolean).join("\n").trim() || `booth stop exited with ${result.code}`);
        pi.sendMessage({
          customType: "booth-task-control",
          content: `stopped · task [${task.taskId}] · ticket ${task.ticketId}`,
          display: true,
          attribution: "agent",
        }, { triggerTurn: false });
      } catch (error) {
        ctx.ui.notify(`停止任务失败：${error instanceof Error ? error.message : String(error)}`, "error");
      }
    },
  });

  pi.registerCommand("booth-retry", {
    description: "Retry a delegated team task or ticket",
    handler: async (args, ctx) => {
      const requestedId = args.trim();
      if (!requestedId) {
        ctx.ui.notify("用法：/booth-retry <任务 ID>；先运行 /booth-tasks 查看任务。", "warning");
        return;
      }
      try {
        const task = await taskTicket(requestedId);
        if (!task) {
          ctx.ui.notify(`找不到任务或 ticket ${requestedId}；请先运行 /booth-tasks 刷新列表。`, "warning");
          return;
        }
        if (!task.ticketId) {
          ctx.ui.notify(`任务 ${task.taskId} 还没有启动 worker，无法重试。`, "warning");
          return;
        }
        const result = await pi.exec(process.env.BOOTH_BINARY || "booth", ["--url", apiBase(), "retry", task.ticketId], { timeout: 30000 });
        if (result.code !== 0) throw new Error([result.stdout, result.stderr].filter(Boolean).join("\n").trim() || `booth retry exited with ${result.code}`);
        pi.sendMessage({
          customType: "booth-task-control",
          content: `retrying · task [${task.taskId}] · ticket ${task.ticketId}`,
          display: true,
          attribution: "agent",
        }, { triggerTurn: false });
      } catch (error) {
        ctx.ui.notify(`重试任务失败：${error instanceof Error ? error.message : String(error)}`, "error");
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
      pi.sendMessage({
        customType: "booth-portal-ack",
        content: `queued for team · ${id.slice(0, 8)}`,
        display: true,
        attribution: "agent",
      }, { triggerTurn: false });
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
  pi.registerMessageRenderer("booth-portal-ack", (message, _options, theme) =>
    new Text(theme.fg("muted", `booth  ›  ${String(message.content)}`)));
  pi.registerMessageRenderer("booth-task-card", (message, _options, theme) =>
    new Text(theme.fg("accent", String(message.content))));
  pi.registerMessageRenderer("booth-task-event", (message, _options, theme) =>
    new Text(theme.fg("accent", String(message.content))));
  pi.registerMessageRenderer("booth-now-card", (message, _options, theme) =>
    new Text(theme.fg("accent", String(message.content))));
  pi.registerMessageRenderer("booth-watch-card", (message, _options, theme) =>
    new Text(theme.fg("muted", String(message.content))));
  pi.registerMessageRenderer("booth-task-control", (message, _options, theme) =>
    new Text(theme.fg("accent", `booth  ›  ${String(message.content)}`)));
  pi.registerMessageRenderer("booth-portal-reply", (message, _options, theme) => {
    const kind = String((message as { kind?: unknown }).kind || "reply");
    const label = kind === "worker_report" || kind === "worker_attention" ? "team" : "booth";
    return new Text(theme.fg(label === "team" ? "accent" : "text", `${label}  ›  ${String(message.content)}`));
  });

  const poll = async () => {
    if (polling) return;
    polling = true;
    try {
      await offsetReady;
      const data = await readFile(outbox, "utf8").catch(() => "");
      if (Buffer.byteLength(data) < offset) offset = 0;
      const bytes = Buffer.from(data);
      const fresh = bytes.subarray(offset).toString("utf8");
      const complete = fresh.lastIndexOf("\n");
      let advanced = false;
      if (complete >= 0) {
        offset += Buffer.byteLength(fresh.slice(0, complete + 1));
        advanced = true;
        for (const line of fresh.slice(0, complete).split("\n")) {
          try {
            const reply = JSON.parse(line) as PortalReply;
            if (reply.text && (!reply.thread || reply.thread === activeThread)) {
              const relay = relayOf(reply.text);
              const kind = relay?.kind || reply.kind;
              pi.sendMessage({ customType: "booth-portal-reply", content: relay ? relayText(relay) : reply.text, kind, display: true, attribution: "agent" }, { triggerTurn: false });
              settlePending(kind);
              status();
            }
          } catch { /* source may be writing a malformed/partial record */ }
        }
      }
      if (advanced) void saveOffset().catch(() => {});
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
  pi.on("session_shutdown", () => {
    clearInterval(interval);
    void saveOffset().catch(() => {});
  });
}
