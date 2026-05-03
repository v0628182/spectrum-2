import React, { useState, useRef, useEffect, useCallback } from 'react';
import { Terminal, Copy, Trash2, ChevronDown, ChevronUp, Check } from 'lucide-react';
import './devlog.css';

export interface LogEntry {
  timestamp: string;
  level: 'info' | 'warn' | 'error' | 'success' | 'debug';
  source: string;
  message: string;
}

/** Maximum entries kept in memory */
const MAX_LOG_ENTRIES = 500;

/** Global log store — accessible from anywhere via devLog.* */
const logStore: LogEntry[] = [];
const listeners: Set<() => void> = new Set();

function notifyListeners() {
  listeners.forEach((fn) => fn());
}

function formatTimestamp(): string {
  const now = new Date();
  const hours = String(now.getHours()).padStart(2, '0');
  const minutes = String(now.getMinutes()).padStart(2, '0');
  const seconds = String(now.getSeconds()).padStart(2, '0');
  const millis = String(now.getMilliseconds()).padStart(3, '0');
  return `${hours}:${minutes}:${seconds}.${millis}`;
}

function pushEntry(entry: LogEntry) {
  logStore.push(entry);
  if (logStore.length > MAX_LOG_ENTRIES) {
    logStore.splice(0, logStore.length - MAX_LOG_ENTRIES);
  }
  notifyListeners();
}

/** Public API — import { devLog } from this module */
export const devLog = {
  info(source: string, message: string) {
    pushEntry({ timestamp: formatTimestamp(), level: 'info', source, message });
  },
  warn(source: string, message: string) {
    pushEntry({ timestamp: formatTimestamp(), level: 'warn', source, message });
  },
  error(source: string, message: string) {
    pushEntry({ timestamp: formatTimestamp(), level: 'error', source, message });
  },
  success(source: string, message: string) {
    pushEntry({ timestamp: formatTimestamp(), level: 'success', source, message });
  },
  debug(source: string, message: string) {
    pushEntry({ timestamp: formatTimestamp(), level: 'debug', source, message });
  },
  clear() {
    logStore.length = 0;
    notifyListeners();
  },
  getAll(): LogEntry[] {
    return [...logStore];
  },
  /** Serialize all entries for clipboard */
  serialize(): string {
    return logStore
      .map((entry) => `[${entry.timestamp}] [${entry.level.toUpperCase()}] [${entry.source}] ${entry.message}`)
      .join('\n');
  },
};

/** Hook to subscribe to log updates */
function useLogEntries(): LogEntry[] {
  const [, setTick] = useState(0);

  useEffect(() => {
    const handler = () => setTick((t) => t + 1);
    listeners.add(handler);
    return () => { listeners.delete(handler); };
  }, []);

  return logStore;
}

/** Intercept native console methods to capture ALL output */
export function installConsoleInterceptor() {
  const originalLog = console.log;
  const originalWarn = console.warn;
  const originalError = console.error;

  console.log = (...args: unknown[]) => {
    originalLog.apply(console, args);
    const message = args.map((a) => (typeof a === 'string' ? a : JSON.stringify(a))).join(' ');
    if (!message.includes('[DevLog]')) {
      devLog.debug('console', message);
    }
  };

  console.warn = (...args: unknown[]) => {
    originalWarn.apply(console, args);
    const message = args.map((a) => (typeof a === 'string' ? a : JSON.stringify(a))).join(' ');
    devLog.warn('console', message);
  };

  console.error = (...args: unknown[]) => {
    originalError.apply(console, args);
    const message = args.map((a) => (typeof a === 'string' ? a : JSON.stringify(a))).join(' ');
    devLog.error('console', message);
  };

  // Capture unhandled promise rejections
  window.addEventListener('unhandledrejection', (event) => {
    const reason = event.reason instanceof Error ? event.reason.message : String(event.reason);
    devLog.error('unhandled', `Promise rejected: ${reason}`);
  });

  // Capture global errors
  window.addEventListener('error', (event) => {
    devLog.error('window', `${event.message} at ${event.filename}:${event.lineno}`);
  });

  devLog.info('DevLog', 'Console interceptor installed. All output captured.');
}

/** Level badge color mapping */
const LEVEL_COLORS: Record<string, string> = {
  info: 'var(--clr-acid)',
  warn: '#f59e0b',
  error: '#ef4444',
  success: '#22c55e',
  debug: 'var(--text-muted)',
};

interface DevLogPanelProps {
  defaultOpen?: boolean;
}

export const DevLogPanel: React.FC<DevLogPanelProps> = ({ defaultOpen = false }) => {
  const entries = useLogEntries();
  const [isOpen, setIsOpen] = useState(defaultOpen);
  const [copied, setCopied] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const autoScrollRef = useRef(true);

  // Auto-scroll to bottom on new entries
  useEffect(() => {
    if (autoScrollRef.current && scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [entries.length]);

  const handleScroll = useCallback(() => {
    if (!scrollRef.current) return;
    const { scrollTop, scrollHeight, clientHeight } = scrollRef.current;
    autoScrollRef.current = scrollHeight - scrollTop - clientHeight < 40;
  }, []);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(devLog.serialize());
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      devLog.error('DevLog', 'Failed to copy to clipboard');
    }
  };

  const handleClear = () => {
    devLog.clear();
  };

  const entryCount = entries.length;
  const errorCount = entries.filter((e) => e.level === 'error').length;

  return (
    <div className={`devlog-container ${isOpen ? 'open' : 'collapsed'}`}>
      {/* Header / Toggle bar */}
      <button
        className="devlog-header"
        onClick={() => setIsOpen(!isOpen)}
        type="button"
      >
        <div className="devlog-header-left">
          <Terminal size={13} />
          <span className="devlog-title">DEV LOG</span>
          <span className="devlog-count">{entryCount}</span>
          {errorCount > 0 && (
            <span className="devlog-error-badge">{errorCount} ERR</span>
          )}
        </div>
        <div className="devlog-header-right">
          {isOpen && (
            <>
              <button
                className="devlog-action-btn"
                onClick={(e) => { e.stopPropagation(); void handleCopy(); }}
                title="Copy all logs"
                type="button"
              >
                {copied ? <Check size={12} /> : <Copy size={12} />}
                <span>{copied ? 'COPIED' : 'COPY'}</span>
              </button>
              <button
                className="devlog-action-btn danger"
                onClick={(e) => { e.stopPropagation(); handleClear(); }}
                title="Clear logs"
                type="button"
              >
                <Trash2 size={12} />
              </button>
            </>
          )}
          {isOpen ? <ChevronDown size={14} /> : <ChevronUp size={14} />}
        </div>
      </button>

      {/* Log body */}
      {isOpen && (
        <div
          className="devlog-body"
          ref={scrollRef}
          onScroll={handleScroll}
        >
          {entries.length === 0 && (
            <div className="devlog-empty">No log entries yet.</div>
          )}
          {entries.map((entry, index) => (
            <div key={index} className={`devlog-entry level-${entry.level}`}>
              <span className="devlog-ts">{entry.timestamp}</span>
              <span
                className="devlog-level"
                style={{ color: LEVEL_COLORS[entry.level] ?? 'var(--text-muted)' }}
              >
                {entry.level.toUpperCase().padEnd(7)}
              </span>
              <span className="devlog-source">{entry.source}</span>
              <span className="devlog-msg">{entry.message}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
