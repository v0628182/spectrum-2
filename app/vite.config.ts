import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import obfuscatorPlugin from 'rollup-plugin-obfuscator'

// https://vite.dev/config/
export default defineConfig(({ command }) => ({
  base: command === 'build' ? './' : '/',
  plugins: [react()],
  build: {
    rollupOptions: {
      plugins: [
        // Light obfuscation — production only
        // Conservative settings to avoid antivirus false positives:
        // ✓ Variable renaming, string array, console removal
        // ✗ NO dead code injection, NO self-defending, NO debug protection
        obfuscatorPlugin({
          options: {
            compact: true,
            controlFlowFlattening: false,    // Heavy → AV trigger
            deadCodeInjection: false,         // Heavy → AV trigger
            debugProtection: false,           // AV trigger
            disableConsoleOutput: true,        // Remove console.log
            identifierNamesGenerator: 'hexadecimal',
            renameGlobals: false,             // Keep exports intact
            selfDefending: false,             // AV trigger
            stringArray: true,                // Encode strings
            stringArrayEncoding: ['base64'],  // Light encoding
            stringArrayThreshold: 0.5,        // Only encode 50% of strings
            splitStrings: true,
            splitStringsChunkLength: 10,
            transformObjectKeys: false,       // Keep object keys readable
            unicodeEscapeSequence: false,     // AV trigger
          },
        }),
      ],
    },
  },
}))
