import test from 'node:test'
import assert from 'node:assert/strict'
import { appendFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'

import { apply } from '../index.js'

/** A booth Source whose enabled switch, inbox and stored state the test controls. */
function booth({ enabled = { value: true }, cursor } = {}) {
  const received = []
  const reports = []
  const values = new Map()
  if (cursor !== undefined) values.set('booth:cursor', cursor)
  const source = {
    configured: () => true,
    config: () => (enabled.value ? {} : undefined),
    receive: async trigger => {
      received.push(trigger)
      return 'stored'
    },
    state: {
      get: key => values.get(key),
      set: (key, value) => {
        if (value === undefined) values.delete(key)
        else values.set(key, value)
      },
    },
    report: (_entry, health) => { reports.push(health) },
  }
  const orchestrator = {
    sources: {
      kinds: new Map(),
      kind(id) { return this.kinds.get(id) },
      register(kind) { this.kinds.set(kind.kind, kind) },
    },
    context: () => source,
  }
  const cleanups = []
  const ctx = {
    get: name => (name === 'orchestrator' ? orchestrator : undefined),
    effect: work => { cleanups.push(work()) },
  }
  return {
    ctx,
    received,
    reports,
    stop: () => { for (const cleanup of cleanups) cleanup() },
  }
}

async function eventually(predicate, timeout = 2000) {
  const deadline = Date.now() + timeout
  while (Date.now() < deadline) {
    if (predicate()) return true
    await delay(10)
  }
  return predicate()
}

function line(id, text) {
  return `${JSON.stringify({ id, text })}\n`
}

test('an enabled console Source delivers an instruction', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'booth-local-'))
  const console = booth()
  try {
    writeFileSync(join(dir, 'booth-inbox.jsonl'), line('one', 'hi'))
    apply(console.ctx, { stateDir: dir, pollMs: 5 })
    assert.ok(await eventually(() => console.received.length === 1), 'the instruction is delivered')
    assert.equal(console.received[0].id, 'local-one')
    assert.equal(console.received[0].source, 'local')
    assert.equal(console.received[0].actor.id, 'operator')
  } finally {
    console.stop()
    rmSync(dir, { recursive: true, force: true })
  }
})

test('a console instruction uses the admission row its policy describes', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'booth-local-'))
  const console = booth()
  try {
    writeFileSync(join(dir, 'booth-inbox.jsonl'), line('one', 'hi'))
    apply(console.ctx, { stateDir: dir, pollMs: 5 })
    assert.ok(await eventually(() => console.received.length === 1), 'the instruction is delivered')

    const kind = console.ctx.get('orchestrator').sources.kind('local')
    const rows = kind.policy.rows({})
    assert.deepEqual(rows.map(row => row.id), ['console'], 'the policy declares the console admission row')
    assert.equal(typeof kind.scope, 'function', 'the Source declares the receiving scope those rows describe')
    assert.deepEqual(
      kind.scope({ meta: console.received[0].meta }, {}),
      { listened: true, observed: false, row: 'console' },
      'the instruction is judged by the console admission cells, not by the default cells',
    )
  } finally {
    console.stop()
    rmSync(dir, { recursive: true, force: true })
  }
})

test('an instruction waits while the Source is off', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'booth-local-'))
  const enabled = { value: false }
  const console = booth({ enabled })
  try {
    writeFileSync(join(dir, 'booth-inbox.jsonl'), line('one', 'hi'))
    apply(console.ctx, { stateDir: dir, pollMs: 5 })
    await delay(150)
    assert.equal(console.received.length, 0, 'a Source that is off does not read its inbox')
    assert.ok(
      console.reports.some(report => report.state === 'paused' && report.detail === 'Off'),
      'the off Source reports paused',
    )

    enabled.value = true
    assert.ok(
      await eventually(() => console.received.length === 1),
      'the instruction typed while off arrives once the Source is on',
    )
  } finally {
    console.stop()
    rmSync(dir, { recursive: true, force: true })
  }
})

test('an appended instruction is read without replaying the earlier one', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'booth-local-'))
  const inbox = join(dir, 'booth-inbox.jsonl')
  const console = booth()
  try {
    writeFileSync(inbox, line('one', 'first'))
    apply(console.ctx, { stateDir: dir, pollMs: 5 })
    assert.ok(await eventually(() => console.received.length === 1), 'the first instruction is delivered')

    appendFileSync(inbox, line('two', 'second'))
    assert.ok(await eventually(() => console.received.length === 2), 'the appended instruction is delivered')
    assert.deepEqual(console.received.map(trigger => trigger.id), ['local-one', 'local-two'])
  } finally {
    console.stop()
    rmSync(dir, { recursive: true, force: true })
  }
})

