import init, { process } from './pkg/enigma_web.js';

const originalLog = console.log.bind(console);
console.log = (...args) => {
  originalLog(...args);
  self.postMessage({ type: 'log', text: args.map(String).join(' ') });
};

try {
  await init();
  self.postMessage({ type: 'ready' });
} catch (error) {
  self.postMessage({ type: 'fatal', text: `WASM initialization failed: ${error}` });
}

self.onmessage = async ({ data }) => {
  let output;
  try {
    output = await process(new Uint8Array(data.buffer), data.name, data.mode, data.rva, data.binary, data.backend);
    const bytes = output.data;
    self.postMessage({ type: 'result', name: output.name, text: output.text, buffer: bytes.buffer }, [bytes.buffer]);
  } catch (error) {
    self.postMessage({ type: 'error', text: String(error) });
  } finally {
    output?.free();
  }
};
