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

async function watch(source, signal, files) {
  let cursor = source.state.get(CURSOR) ?? 0
  source.report(HEALTH, { state: 'starting', detail: 'Waiting for the deployment check' })
  while (!signal.aborted) {
    if (!source.configured()) {
      source.report(HEALTH, { state: 'paused', detail: 'Waiting for setup: name the repository in /setup' })
      await delay(1000, undefined, { signal }).catch(() => {})
      continue
    }
    const { lines, offset } = readLines(files.inbox, cursor)
    if (!lines.length) {
      if (offset !== cursor) {
        cursor = offset
        source.state.set(CURSOR, cursor)
      }
      source.report(HEALTH, { state: 'ready', detail: 'Waiting for an instruction' })
      await delay(files.pollMs, undefined, { signal }).catch(() => {})
      continue
    }
    for (const line of lines) {
      try {
        await source.receive(triggerOf(line))
      } catch (error) {
        source.report(HEALTH, { state: 'error', detail: `Could not deliver ${line.id}: ${error instanceof Error ? error.message : String(error)}` })
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

  ctx.effect(() => ctx.orchestrator.sources.register(kind), 'booth: local source kind')
  ctx.effect(() => {
    const source = ctx.orchestrator.context(kind)
    const controller = new AbortController()
    const running = watch(source, controller.signal, files)
      .catch(error => { source.report(HEALTH, { state: 'error', detail: String(error) }) })
    return async () => { controller.abort(); await running }
  })
}
