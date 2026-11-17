/**
 * Booth portal adapter for the team service.
 *
 * The interaction kernel supplies the editor; booth owns the terminal product and the
 * local team-agent bridge. This extension only bridges the portal JSONL source
 * and hides worker bookkeeping.
 */
import { appendFile, mkdir, readFile, stat, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { ExtensionAPI, ExtensionUIContext } from "@oh-my-pi/pi-coding-agent";
import { Text } from "@oh-my-pi/pi-tui";

type PortalReply = { id?: string; text?: string; kind?: string; thread?: string };
type RelayPayload = {
  kind?: string;
  state?: string;
  summary?: string;
  ticket?: {
    ticket_id?: string;
    title?: string;
    state?: string;
    attention?: { question?: string };
  };
};

type TicketAssignment = {
  state?: string;
  ended_reason?: string;
  report?: { state?: string; preview?: string; at?: string };
};

type TicketDetail = {
  ticket_id?: string;
  title?: string;
  state?: string;
  attention?: { question?: string };
  assignments?: { items?: TicketAssignment[] };
};

function relayOf(text: string): RelayPayload | undefined {
  try {
    const value = JSON.parse(text) as RelayPayload;
    return value && ["worker_report", "worker_attention", "follow_up"].includes(value.kind || "") ? value : undefined;
  } catch {
    return undefined;
  }
}

async function relayText(relay: RelayPayload): Promise<string> {
  const title = relay.ticket?.title || relay.ticket?.ticket_id || "team task";
  if (relay.kind === "worker_attention") {
    let question = relay.ticket?.attention?.question;
    const ticketId = relay.ticket?.ticket_id;
    if (!question && ticketId) {
      try {
        const response = await fetch(
          `${apiBase()}/orchestrator/api/tickets/${encodeURIComponent(ticketId)}`,
          { headers: { "x-orchestrator-request": "1" }, signal: AbortSignal.timeout(3000) },
        );
        if (response.ok) {
          const ticket = await response.json() as { attention?: { question?: string } };
          question = ticket.attention?.question;
        }
      } catch {
        // The relay remains useful without the optional detail; the next task poll retries it.
      }
    }
    return [
      "needs your decision",
      title,
      ticketId && ticketId !== title ? `task · ${ticketId}` : "",
      question || "The team is waiting for your direction.",
      "reply · /booth-answer",
    ].filter(Boolean).join("\n");
  }
  if (relay.kind === "follow_up") {
    return `follow-up routed to the team\n${title}`;
  }
  const state = relay.state || relay.ticket?.state || "reported";
  const label = ["completed", "merged", "ready", "done"].includes(state)
    ? "finished"
    : ["blocked", "failed"].includes(state)
      ? state
      : "working update";
  return `${label}\n${title}\n${relay.summary || "The team sent an update."}`;
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
  conversationId?: string;
  conversationUpdatedAt?: string;
  instruction?: string;
  dependsOn?: string[];
  depends_on?: string[];
  ticket?: {
    ticket_id?: string;
    title?: string;
    state?: string;
    attention?: { question?: string };
  };
};

type TaskPulse = {
  total: number;
  working: number;
  waiting: number;
  blocked: number;
  focus?: string;
  focusId?: string;
};

type PortalMessageKind = "instruction" | "answer";

async function portalTasks(maxPages = 20): Promise<BoothTask[]> {
  const tasks: BoothTask[] = [];
  let offset = 0;
  for (let page = 0; page < maxPages; page++) {
    const response = await fetch(`${apiBase()}/orchestrator/api/portal?offset=${offset}&limit=20`, {
      headers: { "x-orchestrator-request": "1" }, signal: AbortSignal.timeout(3000),
    });
    if (!response.ok) throw new Error(`任务视图暂时不可用（HTTP ${response.status}）`);
    const view = await response.json() as { items?: Array<{ id?: string; updatedAt?: string; tasks?: BoothTask[] }>; next_offset?: number | null };
    tasks.push(...(view.items || []).flatMap(item => (item.tasks || []).map(task => ({
      ...task,
      conversationId: item.id,
      conversationUpdatedAt: item.updatedAt,
    }))));
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
  let taskPulse: TaskPulse = { total: 0, working: 0, waiting: 0, blocked: 0 };
  const taskSnapshot = new Map<string, { state: string; title: string }>();
  let taskSnapshotReady = false;
  let watchMode = true;
  let welcomeShown = false;
  const watchReady = (async () => {
    try { watchMode = (await readFile(watchFile, "utf8")).trim() !== "off"; }
    catch { /* first launch defaults to live team events */ }
  })();
  let ui: Pick<ExtensionUIContext, "setStatus" | "setTitle" | "setHeader"> | undefined;

  // The interaction kernel supplies editor and terminal mechanics; booth owns the product
  // identity shown in the terminal title.
  function renderHeader(theme: { fg: (name: string, text: string) => string }) {
    const state = teamState === "working" ? "working" : teamState === "waiting" ? "waiting" : "ready";
    return new Text(theme.fg("accent", " booth ") + theme.fg("muted", `  team agents · ${state}`));
  }

  function brandBooth() {
    ui?.setTitle("booth");
    ui?.setStatus("booth-brand", `booth  ·  team agents  ·  ${teamState}`);
    updatePulseStatus();
    // The kernel's built-in header carries its own product name. Replace it with the
    // booth identity while leaving the editor, palette, history and shortcuts
    // supplied by the interaction kernel intact.
    ui?.setHeader?.(((_tui: unknown, theme: { fg: (name: string, text: string) => string }) => renderHeader(theme)) as unknown);
  }

  function updateWatchStatus() {
    ui?.setStatus("booth-watch", watchMode ? "watch · live team events" : undefined);
  }

  function showWelcome() {
    if (welcomeShown) return;
    welcomeShown = true;
    pi.sendMessage({
      customType: "booth-welcome",
      content: "booth · team console\none portal · many agents\ntry /booth-focus when you want the next move",
      display: true,
      attribution: "agent",
    }, { triggerTurn: false });
  }

  function pulseText(): string {
    if (taskPulse.waiting > 0) {
      return `team · ${taskPulse.waiting} needs you · /booth-focus`;
    }
    if (pending.size) {
      const oldest = Math.min(...[...pending.values()].map(item => item.sent));
      const seconds = Math.max(0, Math.floor((Date.now() - oldest) / 1000));
      return `portal · ${pending.size} queued · ${seconds}s · /booth-focus`;
    }
    if (taskPulse.working > 0) {
      return `team · ${taskPulse.working} working${taskPulse.focus ? ` · ${taskPulse.focus}` : ""}`;
    }
    if (teamState === "waiting") return "team · waiting for you";
    return "team · ready";
  }

  function updatePulseStatus() {
    ui?.setStatus("booth-pulse", pulseText());
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
    updatePulseStatus();
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
      const activeTask = tasks.find(task => taskState(task.ticket?.state || "planned") === "working");
      const activeTitle = activeTask?.ticket?.title;
      const blocked = tasks.filter(task => taskState(task.ticket?.state || "planned") === "blocked").length;
      taskPulse = {
        total: tasks.length,
        working: active,
        waiting,
        blocked,
        focus: activeTitle,
        focusId: activeTask?.id,
      };
      if (pending.size) teamState = "working";
      else if (waiting) teamState = "waiting";
      else teamState = "ready";
      brandBooth();
      updatePulseStatus();
    } catch {
      // The local transcript remains usable while the optional task summary is unavailable.
    }
  }

  /**
   * Explain a silent portal turn with live backend evidence. A queued line by itself is not a
   * useful status: the operator needs to know whether the portal has not started, a worker is
   * running, a worker is idle, or the runtime/model is unhealthy.
   */
  async function diagnosePending(): Promise<string> {
    const oldest = pending.size ? Math.min(...[...pending.values()].map(item => item.sent)) : Date.now();
    const age = Math.max(0, Math.floor((Date.now() - oldest) / 1000));
    const lines = [
      "booth check",
      pending.size
        ? `portal · ${pending.size} message${pending.size === 1 ? "" : "s"} waiting · ${age}s`
        : "portal · no message is waiting; inspecting the live team board",
    ];
    const [stateResult, overviewResult, tasksResult] = await Promise.allSettled([
      fetch(`${apiBase()}/orchestrator/api/state`, {
        headers: { "x-orchestrator-request": "1" }, signal: AbortSignal.timeout(3000),
      }),
      fetch(`${apiBase()}/orchestrator/api/overview`, {
        headers: { "x-orchestrator-request": "1" }, signal: AbortSignal.timeout(3000),
      }),
      portalTasks(4),
    ]);

    let healthShown = false;
    if (stateResult.status === "fulfilled" && stateResult.value.ok) {
      const state = await stateResult.value.json() as {
        paused?: boolean;
        health?: Record<string, { state?: string; detail?: string }>;
      };
      if (state.paused) lines.push("runtime · paused");
      for (const [name, health] of Object.entries(state.health || {})) {
        if (health.state && health.state !== "ready") {
          lines.push(`health · ${name} · ${health.state}${health.detail ? ` · ${health.detail}` : ""}`);
          healthShown = true;
        }
      }
      if (!healthShown && !state.paused) lines.push("runtime · healthy; checking portal and workers");
    } else if (stateResult.status === "rejected") {
      lines.push(`runtime · unavailable · ${stateResult.reason instanceof Error ? stateResult.reason.message : String(stateResult.reason)}`);
    } else {
      lines.push("runtime · management API did not answer");
    }

    if (overviewResult.status === "fulfilled" && overviewResult.value.ok) {
      const overview = await overviewResult.value.json() as {
        executor?: { running?: number; queued?: number; maxWorkers?: number };
        problems?: Array<{ kind?: string; name?: string; detail?: string }>;
      };
      const load = overview.executor;
      if (load) lines.push(`executor · ${load.running || 0} running · ${load.queued || 0} queued${load.maxWorkers ? ` · ${load.maxWorkers} max` : ""}`);
      for (const problem of (overview.problems || []).slice(0, 4)) {
        const label = problem.kind || problem.name || "runtime";
        const detail = problem.detail ? ` · ${problem.detail}` : "";
        if (!lines.some(line => line.includes(`health · ${label}`) && line.includes(problem.detail || "\u0000"))) {
          lines.push(`problem · ${label}${detail}`);
        }
      }
    } else if (overviewResult.status === "rejected") {
      lines.push(`overview · unavailable · ${overviewResult.reason instanceof Error ? overviewResult.reason.message : String(overviewResult.reason)}`);
    }

    let tasks: BoothTask[] = [];
    if (tasksResult.status === "fulfilled") tasks = tasksResult.value;
    else lines.push(`team board · unavailable · ${tasksResult.reason instanceof Error ? tasksResult.reason.message : String(tasksResult.reason)}`);

    const active = tasks
      .filter(task => !["done"].includes(taskState(task.ticket?.state || "planned")))
      .sort((a, b) => (b.conversationUpdatedAt || "").localeCompare(a.conversationUpdatedAt || ""));
    if (!active.length) {
      lines.push("portal · no delegated task is recorded; the portal has not completed its turn yet");
      lines.push("next · check the model/session health, then retry the message");
    } else {
      lines.push("team · live worker state");
      const details = await Promise.all(active.slice(0, 6).map(async task => {
        const cardState = taskState(task.ticket?.state || "planned");
        const title = task.ticket?.title || task.instruction || task.id || "task";
        let detail = "";
        const ticketId = task.ticket?.ticket_id;
        if (ticketId) {
          try {
            const response = await fetch(`${apiBase()}/orchestrator/api/tickets/${encodeURIComponent(ticketId)}`, {
              headers: { "x-orchestrator-request": "1" }, signal: AbortSignal.timeout(3000),
            });
            if (response.ok) {
              const ticket = await response.json() as TicketDetail;
              const assignment = ticket.assignments?.items?.[0];
              if (assignment?.state) detail = ` · worker ${assignment.state}`;
              if (assignment?.report?.state) detail += ` · report ${assignment.report.state}`;
              if (ticket.attention?.question) detail += " · needs your decision";
            }
          } catch {
            detail = " · ticket detail unavailable";
          }
        }
        return `  ${cardState} · [${task.id || ticketId || "task"}] ${title}${detail}`;
      }));
      lines.push(...details);
      const idle = active.some(task => taskState(task.ticket?.state || "planned") === "planned");
      lines.push(idle
        ? "portal · a task is planned or idle; it must inspect the ticket and request a worker status"
        : "portal · worker activity is visible; the portal will answer when its relay arrives");
    }
    lines.push("commands · /booth-now  /booth-tasks  /booth-focus");
    return lines.join("\n");
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
        if (state === "waiting") {
          if (task.ticket?.attention?.question) lines.push(`       ? ${task.ticket.attention.question}`);
          lines.push("       ↳ /booth-answer");
        }
      }
    }
    return lines.join("\n");
  }

  async function taskDetail(taskId: string): Promise<string | undefined> {
    const task = (await portalTasks()).find(item => item.id === taskId || item.ticket?.ticket_id === taskId);
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
    if (task.ticket?.attention?.question) {
      lines.push(`question: ${task.ticket.attention.question}`);
      lines.push("answer with: /booth-answer");
    }
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

  /**
   * Turn the team's current state into one small, actionable card. The board is useful when a
   * person wants inventory; focus is for the much more common question: "what should I do now?"
   */
  async function boothFocus(): Promise<string> {
    await refreshTaskStatus();
    if (taskPulse.waiting > 0) {
      const tasks = await portalTasks();
      const task = tasks.find(item => taskState(item.ticket?.state || "planned") === "waiting");
      const title = task?.ticket?.title || task?.instruction || task?.id || "team task";
      const question = task?.ticket?.attention?.question || "The team is waiting for your direction.";
      return [
        "booth focus",
        "next · answer the team",
        title,
        `? ${question}`,
        "run · /booth-answer",
      ].join("\n");
    }
    if (pending.size) {
      const oldest = Math.min(...[...pending.values()].map(item => item.sent));
      const seconds = Math.max(0, Math.floor((Date.now() - oldest) / 1000));
      return [
        "booth focus",
        "next · let the portal work",
        `${pending.size} message${pending.size === 1 ? "" : "s"} queued · oldest ${seconds}s`,
        "run · /booth-now for the live board",
      ].join("\n");
    }
    if (taskPulse.working > 0) {
      return [
        "booth focus",
        "next · keep an eye on the team",
        taskPulse.focusId ? `[${taskPulse.focusId}] ${taskPulse.focus || "active team task"}` : "active team work",
        "run · /booth-tasks for task cards",
      ].join("\n");
    }
    if (taskPulse.blocked > 0) {
      return [
        "booth focus",
        "next · inspect a blocked task",
        `${taskPulse.blocked} blocked task${taskPulse.blocked === 1 ? "" : "s"}`,
        "run · /booth-tasks",
      ].join("\n");
    }
    return [
      "booth focus",
      "next · give the team a direction",
      "type an instruction, or / for commands",
    ].join("\n");
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
      if (state.setup?.problem) return setupProblemText(state.setup.problem.detail || "配置不完整");
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

  function setupProblemText(detail: string): string {
    if (/rate limit exceeded|api rate limit/i.test(detail)) {
      return `GitHub API 当前触发限流：${detail}\n\n请先运行 gh auth status；如果 token 已失效，运行 gh auth login -h github.com（或 gh auth refresh），然后重试 /booth-setup owner/repo。认证有效时请等待 GitHub 限流窗口恢复。`;
    }
    if (/cannot read its account|bad credentials|authentication token|token is invalid/i.test(detail)) {
      return `GitHub 认证不可用：${detail}\n\n请运行 gh auth status，并在需要时运行 gh auth login -h github.com（或 gh auth refresh），然后重试 /booth-setup owner/repo。`;
    }
    return `团队服务尚未完成配置：${detail}。请使用 /booth-setup owner/repo（需要已登录 gh）。`;
  }

  async function queuePortalMessage(text: string, kind: PortalMessageKind): Promise<boolean> {
    const message = text.trim();
    if (!message) return false;
    pi.sendMessage({
      customType: "booth-portal-user",
      content: message,
      details: { kind },
      display: true,
      attribution: "user",
    }, { triggerTurn: false });
    const problem = await readiness();
    if (problem) {
      const noun = kind === "answer" ? "回答" : "消息";
      pi.sendMessage({
        customType: "booth-portal-reply",
        content: `${problem}\n\n这条${noun}尚未发送，完成配置后请重新发送。`,
        display: true,
        attribution: "agent",
      }, { triggerTurn: false });
      return false;
    }
    try {
      const id = await send(message);
      pending.set(id, { sent: Date.now(), warned: false });
      const queued = kind === "answer" ? "answer queued for team" : "queued for team";
      pi.sendMessage({
        customType: "booth-portal-ack",
        content: `${queued} · ${id.slice(0, 8)}`,
        details: { kind },
        display: true,
        attribution: "agent",
      }, { triggerTurn: false });
      status();
      return true;
    } catch (error) {
      pi.sendMessage({
        customType: "booth-portal-reply",
        content: `发送失败：${error instanceof Error ? error.message : String(error)}。请检查后重新发送。`,
        display: true,
        attribution: "agent",
      }, { triggerTurn: false });
      return false;
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
    showWelcome();
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

  pi.registerCommand("booth-answer", {
    description: "Answer the team's current human question",
    handler: async (args, ctx) => {
      const answer = args.trim();
      if (!answer) {
        try {
          const task = (await portalTasks()).find(item => taskState(item.ticket?.state || "planned") === "waiting");
          if (!task) {
            ctx.ui.notify("当前没有等待你的团队问题。运行 /booth-tasks 查看任务。", "info");
            return;
          }
          const title = task.ticket?.title || task.instruction || task.id || "team task";
          const question = task.ticket?.attention?.question || "The team is waiting for your direction.";
          pi.sendMessage({
            customType: "booth-answer-prompt",
            content: `needs your decision\n${title}\n${question}`,
            display: true,
            attribution: "agent",
          }, { triggerTurn: false });
          const prompted = await ctx.ui.input("booth answer", "type the decision for the team");
          if (prompted?.trim()) await queuePortalMessage(prompted, "answer");
        } catch (error) {
          ctx.ui.notify(`回答窗口失败：${error instanceof Error ? error.message : String(error)}`, "error");
        }
        return;
      }
      await queuePortalMessage(answer, "answer");
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

  pi.registerCommand("booth-focus", {
    description: "Show the single next action for the booth team",
    handler: async (_args, ctx) => {
      try {
        pi.sendMessage({ customType: "booth-focus-card", content: await boothFocus(), display: true, attribution: "agent" }, { triggerTurn: false });
      } catch (error) {
        ctx.ui.notify(`booth focus 失败：${error instanceof Error ? error.message : String(error)}`, "error");
      }
    },
  });

  pi.registerCommand("booth-check", {
    description: "Inspect the portal, runtime health and delegated workers",
    handler: async (_args, ctx) => {
      try {
        if (!pending.size) {
          await refreshTaskStatus();
          pi.sendMessage({ customType: "booth-check-card", content: await diagnosePending(), display: true, attribution: "agent" }, { triggerTurn: false });
          return;
        }
        pi.sendMessage({ customType: "booth-check-card", content: await diagnosePending(), display: true, attribution: "agent" }, { triggerTurn: false });
      } catch (error) {
        ctx.ui.notify(`团队检查失败：${error instanceof Error ? error.message : String(error)}`, "error");
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
    await queuePortalMessage(text, "instruction");
    return { handled: true };
  });

  pi.registerMessageRenderer("booth-portal-user", (message, _options, theme) => {
    const kind = String((message as { details?: { kind?: unknown } }).details?.kind || "instruction");
    const label = kind === "answer" ? "answer" : String(message.content);
    const content = kind === "answer" ? `you  ›  answer\n${String(message.content)}` : `you  ›  ${label}`;
    return new Text(theme.fg("accent", content));
  });
  pi.registerMessageRenderer("booth-portal-system", (message, _options, theme) =>
    new Text(theme.fg("muted", `booth  ·  ${String(message.content)}`)));
  pi.registerMessageRenderer("booth-welcome", (message, _options, theme) =>
    new Text(theme.fg("accent", String(message.content))));
  pi.registerMessageRenderer("booth-answer-prompt", (message, _options, theme) =>
    new Text(theme.fg("warning", `${String(message.content)}\nreply in the booth answer window`)));
  pi.registerMessageRenderer("booth-portal-ack", (message, _options, theme) => {
    return new Text(theme.fg("muted", `booth  ›  ${String(message.content)}`));
  });
  pi.registerMessageRenderer("booth-task-card", (message, _options, theme) =>
    new Text(theme.fg("accent", String(message.content))));
  pi.registerMessageRenderer("booth-task-event", (message, _options, theme) =>
    new Text(theme.fg("accent", String(message.content))));
  pi.registerMessageRenderer("booth-now-card", (message, _options, theme) =>
    new Text(theme.fg("accent", String(message.content))));
  pi.registerMessageRenderer("booth-focus-card", (message, _options, theme) =>
    new Text(theme.fg("warning", String(message.content))));
  pi.registerMessageRenderer("booth-check-card", (message, _options, theme) =>
    new Text(theme.fg("warning", String(message.content))));
  pi.registerMessageRenderer("booth-watch-card", (message, _options, theme) =>
    new Text(theme.fg("muted", String(message.content))));
  pi.registerMessageRenderer("booth-task-control", (message, _options, theme) =>
    new Text(theme.fg("accent", `booth  ›  ${String(message.content)}`)));
  pi.registerMessageRenderer("booth-portal-reply", (message, _options, theme) => {
    const kind = String((message as { details?: { kind?: unknown } }).details?.kind || "reply");
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
              pi.sendMessage({ customType: "booth-portal-reply", content: relay ? await relayText(relay) : reply.text, details: { kind }, display: true, attribution: "agent" }, { triggerTurn: false });
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
          try {
            pi.sendMessage({ customType: "booth-check-card", content: await diagnosePending(), display: true, attribution: "agent" }, { triggerTurn: false });
          } catch (error) {
            pi.sendMessage({
              customType: "booth-portal-reply",
              content: `门户超过 30 秒没有回复，自动检查失败：${error instanceof Error ? error.message : String(error)}。运行 /booth-check 重试。`,
              display: true,
              attribution: "agent",
            }, { triggerTurn: false });
          }
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
