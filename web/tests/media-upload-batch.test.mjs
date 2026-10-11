import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import ts from 'typescript';

const source = await fs.readFile(new URL('../src/lib/media-upload-batch.ts', import.meta.url), 'utf8');
const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText;
const { runMediaUploadBatch } = await import('data:text/javascript;base64,' + Buffer.from(code).toString('base64'));
const files = names => names.map(name => new File(['image'], name, { type: 'image/png' }));

test('batch transfers independent files before waiting for processing and keeps each failure isolated', async () => {
  const selected = files(['first.png', 'failed.png', 'last.png']);
  const events = [], registered = [], states = [];
  const result = await runMediaUploadBatch(selected, {
    transfer: async (file, progress) => {
      events.push(`transfer:${file.name}`); progress(50);
      if (file.name === 'failed.png') throw new Error('independent failure');
      return { id: file.name, ready: file.name === 'first.png' };
    },
    poll: async item => { events.push(`poll:${item.id}`); return { ...item, ready: true }; },
    ready: item => item.ready,
    onMedia: item => registered.push({ ...item }),
    onChange: entries => states.push(entries.map(entry => entry.state)),
    wait: async () => { events.push('wait'); },
  });
  assert.deepEqual(events, ['transfer:first.png', 'transfer:failed.png', 'transfer:last.png', 'wait', 'poll:last.png']);
  assert.deepEqual(result.map(entry => entry.state), ['ready', 'failed', 'ready']);
  assert.equal(result[1].error.message, 'independent failure');
  assert.deepEqual(registered.map(item => [item.id, item.ready]), [['first.png', true], ['last.png', false], ['last.png', true]]);
  assert.ok(states.some(state => state[1] === 'failed' && state[2] === 'queued'));
});

test('polling failure preserves registered media and still processes the other images', async () => {
  const registered = [], polled = [];
  const result = await runMediaUploadBatch(files(['offline.png', 'ready.png']), {
    transfer: async file => ({ id: file.name, ready: false }),
    poll: async item => {
      polled.push(item.id);
      if (item.id === 'offline.png') throw new Error('poll unavailable');
      return { ...item, ready: true };
    },
    ready: item => item.ready,
    onMedia: item => registered.push(item.id),
    onChange: () => {}, wait: async () => {},
  });
  assert.deepEqual(result.map(entry => entry.state), ['pending', 'ready']);
  assert.equal(result[0].media.id, 'offline.png');
  assert.equal(result[0].error.message, 'poll unavailable');
  assert.deepEqual(polled, ['offline.png', 'ready.png']);
  assert.deepEqual(registered, ['offline.png', 'ready.png', 'ready.png']);
});

test('processing waits are bounded across the batch and leave unfinished registrations pending', async () => {
  let waits = 0;
  const polled = [];
  const result = await runMediaUploadBatch(files(['one.png', 'two.png']), {
    transfer: async file => ({ id: file.name, ready: false }),
    poll: async item => { polled.push(item.id); return item; },
    ready: item => item.ready,
    onMedia: () => {}, onChange: () => {},
    attempts: 2, wait: async () => { waits++; },
  });
  assert.equal(waits, 2);
  assert.deepEqual(polled, ['one.png', 'two.png', 'one.png', 'two.png']);
  assert.deepEqual(result.map(entry => [entry.state, entry.media.id]), [['pending', 'one.png'], ['pending', 'two.png']]);
});

test('leaving the page stops remaining uploads and processing without publishing stale UI changes', async () => {
  let active = true, changes = 0, registered = 0;
  const transferred = [];
  const result = await runMediaUploadBatch(files(['one.png', 'two.png']), {
    transfer: async file => { transferred.push(file.name); active = false; return { id: file.name, ready: false }; },
    poll: async () => { throw new Error('must not poll'); },
    ready: item => item.ready,
    onMedia: () => { registered++; }, onChange: () => { changes++; },
    active: () => active,
  });
  assert.deepEqual(transferred, ['one.png']);
  assert.equal(result[1].state, 'queued');
  assert.equal(registered, 0);
  assert.equal(changes, 2);
});
