/**
 * OMP frontend adapter for dsh-booth.
 *
 * OMP remains the terminal UI; booth remains the local team-agent kernel. This
 * extension only bridges the portal JSONL source and hides worker bookkeeping.
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

async function portalThread(): Promise<string> {

  await mkdir(stateDir, { recursive: true });
  try { activeThread = (await readFile(conversationFile, "utf8")).trim() || thread; }
  catch { await appendFile(conversationFile, thread); }
  return activeThread;
}

async function send(text: string): Promise<void> {
  const record = { id: crypto.randomUUID(), text: text.trim(), sender: "operator", thread: await portalThread() };
  await appendFile(inbox, `${JSON.stringify(record)}\n`);
}

export default function boothPortal(pi: ExtensionAPI) {
  let offset = 0;
  let polling = false;

  void stat(outbox).then(s => { offset = s.size; }).catch(() => {});

  pi.on("input", async (event) => {
    const text = event.text.trim();
    if (!text || text.startsWith("/")) return;
    await send(text);
    pi.sendMessage({ customType: "booth-portal-user", content: text, display: true, attribution: "user" }, { triggerTurn: false });
    return { handled: true };
  });

  pi.registerMessageRenderer("booth-portal-user", (message, _options, theme) =>
    new Text(theme.fg("accent", `you  ${String(message.content)}`)));
  pi.registerMessageRenderer("booth-portal-reply", (message, _options, theme) =>
    new Text(theme.fg("text", String(message.content))));

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
              pi.sendMessage({ customType: "booth-portal-reply", content: reply.text, display: true, attribution: "agent" }, { triggerTurn: false });
            }
          } catch { /* source may be writing a malformed/partial record */ }
        }
      }
    } finally { polling = false; }
  };
  setInterval(() => { void poll(); }, 500);
}
