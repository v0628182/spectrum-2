const controls = [
  ['stepControls', 'footstepEnhance', 'Footstep Enhance', 0, 100, 1],
  ['stepControls', 'stepLowBodyBoostDb', 'Paso: cuerpo bajo', 0, 14, 0.5],
  ['stepControls', 'stepLowMidBoostDb', 'Paso: cuerpo medio', 0, 14, 0.5],
  ['stepControls', 'stepBodyBoostDb', 'Paso: presencia 1.55k', 0, 20, 0.5],
  ['stepControls', 'stepClarityBoostDb', 'Paso: claridad 3.5k', 0, 24, 0.5],
  ['stepControls', 'detectionSensitivity', 'Sensibilidad detector', 0, 100, 1],
  ['weaponControls', 'gunshotReduction', 'Reducción disparos', 0, 100, 1],
  ['weaponControls', 'explosionReduction', 'Reducción explosiones', 0, 100, 1],
  ['weaponControls', 'weaponMidCutDb', 'Corte arma 1.6k', -48, 0, 1],
  ['weaponControls', 'weaponAirCutDb', 'Corte agudos arma', -48, 0, 1],
  ['weaponControls', 'sustainedHoldMs', 'Hold ruido largo ms', 100, 1600, 25],
  ['weaponControls', 'masterDuckDb', 'Duck maestro arma', -24, 0, 1],
  ['weaponControls', 'impactDuckDb', 'Duck impactos', -40, 0, 1],
  ['outputControls', 'actionDetail', 'Action Detail', 0, 100, 1],
  ['outputControls', 'outputCeilingDb', 'Techo salida dB', -12, -0.5, 0.5],
  ['levelControls', 'footstepLevelerAmount', 'Footstep Volume', 0, 100, 1],
  ['levelControls', 'footstepTargetRmsDb', 'Loudness objetivo', -36, -14, 0.5],
  ['levelControls', 'footstepMaxLiftDb', 'Max Lift dB', 0, 18, 0.5],
  ['levelControls', 'footstepLevelerSpeedMs', 'Velocidad ms', 10, 250, 5],
  ['stableControls', 'stabilityAmount', 'Estabilidad general', 0, 100, 1],
  ['stableControls', 'spectralFloorDb', 'Spectral floor dB', -48, -18, 1],
  ['stableControls', 'stableReleaseMs', 'Release estable ms', 80, 500, 10],
  ['stableControls', 'footstepGuardAmount', 'Proteccion pasos', 0, 100, 1],
  ['stableControls', 'maxCutStepDb', 'Max cambio corte dB', 3, 24, 1],
];

let params = {};
let clips = [];
let processTimer = 0;
let processGeneration = 0;
let isProcessing = false;
let lastProcessedPath = '';
let lastLogPath = '';
let activeAB = 'processed';
let loopStart = null;
let loopEnd = null;
let loopEnabled = false;
let markers = [];
let loopTimer = 0;
let userSeeking = false;
let pendingAutoProcess = false;
let pendingProcessedResult = null;
const fallbackParams = {
  footstepEnhance: 100,
  actionDetail: 55,
  gunshotReduction: 100,
  explosionReduction: 100,
  detectionSensitivity: 55,
  outputCeilingDb: -6,
  stepBodyBoostDb: 11,
  stepClarityBoostDb: 18,
  stepLowBodyBoostDb: 8,
  stepLowMidBoostDb: 7,
  weaponMidCutDb: -30,
  weaponAirCutDb: -28,
  sustainedHoldMs: 900,
  masterDuckDb: -10,
  impactDuckDb: -24,
  footstepLevelerAmount: 35,
  footstepTargetRmsDb: -24,
  footstepMaxLiftDb: 8,
  footstepLevelerSpeedMs: 80,
  stabilityAmount: 70,
  spectralFloorDb: -34,
  stableReleaseMs: 260,
  footstepGuardAmount: 85,
  maxCutStepDb: 8,
};

const $ = (id) => document.getElementById(id);

function setStatus(text) { $('status').textContent = text; }

