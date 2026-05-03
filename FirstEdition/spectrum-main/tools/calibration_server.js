const http = require('http');
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const root = path.resolve(__dirname, '..');
const publicDir = path.join(root, 'tools', 'calibration_ui');
const port = Number(process.env.WZA_UI_PORT || 4177);

const mime = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'application/javascript; charset=utf-8',
  '.wav': 'audio/wav',
  '.json': 'application/json; charset=utf-8',
};

const defaultParams = {
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
  protectionExtreme: true,
  debugLogging: true,
};

function sendJson(res, value, status = 200) {
  const body = JSON.stringify(value, null, 2);
  res.writeHead(status, {
    'Content-Type': mime['.json'],
    'Content-Length': Buffer.byteLength(body),
    'Cache-Control': 'no-store',
  });
  res.end(body);
}

function serveMedia(req, res, filePath) {
  const ext = path.extname(filePath).toLowerCase();
  const stat = fs.statSync(filePath);
  const contentType = mime[ext] || 'application/octet-stream';
  const range = req.headers.range;

  if (!range) {
    res.writeHead(200, {
      'Content-Type': contentType,
      'Content-Length': stat.size,
      'Accept-Ranges': 'bytes',
      'Cache-Control': 'no-store',
    });
    fs.createReadStream(filePath).pipe(res);
    return;
  }

  const match = /^bytes=(\d*)-(\d*)$/.exec(range);
  if (!match) {
    res.writeHead(416, {
      'Content-Range': `bytes */${stat.size}`,
      'Accept-Ranges': 'bytes',
    });
    res.end();
    return;
  }

  let start = match[1] === '' ? 0 : Number(match[1]);
  let end = match[2] === '' ? stat.size - 1 : Number(match[2]);
  if (!Number.isFinite(start) || !Number.isFinite(end) || start > end || start >= stat.size) {
    res.writeHead(416, {
      'Content-Range': `bytes */${stat.size}`,
      'Accept-Ranges': 'bytes',
    });
    res.end();
    return;
  }

  end = Math.min(end, stat.size - 1);
  res.writeHead(206, {
    'Content-Type': contentType,
    'Content-Length': end - start + 1,
    'Content-Range': `bytes ${start}-${end}/${stat.size}`,
    'Accept-Ranges': 'bytes',
    'Cache-Control': 'no-store',
  });
  fs.createReadStream(filePath, { start, end }).pipe(res);
}

function safeJoin(base, requested) {
  const clean = String(requested || '').replace(/^[/\\]+/, '');
  const resolved = path.resolve(base, clean);
  if (!resolved.startsWith(base)) {
    throw new Error('Path outside workspace');
  }
  return resolved;
}

function listWavs(dir) {
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir)
    .filter((name) => name.toLowerCase().endsWith('.wav'))
    .map((name) => {
      const fullPath = path.join(dir, name);
      const stat = fs.statSync(fullPath);
      return {
        name,
        path: path.relative(root, fullPath).replaceAll('\\', '/'),
        bytes: stat.size,
      };
    });
}

function listPresets() {
  const configDir = path.join(root, 'config');
  return fs.readdirSync(configDir)
    .filter((name) => name.toLowerCase().endsWith('.ini'))
    .sort((a, b) => a.localeCompare(b))
    .map((name) => ({ name, params: parseIni(path.join(configDir, name)) }));
}

function parseIni(filePath) {
  const params = { ...defaultParams };
  if (!fs.existsSync(filePath)) return params;
  const lines = fs.readFileSync(filePath, 'utf8').split(/\r?\n/);
  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#') || trimmed.startsWith(';') || trimmed.startsWith('[')) continue;
    const eq = trimmed.indexOf('=');
    if (eq === -1) continue;
    const key = trimmed.slice(0, eq).trim();
    const raw = trimmed.slice(eq + 1).trim();
    if (!(key in params)) continue;
    if (raw === 'true' || raw === 'false') params[key] = raw === 'true';
    else params[key] = Number(raw);
  }
  return params;
}

