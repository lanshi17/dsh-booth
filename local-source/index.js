/**
 * The booth console's own Source kind.
 *
 * dsh-superagent accepts work only from a Source: GitHub, Feishu, or a kind a bundle adds. This
 * bundle adds one whose stream is a file the console appends to. A line in `<stateDir>/booth-inbox.jsonl`
 * is one instruction from the operator; the agent's acknowledgements, replies and notices are appended
 * to `<stateDir>/booth-outbox.jsonl`, which the console reads back.
 *
 * The Source declares every instruction as a person's direct message: it is `human`, `addressed: 'direct'`,
 * and its actor is the sender the line names (default `operator`). The Policy still decides whether that
 * sender may originate work, and Eval still decides whether the request is work the agent accepts.
 */

import { closeSync, existsSync, fstatSync, openSync, readSync, appendFileSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'

export const name = 'dsh-booth-local'
export const inject = ['orchestrator']

const KIND = 'local'
const HEALTH = 'local'
const CURSOR = 'booth:cursor'

/** Entries end with a newline, so a partial tail is left for the next read. */
const newline = 0x0a

function schema(parse) {
  return {
    parse,
    safeParse(value) {
      try {
        return { success: true, data: parse(value) }
      } catch (error) {
        return { success: false, error }
      }
    },
  }
}

/** An instruction line: `{id, text, at?, thread?, sender?}`. Unknown fields are kept out of the domain. */
const Line = schema(value => {
  if (typeof value !== 'object' || value === null) throw new Error('an instruction must be an object')
  if (typeof value.id !== 'string' || !value.id) throw new Error('an instruction needs an id')
  if (typeof value.text !== 'string') throw new Error('an instruction needs text')
  return value
})

const Config = schema(() => ({}))
const Policy = schema(value => ({
  allowed: Array.isArray(value?.allowed)
    ? value.allowed.filter(entry => entry && typeof entry.id === 'string' && entry.id)
    : [],
}))

/** One instruction as a Trigger; the console's own thread keeps follow-ups together. */
function triggerOf(line) {
  const id = line.id
  const thread = typeof line.thread === 'string' && line.thread ? line.thread : `t-${id}`
  return {
    id: `local-${id}`,
    source: KIND,
    threads: [{ source: KIND, id: thread }],
    actor: { source: KIND, id: typeof line.sender === 'string' && line.sender ? line.sender : 'operator' },
    human: true,
    addressed: 'direct',
    at: typeof line.at === 'string' && Number.isFinite(Date.parse(line.at)) ? new Date(Date.parse(line.at)).toISOString() : new Date().toISOString(),
    text: line.text,
    channel: { source: KIND, thread, address: {} },
    meta: { kind: 'instruction' },
  }
}

/** Reads the complete lines appended since `offset`; returns them and the offset after the last newline. */
function readLines(inbox, offset) {
  if (!existsSync(inbox)) return { lines: [], offset: 0 }
  const fd = openSync(inbox, 'r')
  try {
    const size = fstatSync(fd).size
    // A rewritten or truncated inbox has nothing to replay.
    if (size < offset) offset = 0
    if (size === offset) return { lines: [], offset }
    const buffer = Buffer.alloc(size - offset)
    const read = readSync(fd, buffer, 0, buffer.length, offset)
    const end = buffer.subarray(0, read).lastIndexOf(newline)
    if (end < 0) return { lines: [], offset }
    const text = buffer.subarray(0, end + 1).toString('utf8')
    const lines = []
    for (const raw of text.split('\n')) {
      if (!raw.trim()) continue
      try {
        lines.push(Line.parse(JSON.parse(raw)))
      } catch {
        // A malformed line is not an instruction; skip it without stopping the stream.
      }
    }
    return { lines, offset: offset + end + 1 }
  } finally {
    closeSync(fd)
  }
}

/** Appends the agent's outbound message to the outbox; the console reads it back by id. */
function reply(outbox, message) {
  mkdirSync(join(outbox, '..'), { recursive: true })
  appendFileSync(outbox, `${JSON.stringify({
    at: new Date().toISOString(),
    id: message.id,
    kind: message.kind,
    text: message.text,
    ...message.ticketId ? { ticket: message.ticketId } : {},
    ...message.channel?.thread ? { thread: message.channel.thread } : {},
  })}\n`)
}

/** Reports health, ignoring the throw a released registry entry causes during a restart. */
function report(source, state, detail) {
  try {
    source.report(HEALTH, { state, ...detail ? { detail } : {} })
  } catch {
    // The registry this context belonged to is being replaced; the supervisor reports again.
  }
}

async function watch(source, signal, files) {
  let cursor = source.state.get(CURSOR) ?? 0
  report(source, 'starting', 'Waiting for the deployment check')
  while (!signal.aborted) {
    if (!source.configured()) {
      report(source, 'paused', 'Waiting for setup: name the repository in /setup')
      await delay(1000, undefined, { signal }).catch(() => {})
      continue
    }
    // The Source's switch in Settings, as the other Sources read it. While the Source is off,
    // reading would consume instructions the Engine only records as filtered, so they would be
    // lost when it is turned on again; leaving them in the inbox keeps them for that resumption.
    if (!source.config(KIND)) {
      report(source, 'paused', 'Off')
      await delay(1000, undefined, { signal }).catch(() => {})
      continue
    }
    const { lines, offset } = readLines(files.inbox, cursor)
    if (!lines.length) {
      if (offset !== cursor) {
        cursor = offset
        source.state.set(CURSOR, cursor)
      }
      report(source, 'ready', 'Waiting for an instruction')
      await delay(files.pollMs, undefined, { signal }).catch(() => {})
      continue
    }
    for (const line of lines) {
      try {
        await source.receive(triggerOf(line))
      } catch (error) {
        report(source, 'error', `Could not deliver ${line.id}: ${error instanceof Error ? error.message : String(error)}`)
      }
    }
    cursor = offset
    source.state.set(CURSOR, cursor)
  }
}

export function apply(ctx, config = {}) {
  const stateDir = config.stateDir
  if (typeof stateDir !== 'string' || !stateDir) throw new Error('dsh-booth-local needs a stateDir')
  const files = {
    inbox: join(stateDir, 'booth-inbox.jsonl'),
    outbox: join(stateDir, 'booth-outbox.jsonl'),
    pollMs: typeof config.pollMs === 'number' && config.pollMs > 0 ? config.pollMs : 500,
  }

  const kind = {
    kind: KIND,
    title: { zh: 'booth 控制台', en: 'booth console' },
    description: {
      zh: '在 booth 控制台里输入的指令。控制台把它们写进一个本地队列，智能体在这里收到；它的回复也回到控制台。',
      en: 'Instructions typed in the booth console. The console appends them to a local queue the agent reads here, and replies come back the same way.',
    },
    schema: Config,
    fields: [],
    health: [HEALTH],
    policy: {
      schema: Policy,
      fields: [{
        key: 'allowed',
        type: 'rows',
        label: { zh: '能在控制台里发指令的人', en: 'People who may send instructions from the console' },
        columns: [{ key: 'id', type: 'text', label: { zh: '标识', en: 'ID' } }],
      }],
      explicit: {
        zh: '名单里的标识在控制台里输入的一条指令。',
        en: 'An instruction an ID on the list types into the console.',
      },
      rows: () => [{
        id: 'console',
        label: { zh: '控制台输入', en: 'Console input' },
        note: { zh: '全部', en: 'All' },
        cells: { listedUnaddressed: false, unlistedAddressed: true, unlistedUnaddressed: false },
      }],
    },
    channel: {
      async send(_host, message) {
        reply(files.outbox, message)
        return 'sent'
      },
      link: () => ({ url: '', label: { zh: 'booth', en: 'booth' } }),
    },
  }

  // dsh-superagent restarts its plugin (and with it the Source registry) on some settings
  // saves, which would leave a kind registered once and then lost. A small supervisor keeps
  // the kind registered against whichever registry is current, and restarts the watcher with it.
  let host = undefined
  let controller
  const teardown = () => {
    if (controller) controller.abort()
    controller = undefined
    host = undefined
  }
  const sync = () => {
    const orchestrator = ctx.get('orchestrator')
    if (!orchestrator) {
      teardown()
      return
    }
    const live = host === orchestrator.sources && !!orchestrator.sources.kind(KIND)
    if (live && controller && !controller.signal.aborted) return
    if (!live) {
      teardown()
      host = orchestrator.sources
      orchestrator.sources.register(kind)
    }
    // Registering and watching are separate steps: the watcher is restarted when its own
    // registration survived but the loop ended.
    const source = orchestrator.context(kind)
    controller = new AbortController()
    watch(source, controller.signal, files)
      .catch(error => { report(source, 'error', String(error)) })
  }
  const timer = setInterval(() => {
    try {
      sync()
    } catch {
      // The service is between instances; the next tick tries again.
    }
  }, 1000)
  try {
    sync()
  } catch {
    // The orchestrator may not be mounted yet; the timer registers the kind.
  }
  ctx.effect(() => () => {
    clearInterval(timer)
    teardown()
  }, 'booth: local source supervision')
}