function scheduleProcess(reason = 'cambio') {
  const auto = $('autoProcess');
  if (!auto || !auto.checked) return;
  if (userSeeking) {
    pendingAutoProcess = true;
    setStatus(`Cambio detectado (${reason}). Esperando a que termines de mover la barra...`);
    return;
  }
  clearTimeout(processTimer);
  setStatus(`Cambio detectado (${reason}). Procesando en breve...`);
  processTimer = setTimeout(() => processClip(true), 650);
}

function createControls() {
  for (const [group, key, label, min, max, step] of controls) {
    const wrap = document.createElement('div');
    wrap.className = 'control';
    wrap.innerHTML = `<label><span>${label}</span><strong id="${key}Value"></strong></label>
      <input id="${key}" type="range" min="${min}" max="${max}" step="${step}">`;
    $(group).appendChild(wrap);
    $(key).addEventListener('input', () => {
      params[key] = Number($(key).value);
      updateControlLabels();
      scheduleProcess(key);
    });
  }
}

function updateControlLabels() {
  for (const [, key] of controls) {
    if ($(key)) {
      const value = Number.isFinite(Number(params[key])) ? Number(params[key]) : fallbackParams[key];
      params[key] = value;
      $(key).value = value;
      $(`${key}Value`).textContent = value;
    }
  }
}

async function loadFiles() {
  const previous = $('clip').value;
  const data = await fetch('/api/files', { cache: 'no-store' }).then((r) => r.json());
  clips = Array.isArray(data) ? data : (data.value || []);
  $('clip').innerHTML = clips.map((f) => `<option value="${f.path}">${f.name}</option>`).join('');
  if (clips.length === 0) {
    setStatus('No hay WAVs en captures/raw o captures/raw_test.');
    return;
  }
  if (previous && clips.some((clip) => clip.path === previous)) {
    $('clip').value = previous;
  }
  updateOriginal();
}

async function loadPresets() {
  const data = await fetch('/api/presets', { cache: 'no-store' }).then((r) => r.json());
  const presets = Array.isArray(data) ? data : (data.value || []);
  $('preset').innerHTML = presets.map((p, i) => `<option value="${i}">${p.name}</option>`).join('');
  $('preset')._presets = presets;
  if (presets.length > 0) applyPreset(0);
  else {
    params = { ...fallbackParams };
    updateControlLabels();
  }
}

function applyPreset(index) {
  const preset = $('preset')._presets[index];
  params = { ...fallbackParams, ...(preset ? preset.params : {}) };
  markers = [];
  renderMarkers();
  updateControlLabels();
  scheduleProcess('preset');
}

function mediaUrl(path) {
  return `/media?path=${encodeURIComponent(path)}`;
}

function canSwapProcessedAudio(force = false) {
  const processed = $('processedAudio');
  return force || (!userSeeking && (processed.paused || processed.ended || !processed.src));
}

async function applyProcessedResult(result, force = false) {
  if (!result || !result.output || !canSwapProcessedAudio(force)) {
    pendingProcessedResult = result;
    $('applyPending').hidden = !pendingProcessedResult;
    setStatus('Procesado nuevo listo. Pausa el audio o pulsa "Aplicar procesado listo".');
    return false;
  }

  const processed = $('processedAudio');
  const previousTime = Number.isFinite(processed.currentTime) ? processed.currentTime : 0;
  const wasPlaying = !processed.paused && !processed.ended;
  const nextUrl = `${mediaUrl(result.output)}&t=${Date.now()}`;

  processed.pause();
  processed.src = nextUrl;
  processed.load();
  await new Promise((resolve) => {
    let resolved = false;
    const done = () => {
      if (resolved) return;
      resolved = true;
      resolve();
    };
    processed.addEventListener('loadedmetadata', done, { once: true });
    setTimeout(done, 1200);
  });
  if (previousTime > 0 && Number.isFinite(processed.duration)) {
    processed.currentTime = Math.min(previousTime, Math.max(0, processed.duration - 0.05));
  }
  if (wasPlaying && force) {
    processed.play().catch(() => {});
  }
  await drawWaveform('processedCanvas', result.output);
  pendingProcessedResult = null;
  $('applyPending').hidden = true;
  return true;
}

