const { spawnSync } = require('node:child_process');
const path = require('node:path');

const launcher = path.join(__dirname, 'abrir-app3.vbs');

const result = spawnSync('wscript.exe', [launcher], {
  cwd: __dirname,
  windowsHide: true,
  stdio: 'ignore',
});

if (result.error) {
  console.error(`[EchoAudio launcher] No se pudo abrir ${launcher}: ${result.error.message}`);
  process.exitCode = 1;
}
