const $ = (id) => document.getElementById(id);
let worker;
let ready = false;
let busy = false;
let failed = false;
let selected;
let resultUrl;
let timer;
let started;
const LOG_LINES = 500;
let logLines = [];
let logFrame;
let lastProgress;

function appendLog(text) {
  for (const line of String(text).split(/\r?\n/)) {
    const match = /^(WebGPU|CPU) search: (\d+) \/ (\d+) candidates$/.exec(line);
    if (match && busy) {
      const current = Number(match[2]);
      const total = Number(match[3]);
      if (total > 0 && current <= total) {
        $('progress').max = total;
        $('progress').value = current;
        $('progress-label').textContent = `${match[1]} / Salt 搜索`;
        $('progress-value').textContent = `${(current / total * 100).toFixed(2)}%`;
      }
    } else if (busy && /^(Salt: |CPU search: one |WebGPU: starting)/.test(line)) {
      $('progress').removeAttribute('value');
      $('progress-label').textContent = line.startsWith('Salt: ') ? '恢复与重建' : 'Salt 搜索';
      $('progress-value').textContent = '';
    }
    if (match && lastProgress === match[1]) logLines[logLines.length - 1] = line;
    else logLines.push(line);
    lastProgress = match?.[1];
  }
  if (logLines.length > LOG_LINES) logLines.splice(0, logLines.length - LOG_LINES);
  if (!logFrame) logFrame = requestAnimationFrame(() => {
    logFrame = undefined;
    const log = $('log');
    const follow = log.scrollHeight - log.scrollTop - log.clientHeight < 40;
    log.textContent = logLines.join('\n');
    if (follow) log.scrollTop = log.scrollHeight;
  });
}

function status(text, error = false) {
  $('status').textContent = text;
  $('status').classList.toggle('error', error);
}

function controls() {
  $('run').disabled = busy || (!failed && (!ready || !selected));
  $('pick').disabled = busy;
  $('file').disabled = busy;
  $('cancel').disabled = !busy;
  for (const input of $('controls').querySelectorAll('input, select')) input.disabled = busy;
}

function tab(log) {
  $('result-pane').hidden = log;
  $('log').hidden = !log;
  for (const [id, active] of [['result-tab', !log], ['log-tab', log]]) {
    $(id).classList.toggle('active', active);
    $(id).setAttribute('aria-pressed', String(active));
  }
}

function finish() {
  clearInterval(timer);
  busy = false;
  controls();
}

function boot() {
  ready = false;
  failed = false;
  controls();
  try {
    worker = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
  } catch (error) {
    fail(`无法创建 Worker：${error}`);
    return;
  }
  worker.onmessage = ({ data }) => {
    if (data.type === 'ready') {
      ready = true;
      status('引擎就绪');
      controls();
    } else if (data.type === 'log') {
      appendLog(data.text);
    } else if (data.type === 'result') {
      finish();
      $('progress').max = 1;
      $('progress').value = 1;
      $('progress-label').textContent = '完成';
      $('progress-value').textContent = '100%';
      resultUrl = URL.createObjectURL(new Blob([data.buffer], { type: 'application/octet-stream' }));
      $('download').disabled = false;
      $('download').dataset.name = data.name;
      $('welcome').hidden = true;
      $('preview').hidden = false;
      $('preview').textContent = data.text || '未发现已注册的模板 VM 入口。';
      $('heading').textContent = '分析完成';
      status(`已生成 ${data.name} · ${data.buffer.byteLength.toLocaleString()} bytes`);
      tab(false);
    } else if (data.type === 'error' || data.type === 'fatal') {
      fail(data.text);
    }
  };
  worker.onerror = (event) => fail(event.message || 'Worker failed. Browser memory may be exhausted.');
  worker.onmessageerror = () => fail('Worker message could not be decoded.');
}

function fail(message) {
  finish();
  ready = false;
  failed = true;
  worker?.terminate();
  $('progress-area').hidden = true;
  appendLog(`ERROR: ${message}`);
  $('heading').textContent = '分析未完成';
  status(message, true);
  tab(true);
  controls();
  // A fresh worker also releases WASM memory after a trap or allocation failure.
  $('run').textContent = '重新加载引擎';
  $('run').disabled = false;
}

function choose(file) {
  if (!file || busy) return;
  selected = file;
  $('filename').textContent = file.name;
  $('filesize').textContent = `${file.size.toLocaleString()} bytes`;
  controls();
}

$('pick').onclick = () => $('file').click();
$('file').onchange = () => choose($('file').files[0]);
$('controls').onchange = () => {
  $('devirt').hidden = new FormData($('controls')).get('mode') !== 'devirt';
  $('backend-field').hidden = new FormData($('controls')).get('mode') !== 'unpack';
};
$('result-tab').onclick = () => tab(false);
$('log-tab').onclick = () => tab(true);
$('controls').onsubmit = async (event) => {
  event.preventDefault();
  if (!ready) {
    $('run').textContent = '运行分析';
    status('正在重新加载引擎…');
    boot();
    return;
  }
  if (busy || !selected) return;
  const mode = new FormData($('controls')).get('mode');
  const rva = $('rva').value.trim();
  if (mode === 'devirt' && !/^(?:0x)?[0-9a-f]{1,8}$/i.test(rva)) {
    status('请输入合法的 32 位十六进制 RVA。', true);
    $('rva').focus();
    return;
  }
  if (resultUrl) URL.revokeObjectURL(resultUrl);
  resultUrl = undefined;
  $('download').disabled = true;
  $('log').textContent = '';
  logLines = [];
  lastProgress = undefined;
  $('preview').textContent = '';
  $('welcome').hidden = true;
  busy = true;
  $('progress-area').hidden = false;
  $('progress').removeAttribute('value');
  $('progress-label').textContent = '解析与准备';
  $('progress-value').textContent = '';
  controls();
  started = performance.now();
  $('elapsed').textContent = '0.0 s';
  timer = setInterval(() => { $('elapsed').textContent = `${((performance.now() - started) / 1000).toFixed(1)} s`; }, 100);
  $('heading').textContent = `正在运行 ${mode}`;
  status('处理中');
  tab(true);
  const activeWorker = worker;
  try {
    const buffer = await selected.arrayBuffer();
    if (worker !== activeWorker || !busy) return;
    worker.postMessage({ buffer, name: selected.name, mode, rva, binary: $('binary').checked, backend: $('backend').value }, [buffer]);
  } catch (error) {
    if (worker === activeWorker) fail(String(error));
  }
};
$('cancel').onclick = () => {
  worker.terminate();
  finish();
  $('progress-area').hidden = true;
  appendLog('Cancelled by user.');
  $('heading').textContent = '任务已取消';
  status('任务已取消，正在重新加载引擎…');
  boot();
};
$('download').onclick = () => {
  if (!resultUrl) return;
  const link = document.createElement('a');
  link.href = resultUrl;
  link.download = $('download').dataset.name;
  link.click();
};
document.addEventListener('dragover', (event) => { event.preventDefault(); if (!busy) document.body.classList.add('dragging'); });
document.addEventListener('dragleave', (event) => { if (!event.relatedTarget) document.body.classList.remove('dragging'); });
document.addEventListener('drop', (event) => {
  event.preventDefault();
  document.body.classList.remove('dragging');
  choose(event.dataTransfer.files[0]);
});
boot();