function writeTempConfig(params, logPath) {
  const cfg = { ...defaultParams, ...params, debugLogging: true };
  const text = `[audio]
footstepEnhance=${cfg.footstepEnhance}
actionDetail=${cfg.actionDetail}
gunshotReduction=${cfg.gunshotReduction}
explosionReduction=${cfg.explosionReduction}
detectionSensitivity=${cfg.detectionSensitivity}
outputCeilingDb=${cfg.outputCeilingDb}
stepBodyBoostDb=${cfg.stepBodyBoostDb}
stepClarityBoostDb=${cfg.stepClarityBoostDb}
stepLowBodyBoostDb=${cfg.stepLowBodyBoostDb}
stepLowMidBoostDb=${cfg.stepLowMidBoostDb}
weaponMidCutDb=${cfg.weaponMidCutDb}
weaponAirCutDb=${cfg.weaponAirCutDb}
sustainedHoldMs=${cfg.sustainedHoldMs}
masterDuckDb=${cfg.masterDuckDb}
impactDuckDb=${cfg.impactDuckDb}
footstepLevelerAmount=${cfg.footstepLevelerAmount}
footstepTargetRmsDb=${cfg.footstepTargetRmsDb}
footstepMaxLiftDb=${cfg.footstepMaxLiftDb}
footstepLevelerSpeedMs=${cfg.footstepLevelerSpeedMs}
stabilityAmount=${cfg.stabilityAmount}
spectralFloorDb=${cfg.spectralFloorDb}
stableReleaseMs=${cfg.stableReleaseMs}
footstepGuardAmount=${cfg.footstepGuardAmount}
maxCutStepDb=${cfg.maxCutStepDb}
protectionExtreme=${cfg.protectionExtreme ? 'true' : 'false'}
debugLogging=true

[logging]
logPath=${logPath.replaceAll('\\', '/')}
logEveryFrames=8
`;
  const configDir = path.join(root, 'build', 'ui_configs');
  fs.mkdirSync(configDir, { recursive: true });
  const configPath = path.join(configDir, `ui_${Date.now()}.ini`);
  fs.writeFileSync(configPath, text, 'utf8');
  return configPath;
}

function writePreset(name, params) {
  const cleanName = String(name || '').trim().replace(/[^a-zA-Z0-9_-]+/g, '_') || 'preset';
  const fileName = cleanName.toLowerCase().endsWith('.ini') ? cleanName : `${cleanName}.ini`;
  const configPath = path.join(root, 'config', fileName);
  const cfg = { ...defaultParams, ...params, debugLogging: true };
  const text = `[audio]
footstepEnhance=${cfg.footstepEnhance}
actionDetail=${cfg.actionDetail}
gunshotReduction=${cfg.gunshotReduction}
explosionReduction=${cfg.explosionReduction}
detectionSensitivity=${cfg.detectionSensitivity}
outputCeilingDb=${cfg.outputCeilingDb}
stepBodyBoostDb=${cfg.stepBodyBoostDb}
stepClarityBoostDb=${cfg.stepClarityBoostDb}
stepLowBodyBoostDb=${cfg.stepLowBodyBoostDb}
stepLowMidBoostDb=${cfg.stepLowMidBoostDb}
weaponMidCutDb=${cfg.weaponMidCutDb}
weaponAirCutDb=${cfg.weaponAirCutDb}
sustainedHoldMs=${cfg.sustainedHoldMs}
masterDuckDb=${cfg.masterDuckDb}
impactDuckDb=${cfg.impactDuckDb}
footstepLevelerAmount=${cfg.footstepLevelerAmount}
footstepTargetRmsDb=${cfg.footstepTargetRmsDb}
footstepMaxLiftDb=${cfg.footstepMaxLiftDb}
footstepLevelerSpeedMs=${cfg.footstepLevelerSpeedMs}
stabilityAmount=${cfg.stabilityAmount}
spectralFloorDb=${cfg.spectralFloorDb}
stableReleaseMs=${cfg.stableReleaseMs}
footstepGuardAmount=${cfg.footstepGuardAmount}
maxCutStepDb=${cfg.maxCutStepDb}
protectionExtreme=${cfg.protectionExtreme ? 'true' : 'false'}
debugLogging=true

[logging]
logPath=logs/${fileName.replace(/\.ini$/i, '')}.csv
logEveryFrames=8
`;
  fs.writeFileSync(configPath, text, 'utf8');
  return { name: fileName, path: path.relative(root, configPath).replaceAll('\\', '/') };
}

