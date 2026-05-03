import { vanySoundApi } from './vanysound';

let diagnosticsStarted = false;

function serialize(value: unknown): string {
  if (value instanceof Error) {
    return `${value.name}: ${value.message}\n${value.stack ?? ''}`.trim();
  }

  if (typeof value === 'string') {
    return value;
  }

  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

async function push(level: string, values: unknown[]) {
  const message = values.map(serialize).join(' ');
  try {
    await vanySoundApi.appendFrontendLog(level, message);
  } catch {
    // Best effort only.
  }
}

export function startDiagnosticsCapture() {
  if (diagnosticsStarted) {
    return;
  }

  diagnosticsStarted = true;
  const originalConsole = {
    error: console.error.bind(console),
    info: console.info.bind(console),
    log: console.log.bind(console),
    warn: console.warn.bind(console),
  };

  console.log = (...args: unknown[]) => {
    void push('log', args);
    originalConsole.log(...args);
  };

  console.info = (...args: unknown[]) => {
    void push('info', args);
    originalConsole.info(...args);
  };

  console.warn = (...args: unknown[]) => {
    void push('warn', args);
    originalConsole.warn(...args);
  };

  console.error = (...args: unknown[]) => {
    void push('error', args);
    originalConsole.error(...args);
  };

  window.addEventListener('error', (event) => {
    void push('error', [
      `window.error:${event.message}`,
      event.filename,
      `line=${event.lineno}`,
      `col=${event.colno}`,
      event.error,
    ]);
  });

  window.addEventListener('unhandledrejection', (event) => {
    void push('error', ['unhandledrejection', event.reason]);
  });

  void push('info', ['frontend diagnostics initialized']);
}
