/**
 * Repack profiles.bin - standalone Node.js script
 * Replicates the Rust pack logic: EAPF v1 binary + AES-256-CBC encrypt
 */
const crypto = require('crypto');
const fs = require('fs');
const path = require('path');

const PROFILES_DIR = path.resolve(__dirname, 'equalizerAPO/Perfiles');
const OUTPUT_FILE = path.resolve(__dirname, 'instalacion/profiles.bin');

const PROFILE_NAMES = {
  1: 'Gaming / Footsteps',
  2: 'Misa',
  3: 'Loudness EQ',
  4: 'Full EQ',
};

function bundleKey() {
  return crypto.createHash('sha256').update('EchoAudioControl-Profiles-v1').digest();
}

function pushUtf8(buffers, text) {
  const encoded = Buffer.from(text, 'utf8');
  const lenBuf = Buffer.alloc(4);
  lenBuf.writeUInt32LE(encoded.length);
  buffers.push(lenBuf, encoded);
}

function packProfiles() {
  const buffers = [];
  // Header
  buffers.push(Buffer.from('EAPF'));
  const versionBuf = Buffer.alloc(4); versionBuf.writeUInt32LE(1); buffers.push(versionBuf);
  const countBuf = Buffer.alloc(4); countBuf.writeUInt32LE(Object.keys(PROFILE_NAMES).length); buffers.push(countBuf);

  const sortedIds = Object.keys(PROFILE_NAMES).map(Number).sort((a, b) => a - b);
  for (const id of sortedIds) {
    const dir = path.join(PROFILES_DIR, id.toString());
    const configPath = path.join(dir, 'config.txt');
    const strategyPath = path.join(dir, 'strategy.txt');

    if (!fs.existsSync(configPath)) {
      throw new Error(`Missing config.txt for profile ${id}: ${configPath}`);
    }

    const config = fs.readFileSync(configPath, 'utf8');
    let strategy = '';
    try { strategy = fs.readFileSync(strategyPath, 'utf8'); } catch {}

    console.log(`Profile ${id} "${PROFILE_NAMES[id]}": config=${config.length}b strategy=${strategy.length}b`);

    const idBuf = Buffer.alloc(4); idBuf.writeUInt32LE(id); buffers.push(idBuf);
    pushUtf8(buffers, PROFILE_NAMES[id]);
    pushUtf8(buffers, config);
    pushUtf8(buffers, strategy);
  }

  return Buffer.concat(buffers);
}

function encrypt(plaintext) {
  const key = bundleKey();
  const iv = crypto.randomBytes(16);
  const cipher = crypto.createCipheriv('aes-256-cbc', key, iv);
  const encrypted = Buffer.concat([cipher.update(plaintext), cipher.final()]);
  return Buffer.concat([iv, encrypted]);
}

// Main
const packed = packProfiles();
console.log(`\nPacked plaintext: ${packed.length} bytes`);

const encrypted = encrypt(packed);
fs.writeFileSync(OUTPUT_FILE, encrypted);

const stat = fs.statSync(OUTPUT_FILE);
console.log(`\nRESULT=OK`);
console.log(`BUNDLE_PATH=${OUTPUT_FILE}`);
console.log(`PROFILE_COUNT=${Object.keys(PROFILE_NAMES).length}`);
console.log(`BUNDLE_SIZE=${stat.size}`);