function activeAudio() {
  return activeAB === 'original' ? $('originalAudio') : $('processedAudio');
}

function syncTimes(source, target) {
  if (!Number.isFinite(source.currentTime)) return;
  try { target.currentTime = Math.min(source.currentTime, Math.max(0, target.duration || source.currentTime)); } catch {}
}

function updateOriginal() {
  const clip = $('clip').value;
  if (!clip) return;
  $('originalAudio').src = mediaUrl(clip);
  drawWaveform('originalCanvas', clip);
  $('processedAudio').removeAttribute('src');
  $('processedAudio').load();
  lastProcessedPath = '';
  lastLogPath = '';
  pendingProcessedResult = null;
  $('applyPending').hidden = true;
  markers = [];
  renderMarkers();
  scheduleProcess('clip');
}

async function processClip(auto = false) {
  const input = $('clip').value;
  if (!input) return;
  if (auto && userSeeking) {
    pendingAutoProcess = true;
    return;
  }
  const generation = ++processGeneration;
  if (isProcessing && auto) {
    clearTimeout(processTimer);
    processTimer = setTimeout(() => processClip(true), 800);
    return;
  }
  isProcessing = true;
  setStatus(auto ? 'Procesando automaticamente...' : 'Procesando...');
  let result;
  try {
    const response = await fetch('/api/process', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ input, params }),
    });
    result = await response.json();
    if (!response.ok) {
      setStatus(result.error || 'Error procesando.');
      isProcessing = false;
      return;
    }
  } catch (error) {
    setStatus(`Error: ${error.message}`);
    isProcessing = false;
    return;
  }
  if (generation !== processGeneration) {
    isProcessing = false;
    scheduleProcess('nuevo cambio');
    return;
  }
  lastProcessedPath = result.output;
  lastLogPath = result.log;
  const applied = await applyProcessedResult(result, !auto && !userSeeking);
  renderSummary(result.summary);
  if (result.log) {
    fetch(`/api/series?path=${encodeURIComponent(result.log)}`, { cache: 'no-store' })
      .then((r) => r.json())
      .then((series) => drawScoreSeries(series))
      .catch(() => {});
  }
  setStatus(applied ? `Listo: ${result.output}` : `Procesado listo pendiente: ${result.output}`);
  isProcessing = false;
}

function toggleAB() {
  const original = $('originalAudio');
  const processed = $('processedAudio');
  if (!processed.src) return;
  if (activeAB === 'processed') {
    syncTimes(processed, original);
    processed.pause();
    original.play().catch(() => {});
    activeAB = 'original';
    $('toggleAB').textContent = 'Escuchar original';
  } else {
    syncTimes(original, processed);
    original.pause();
    processed.play().catch(() => {});
    activeAB = 'processed';
    $('toggleAB').textContent = 'Escuchar procesado';
  }
}

function updateLoopReadout() {
  const start = loopStart == null ? '--' : loopStart.toFixed(2);
  const end = loopEnd == null ? '--' : loopEnd.toFixed(2);
  $('loopReadout').textContent = `Loop: ${start}s - ${end}s ${loopEnabled ? '(on)' : '(off)'}`;
  $('toggleLoop').textContent = loopEnabled ? 'Loop on' : 'Loop off';
}

function enforceLoop(audio) {
  if (!loopEnabled || loopStart == null || loopEnd == null || loopEnd <= loopStart) return;
  if (!audio || audio.paused || audio.ended) return;
  if (audio.currentTime >= loopEnd || audio.currentTime < loopStart - 0.25) {
    audio.currentTime = loopStart;
  }
}

function enforceLoopAll() {
  enforceLoop($('originalAudio'));
  enforceLoop($('processedAudio'));
}

function addMarker(kind) {
  const audio = activeAudio();
  markers.push({ kind, time: audio.currentTime || 0 });
  renderMarkers();
  saveMarkers(true);
}