test('a numeric cursor from an older version still continues the inbox', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'booth-local-'))
  const inbox = join(dir, 'booth-inbox.jsonl')
  const first = line('one', 'first')
  const console = booth({ cursor: Buffer.byteLength(first) })
  try {
    writeFileSync(inbox, first + line('two', 'second'))
    apply(console.ctx, { stateDir: dir, pollMs: 5 })
    assert.ok(await eventually(() => console.received.length === 1), 'the instruction after the old cursor is delivered')
    assert.deepEqual(console.received.map(trigger => trigger.id), ['local-two'])
  } finally {
    console.stop()
    rmSync(dir, { recursive: true, force: true })
  }
})

test('a rewritten inbox of the same size is read again', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'booth-local-'))
  const inbox = join(dir, 'booth-inbox.jsonl')
  const console = booth()
  try {
    const before = line('old', 'x')
    const after = line('new', 'y')
    assert.equal(Buffer.byteLength(before), Buffer.byteLength(after), 'the test rewrites without changing the size')
    writeFileSync(inbox, before)
    apply(console.ctx, { stateDir: dir, pollMs: 5 })
    assert.ok(await eventually(() => console.received.length === 1), 'the first instruction is delivered')

    writeFileSync(inbox, after)
    assert.ok(
      await eventually(() => console.received.length === 2),
      'a replacement that kept the size is read from the beginning',
    )
    assert.deepEqual(console.received.map(trigger => trigger.id), ['local-old', 'local-new'])
  } finally {
    console.stop()
    rmSync(dir, { recursive: true, force: true })
  }
})

test('a rewritten inbox that grew keeps every new instruction', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'booth-local-'))
  const inbox = join(dir, 'booth-inbox.jsonl')
  const console = booth()
  try {
    writeFileSync(inbox, line('old', 'x'))
    apply(console.ctx, { stateDir: dir, pollMs: 5 })
    assert.ok(await eventually(() => console.received.length === 1), 'the first instruction is delivered')

    // The old cursor now lands inside the first new record; both new records must still arrive.
    writeFileSync(inbox, line('new-1', 'first') + line('new-2', 'second'))
    assert.ok(
      await eventually(() => console.received.length === 3),
      'a replacement that grew is read from the beginning',
    )
    assert.deepEqual(
      console.received.map(trigger => trigger.id),
      ['local-old', 'local-new-1', 'local-new-2'],
    )
  } finally {
    console.stop()
    rmSync(dir, { recursive: true, force: true })
  }
})

test('a cut-short outbox record does not swallow the next reply', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'booth-local-'))
  const outbox = join(dir, 'booth-outbox.jsonl')
  const console = booth()
  try {
    // A crash or a full disk can leave the last record without its newline.
    const damaged = '{"id":"torn","kind":"reply","text":"half'
    writeFileSync(outbox, damaged)
    apply(console.ctx, { stateDir: dir, pollMs: 5 })
    const channel = console.ctx.get('orchestrator').sources.kind('local').channel
    const message = id => ({ id, kind: 'reply', text: id, channel: { source: 'local', thread: 't-1', address: {} } })

    assert.equal(await channel.send({}, message('fresh')), 'sent')

    // The damaged record stays a line of its own, and the reply gets a line the console can parse.
    const first = readFileSync(outbox, 'utf8').split('\n')
    assert.equal(first.length, 3, 'the reply ends with one newline')
    assert.equal(first[0], damaged, 'the damaged record is left as it was')
    assert.equal(JSON.parse(first[1]).id, 'fresh')

    // A terminated outbox gains no blank line between replies.
    assert.equal(await channel.send({}, message('second')), 'sent')
    const lines = readFileSync(outbox, 'utf8').split('\n')
    assert.equal(lines.length, 4)
    assert.equal(JSON.parse(lines[2]).id, 'second')
  } finally {
    console.stop()
    rmSync(dir, { recursive: true, force: true })
  }
})

test('a transient read error does not leave the Source stopped', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'booth-local-'))
  const inbox = join(dir, 'booth-inbox.jsonl')
  const console = booth()
  try {
    // A directory at the inbox path makes the read fail without losing the file the test writes
    // afterwards; any error that ends the watcher has to recover the same way.
    mkdirSync(inbox)
    apply(console.ctx, { stateDir: dir, pollMs: 5 })
    assert.ok(
      await eventually(() => console.reports.some(report => report.state === 'error')),
      'the read error is reported',
    )

    rmSync(inbox, { recursive: true, force: true })
    writeFileSync(inbox, line('one', 'hi'))
    assert.ok(
      await eventually(() => console.received.length === 1, 3000),
      'the watcher starts again and reads the instruction after the error',
    )
  } finally {
    console.stop()
    rmSync(dir, { recursive: true, force: true })
  }
})