function cleanFilePart(value) {
  return String(value || '').trim().replace(/\.[^.]+$/i, '').replace(/[^a-zA-Z0-9_-]+/g, '_') || 'clip';
}

function writeMarkers(payload) {
  const inputRel = String(payload.input || '');
  const inputPath = safeJoin(root, inputRel);
  if (!fs.existsSync(inputPath)) {
    throw new Error('Input WAV not found');
  }

  const markers = Array.isArray(payload.markers) ? payload.markers : [];
  const presetName = String(payload.presetName || 'preset');
  const presetPath = String(payload.presetPath || '');
  const annotationDir = path.join(root, 'captures', 'annotations');
  fs.mkdirSync(annotationDir, { recursive: true });

  const base = cleanFilePart(path.basename(inputPath));
  const presetBase = cleanFilePart(presetName);
  const fileName = `${base}.${presetBase}.markers.json`;
  const annotationPath = path.join(annotationDir, fileName);
  const body = {
    schemaVersion: 1,
    clip: {
      name: path.basename(inputPath),
      path: path.relative(root, inputPath).replaceAll('\\', '/'),
    },
    preset: {
      name: presetName,
      path: presetPath,
    },
    source: 'calibration_ui',
    markers: markers.map((marker) => ({
      kind: String(marker.kind || ''),
      timeSeconds: Number(marker.timeSeconds ?? marker.time ?? 0),
    })),
  };
  fs.writeFileSync(annotationPath, `${JSON.stringify(body, null, 2)}\n`, 'utf8');
  return {
    name: fileName,
    path: path.relative(root, annotationPath).replaceAll('\\', '/'),
    count: body.markers.length,
  };
}

function summarizeCsv(filePath) {
  if (!fs.existsSync(filePath)) return null;
  const lines = fs.readFileSync(filePath, 'utf8').trim().split(/\r?\n/);
  let maxFootstep = 0;
  let maxAction = 0;
  let maxProtection = 0;
  let maxPeak = 0;
  let footFrames = 0;
  let protectionFrames = 0;
  for (const line of lines.slice(1)) {
    const c = line.split(',');
    const foot = Number(c[1]);
    const action = Number(c[2]);
    const protection = Number(c[3]);
    const peak = Number(c[6]);
    maxFootstep = Math.max(maxFootstep, foot);
    maxAction = Math.max(maxAction, action);
    maxProtection = Math.max(maxProtection, protection);
    maxPeak = Math.max(maxPeak, peak);
    if (foot > 0.6) footFrames += 1;
    if (protection > 0.7) protectionFrames += 1;
  }
  return { maxFootstep, maxAction, maxProtection, maxPeak, footFrames, protectionFrames, rows: lines.length - 1 };
}

function readCsvSeries(filePath, maxPoints = 600) {
  if (!fs.existsSync(filePath)) return null;
  const lines = fs.readFileSync(filePath, 'utf8').trim().split(/\r?\n/).slice(1);
  const step = Math.max(1, Math.ceil(lines.length / maxPoints));
  const series = [];
  for (let i = 0; i < lines.length; i += step) {
    const c = lines[i].split(',');
    series.push({
      t: Number(c[0]) * 128 / 48000,
      footstep: Number(c[1]),
      action: Number(c[2]),
      protection: Number(c[3]),
      peak: Number(c[6]),
      stepDb: Number(c[7]),
      lowMidDb: Number(c[8]),
      bassDb: Number(c[9]),
      snrStep: Number(c[10]),
    });
  }
  return series;
}

