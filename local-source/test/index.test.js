import test from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as delay } from 'node:timers/promises'

import { apply } from '../index.js'

/** A booth Source whose enabled switch and inbox the test controls. */
function booth({ enabled = { value: true } } = {}) {
  const received = []
  const reports = []
  const values = new Map()
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