function renderMarkers() {
  $('markersList').innerHTML = markers.map((m) =>
    `<span class="marker-pill">${m.kind} @ ${m.time.toFixed(2)}s</span>`
  ).join('');
}

function currentPreset() {
  const presets = $('preset')._presets || [];
  return presets[Number($('preset').value)] || null;
}

async function saveMarkers(auto = false) {
  const input = $('clip').value;
  const preset = currentPreset();
  if (!input || markers.length === 0) {
    if (!auto) setStatus('No hay marcas para guardar.');
    return;
  }
  try {
    const response = await fetch('/api/save-markers', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        input,
        presetName: preset?.name || 'preset',
        presetPath: preset?.name ? `config/${preset.name}` : '',
        markers,
      }),
    });
    const result = await response.json();
    if (!response.ok) {
      throw new Error(result.error || 'Error guardando marcas');
    }
    setStatus(`${auto ? 'Marca guardada' : 'Marcas guardadas'}: ${result.path}`);
  } catch (error) {
    setStatus(`Error guardando marcas: ${error.message}`);
  }
}

function drawScoreSeries(series) {
  const canvas = $('scoreCanvas');
  const ctx = canvas.getContext('2d');
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = '#0b0d0b';
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  if (!Array.isArray(series) || series.length < 2) return;

  const drawLine = (key, color, scale = 1) => {
    ctx.strokeStyle = color;
    ctx.lineWidth = 2;
    ctx.beginPath();
    series.forEach((p, i) => {
      const x = i / (series.length - 1) * canvas.width;
      const y = canvas.height - Math.max(0, Math.min(1, p[key] * scale)) * canvas.height;
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();
  };
  drawLine('footstep', '#d6ff4f');
  drawLine('protection', '#ff6b4a');
  drawLine('action', '#45d0a1');
  drawLine('peak', '#f0f0f0');
}

async function savePreset() {
  const name = $('presetName').value || 'opcion_2';
  if (!name) return;
  const response = await fetch('/api/save-preset', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, params }),
  });
  const result = await response.json();
  setStatus(`Preset guardado: ${result.name}`);
  await loadPresets();
}

async function comparePresets() {
  const input = $('clip').value;
  const presets = $('preset')._presets || [];
  $('comparisonList').innerHTML = '';
  setStatus('Comparando presets...');
  for (const preset of presets) {
    const response = await fetch('/api/process', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ input, params: preset.params }),
    });
    const result = await response.json();
    const card = document.createElement('div');
    card.className = 'comparison-card';
    card.innerHTML = `<strong>${preset.name}</strong>
      <div>foot ${result.summary?.maxFootstep?.toFixed(3) ?? '--'} | prot ${result.summary?.maxProtection?.toFixed(3) ?? '--'}</div>
      <audio controls src="${mediaUrl(result.output)}"></audio>`;
    $('comparisonList').appendChild(card);
  }
  setStatus('Comparación lista.');
}

async function drawWaveform(canvasId, filePath) {
  const canvas = $(canvasId);
  const ctx = canvas.getContext('2d');
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = '#0b0d0b';
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.strokeStyle = '#d6ff4f';
  ctx.lineWidth = 1;

  try {
    const buffer = await fetch(mediaUrl(filePath)).then((r) => r.arrayBuffer());
    const audio = await new AudioContext().decodeAudioData(buffer);
    const data = audio.getChannelData(0);
    const step = Math.max(1, Math.floor(data.length / canvas.width));
    ctx.beginPath();
    for (let x = 0; x < canvas.width; x += 1) {
      let min = 1;
      let max = -1;
      const start = x * step;
      for (let i = 0; i < step && start + i < data.length; i += 1) {
        const v = data[start + i];
        min = Math.min(min, v);
        max = Math.max(max, v);
      }
      const y1 = (1 - (max + 1) / 2) * canvas.height;
      const y2 = (1 - (min + 1) / 2) * canvas.height;
      ctx.moveTo(x, y1);
      ctx.lineTo(x, y2);
    }
    ctx.stroke();
  } catch {
    ctx.fillStyle = '#9ba696';
    ctx.fillText('No se pudo dibujar waveform', 20, 40);
  }
}