function processWav(payload, res) {
  const inputRel = String(payload.input || '');
  const inputPath = safeJoin(root, inputRel);
  if (!fs.existsSync(inputPath)) {
    sendJson(res, { error: 'Input WAV not found' }, 404);
    return;
  }

  const base = path.basename(inputPath, path.extname(inputPath));
  const stamp = new Date().toISOString().replace(/[-:.TZ]/g, '').slice(0, 14);
  const outDir = path.join(root, 'captures', 'ui_processed');
  const logDir = path.join(root, 'captures', 'ui_logs');
  fs.mkdirSync(outDir, { recursive: true });
  fs.mkdirSync(logDir, { recursive: true });

  const outputPath = path.join(outDir, `${base}.${stamp}.processed.wav`);
  const logPath = path.join(logDir, `${base}.${stamp}.csv`);
  const configPath = writeTempConfig(payload.params || {}, path.relative(root, logPath));
  const tool = path.join(root, 'build', 'wav_process.exe');

  const child = spawn(tool, [inputPath, outputPath, configPath, logPath], { cwd: root, windowsHide: true });
  let stdout = '';
  let stderr = '';
  child.stdout.on('data', (chunk) => { stdout += chunk.toString(); });
  child.stderr.on('data', (chunk) => { stderr += chunk.toString(); });
  child.on('close', (code) => {
    if (code !== 0) {
      sendJson(res, { error: 'Processing failed', code, stdout, stderr }, 500);
      return;
    }
    sendJson(res, {
      output: path.relative(root, outputPath).replaceAll('\\', '/'),
      log: path.relative(root, logPath).replaceAll('\\', '/'),
      stdout,
      summary: summarizeCsv(logPath),
    });
  });
}

const server = http.createServer((req, res) => {
  try {
    const url = new URL(req.url, `http://localhost:${port}`);

    if (req.method === 'GET' && url.pathname === '/api/files') {
      sendJson(res, [
        ...listWavs(path.join(root, 'captures', 'raw')),
        ...listWavs(path.join(root, 'captures', 'raw_test')),
      ]);
      return;
    }

    if (req.method === 'GET' && url.pathname === '/api/presets') {
      sendJson(res, listPresets());
      return;
    }

    if (req.method === 'POST' && url.pathname === '/api/save-preset') {
      let body = '';
      req.on('data', (chunk) => { body += chunk.toString(); });
      req.on('end', () => {
        const payload = JSON.parse(body || '{}');
        sendJson(res, writePreset(payload.name, payload.params || {}));
      });
      return;
    }

    if (req.method === 'POST' && url.pathname === '/api/save-markers') {
      let body = '';
      req.on('data', (chunk) => { body += chunk.toString(); });
      req.on('end', () => {
        const payload = JSON.parse(body || '{}');
        sendJson(res, writeMarkers(payload));
      });
      return;
    }

    if (req.method === 'GET' && url.pathname === '/api/series') {
      const filePath = safeJoin(root, url.searchParams.get('path'));
      sendJson(res, readCsvSeries(filePath) || []);
      return;
    }

    if (req.method === 'GET' && url.pathname === '/media') {
      const filePath = safeJoin(root, url.searchParams.get('path'));
      serveMedia(req, res, filePath);
      return;
    }

    if (req.method === 'POST' && url.pathname === '/api/process') {
      let body = '';
      req.on('data', (chunk) => { body += chunk.toString(); });
      req.on('end', () => processWav(JSON.parse(body || '{}'), res));
      return;
    }

    let filePath = url.pathname === '/' ? path.join(publicDir, 'index.html') : safeJoin(publicDir, url.pathname);
    if (!fs.existsSync(filePath) || fs.statSync(filePath).isDirectory()) {
      sendJson(res, { error: 'Not found' }, 404);
      return;
    }
    const ext = path.extname(filePath).toLowerCase();
    res.writeHead(200, { 'Content-Type': mime[ext] || 'application/octet-stream', 'Cache-Control': 'no-store' });
    fs.createReadStream(filePath).pipe(res);
  } catch (error) {
    sendJson(res, { error: error.message }, 500);
  }
});

server.listen(port, () => {
  console.log(`Calibration UI: http://localhost:${port}`);
});
