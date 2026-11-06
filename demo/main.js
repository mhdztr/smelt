import init, { version, supported_formats, detect_format, convert } from './pkg/smelt_wasm.js';

const statusEl = document.getElementById('status');
const inputEl = document.getElementById('input');
const outputEl = document.getElementById('output');
const fromEl = document.getElementById('from');
const toEl = document.getElementById('to');
const fileEl = document.getElementById('file');
const filenameEl = document.getElementById('filename');
const convertBtn = document.getElementById('convert');
const downloadBtn = document.getElementById('download');

let lastOutput = null;
let lastOutputName = 'output.txt';

function setStatus(msg, kind = '') {
  statusEl.textContent = msg;
  statusEl.className = kind;
}

function extForFormat(fmt) {
  return fmt === 'md' ? 'md' : fmt;
}

function isProbablyText(bytes) {
  const sample = bytes.subarray(0, Math.min(bytes.length, 4096));
  for (let i = 0; i < sample.length; i++) {
    if (sample[i] === 0) return false;
  }
  return true;
}

async function load() {
  try {
    await init();
    const fmts = JSON.parse(supported_formats());
    for (const fmt of fmts) {
      const opt = document.createElement('option');
      opt.value = fmt;
      opt.textContent = fmt;
      fromEl.appendChild(opt.cloneNode(true));
      toEl.appendChild(opt);
    }
    if (fmts.includes('html')) toEl.value = 'html';
    setStatus(`Loaded smelt ${version()}. Supported in this build: ${fmts.join(', ')}.`, 'ok');
  } catch (err) {
    setStatus(
      'Failed to load WASM module. Build it with:\n' +
      '  wasm-pack build crates/smelt-wasm --target web --out-dir ../../demo/pkg --release\n' +
      'Then serve demo/ with a static file server.\n\n' +
      String(err),
      'error',
    );
    convertBtn.disabled = true;
  }
}

fileEl.addEventListener('change', async () => {
  const file = fileEl.files && fileEl.files[0];
  if (!file) return;
  filenameEl.value = file.name;
  const buf = new Uint8Array(await file.arrayBuffer());
  if (isProbablyText(buf)) {
    inputEl.value = new TextDecoder().decode(buf);
    setStatus(`Loaded ${file.name} (${buf.length} bytes). Detected: ${detect_format(buf, file.name)}.`, 'ok');
  } else {
    inputEl.value = `[binary file loaded: ${file.name}, ${buf.length} bytes]`;
    inputEl.dataset.binary = '1';
    setStatus(`Loaded binary file ${file.name} (${buf.length} bytes). Detected: ${detect_format(buf, file.name)}.`, 'ok');
  }
  inputEl._binary = isProbablyText(buf) ? null : buf;
});

convertBtn.addEventListener('click', async () => {
  try {
    convertBtn.disabled = true;
    downloadBtn.disabled = true;
    lastOutput = null;
    const from = fromEl.value === 'auto' ? null : fromEl.value;
    const to = toEl.value;
    const filename = filenameEl.value || null;
    let input;
    if (inputEl._binary) {
      input = inputEl._binary;
    } else {
      input = new TextEncoder().encode(inputEl.value);
    }
    const detected = detect_format(input, filename);
    const out = convert(input, from, to, filename);
    lastOutput = out;
    lastOutputName = `output.${extForFormat(to)}`;
    if (isProbablyText(out) && to !== 'pdf' && to !== 'epub') {
      outputEl.value = new TextDecoder().decode(out);
    } else {
      outputEl.value = `[binary output: ${out.length} bytes; use Download output]`;
    }
    downloadBtn.disabled = false;
    setStatus(`Converted ${filename || 'input'} (${detected} -> ${to}), ${out.length} bytes.`, 'ok');
  } catch (err) {
    outputEl.value = '';
    setStatus(String(err), 'error');
  } finally {
    convertBtn.disabled = false;
  }
});

downloadBtn.addEventListener('click', () => {
  if (!lastOutput) return;
  const blob = new Blob([lastOutput], { type: 'application/octet-stream' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = lastOutputName;
  a.click();
  URL.revokeObjectURL(url);
});

load();