function renderSummary(summary) {
  if (!summary) return;
  const items = [
    ['Footstep max', summary.maxFootstep],
    ['Action max', summary.maxAction],
    ['Protection max', summary.maxProtection],
    ['Peak max', summary.maxPeak],
    ['Frames pasos', summary.footFrames],
    ['Frames protección', summary.protectionFrames],
  ];
  $('summaryGrid').innerHTML = items.map(([name, value]) =>
    `<div class="metric"><strong>${typeof value === 'number' ? value.toFixed(3) : value}</strong><span>${name}</span></div>`
  ).join('');
}

$('refresh').addEventListener('click', loadFiles);
$('clip').addEventListener('change', updateOriginal);
$('preset').addEventListener('change', (event) => applyPreset(Number(event.target.value)));
$('process').addEventListener('click', () => processClip(false));
$('applyPending').addEventListener('click', () => applyProcessedResult(pendingProcessedResult, true));
$('savePreset').addEventListener('click', savePreset);
$('saveMarkers').addEventListener('click', () => saveMarkers(false));
$('comparePresets').addEventListener('click', comparePresets);
$('toggleAB').addEventListener('click', toggleAB);
$('syncAB').addEventListener('click', () => syncTimes(activeAudio(), activeAB === 'original' ? $('processedAudio') : $('originalAudio')));
$('setLoopStart').addEventListener('click', () => {
  loopStart = activeAudio().currentTime || 0;
  if (loopEnd != null && loopEnd <= loopStart) loopEnd = null;
  updateLoopReadout();
});
$('setLoopEnd').addEventListener('click', () => {
  const t = activeAudio().currentTime || 0;
  if (loopStart == null || t <= loopStart) {
    setStatus('El fin del loop debe estar después del inicio.');
    return;
  }
  loopEnd = t;
  updateLoopReadout();
});
$('toggleLoop').addEventListener('click', () => {
  if (loopStart == null || loopEnd == null || loopEnd <= loopStart) {
    setStatus('Marca inicio y fin antes de activar loop.');
    return;
  }
  loopEnabled = !loopEnabled;
  if (loopEnabled) {
    $('originalAudio').currentTime = loopStart;
    if ($('processedAudio').src) $('processedAudio').currentTime = loopStart;
  }
  updateLoopReadout();
});
$('clearLoop').addEventListener('click', () => { loopStart = null; loopEnd = null; loopEnabled = false; updateLoopReadout(); });
document.querySelectorAll('[data-marker]').forEach((button) => {
  button.addEventListener('click', () => addMarker(button.dataset.marker));
});
$('originalAudio').addEventListener('timeupdate', () => enforceLoop($('originalAudio')));
$('processedAudio').addEventListener('timeupdate', () => enforceLoop($('processedAudio')));
$('processedAudio').addEventListener('seeking', () => {
  userSeeking = true;
  clearTimeout(processTimer);
});
$('processedAudio').addEventListener('seeked', () => {
  userSeeking = false;
  if (pendingAutoProcess) {
    pendingAutoProcess = false;
    scheduleProcess('seek terminado');
  }
});
$('processedAudio').addEventListener('pause', () => {
  if (pendingProcessedResult && !userSeeking) {
    applyProcessedResult(pendingProcessedResult, false);
  }
});
$('originalAudio').addEventListener('seeking', () => {
  userSeeking = true;
  clearTimeout(processTimer);
});
$('originalAudio').addEventListener('seeked', () => {
  userSeeking = false;
  if (pendingAutoProcess) {
    pendingAutoProcess = false;
    scheduleProcess('seek terminado');
  }
});
clearInterval(loopTimer);
loopTimer = setInterval(enforceLoopAll, 80);

createControls();
Promise.all([loadPresets(), loadFiles()])
  .then(() => setStatus('Listo.'))
  .catch((error) => {
    params = { ...fallbackParams };
    updateControlLabels();
    setStatus(`Error inicializando UI: ${error.message}`);
  });
