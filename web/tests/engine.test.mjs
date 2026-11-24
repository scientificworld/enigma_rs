import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { initSync, process } from '../pkg/enigma_web.js';

initSync({ module: await readFile(new URL('../pkg/enigma_web_bg.wasm', import.meta.url)) });

test('all modes reject malformed input without a WASM trap', async () => {
  for (const mode of ['unpack', 'runtime', 'scan', 'devirt']) {
    await assert.rejects(
      () => process(new Uint8Array([77, 90]), 'short.exe', mode, '0x1000', false, 'auto'),
      (error) => typeof error === 'string' && error.includes('out of bounds'),
    );
  }
});

test('engine remains usable after an input error', async () => {
  for (let attempt = 0; attempt < 2; attempt++) {
    await assert.rejects(
      () => process(new Uint8Array([0, 0]), 'invalid.exe', 'unpack', '', false, 'cpu'),
      (error) => typeof error === 'string' && error.includes('missing MZ signature'),
    );
  }
});

test('archive traversal names are rejected before processing', async () => {
  for (const name of ['', '.', '..', '../input.exe', 'a/b.exe', 'a\\b.exe', 'C:input.exe']) {
    await assert.rejects(
      () => process(new Uint8Array(), name, 'unpack', '', false, 'auto'),
      (error) => typeof error === 'string' && error.includes('invalid input filename'),
    );
  }
});

test('invalid backend is rejected without accessing a GPU', async () => {
  await assert.rejects(
    () => process(new Uint8Array(), 'input.exe', 'unpack', '', false, 'invalid'),
    (error) => typeof error === 'string' && error.includes('unknown search backend'),
  );
});

test('explicit WebGPU selection still validates malformed input', async () => {
  await assert.rejects(
    () => process(new Uint8Array([0, 0]), 'input.exe', 'unpack', '', false, 'wgpu'),
    (error) => typeof error === 'string' && error.includes('missing MZ signature'),
  );
});
