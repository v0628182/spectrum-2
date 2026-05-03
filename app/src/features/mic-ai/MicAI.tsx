import React, { useState, useMemo, useEffect, useCallback } from 'react';
import { Mic } from 'lucide-react';
import { openUrl } from '@tauri-apps/plugin-opener';
import { StealthSelect } from '../../shared/StealthSelect';
import { vanySoundApi, type AudioDevice } from '../../lib/vanysound';
import { devLog } from '../devlog/DevLogPanel';
import './mic-ai.css';

interface MicAIProps {
  engineActive: boolean;
}

const DEVICE_POLL_INTERVAL_MS = 5000;
const VOICEMEETER_URL = 'https://vb-audio.com/Voicemeeter/';

/** Build display-friendly option strings from raw device list. */
function buildDeviceOptions(devices: AudioDevice[]): string[] {
  return devices.map((device) => {
    const prefix = device.isActive ? '✦ ' : '';
    return `${prefix}${device.name}`;
  });
}

/** Resolve the display value that matches the currently active device. */
function resolveActiveDisplayValue(devices: AudioDevice[]): string {
  const active = devices.find((device) => device.isActive);
  if (active) return `✦ ${active.name}`;
  if (devices.length > 0) return devices[0].name;
  return 'No devices found';
}

/** Strip the active prefix to get the raw device name. */
function stripPrefix(display: string): string {
  return display.replace(/^✦\s*/, '');
}

export const MicAI: React.FC<MicAIProps> = ({ engineActive }) => {
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  const [selectedDisplay, setSelectedDisplay] = useState('SELECT YOUR HEADPHONES');
  const [switching, setSwitching] = useState(false);

  const fetchDevices = useCallback(async () => {
    try {
      devLog.info('AudioSettings', 'Fetching audio output devices...');
      const list = await vanySoundApi.listAudioOutputs();
      devLog.success('AudioSettings', `Found ${list.length} devices: ${list.map((d) => `"${d.name}"${d.isActive ? ' [ACTIVE]' : ''}`).join(', ')}`);
      setDevices(list);
      setSelectedDisplay((current) => {
        if (current === 'SELECT YOUR HEADPHONES' || current === 'No devices found') {
          return resolveActiveDisplayValue(list);
        }
        const currentName = stripPrefix(current);
        const activeDevice = list.find((d) => d.isActive);
        if (activeDevice && activeDevice.name !== currentName) {
          return resolveActiveDisplayValue(list);
        }
        return current;
      });
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      devLog.error('AudioSettings', `Failed to list devices: ${msg}`);
    }
  }, []);

  useEffect(() => {
    void fetchDevices();
    const interval = window.setInterval(() => void fetchDevices(), DEVICE_POLL_INTERVAL_MS);
    return () => window.clearInterval(interval);
  }, [fetchDevices]);

  const handleDeviceChange = async (displayValue: string) => {
    if (switching) return;
    
    if (displayValue.includes('Get Voicemeeter')) {
      void openUrl(VOICEMEETER_URL);
      return;
    }

    const selectedName = stripPrefix(displayValue);
    const device = devices.find((d) => d.name === selectedName);
    if (!device) {
      devLog.warn('AudioSettings', `Device not found for selection: "${selectedName}"`);
      return;
    }

    devLog.info('AudioSettings', `Switching output to: "${device.name}" (GUID: ${device.id})`);
    setSwitching(true);
    setSelectedDisplay(displayValue);
    try {
      await vanySoundApi.setAudioOutput(device.id);
      devLog.success('AudioSettings', `Output switched to: "${device.name}"`);
      await fetchDevices();
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      devLog.error('AudioSettings', `Switch FAILED for "${device.name}": ${msg}`);
    } finally {
      setSwitching(false);
    }
  };

  const deviceOptions = useMemo(() => buildDeviceOptions(devices), [devices]);

  // Generate bar data based on compressor gain
  const barData = useMemo(() => {
    const gainFactor = engineActive
      ? 1.4 - (80 / 100) * 0.6 // default gain equivalent
      : 0.8;

    return Array.from({ length: 22 }).map((_, i) => {
      const isWhite = i === 2 || i === 7 || i === 12 || i === 17 || i === 20;
      const isMedium = i % 2 !== 0 && !isWhite;
      const baseActiveHeight = Math.random() * 30 + 20;
      const baseInactiveHeight = Math.random() * 40 + 25;

      return {
        shadeClass: isWhite ? 'bar-white' : (isMedium ? 'bar-medium' : 'bar-dark'),
        activeHeight: Math.min(baseActiveHeight * gainFactor, 90),
        inactiveHeight: Math.min(baseInactiveHeight * gainFactor, 80),
        delay: i * 0.08,
        duration: Math.random() * 0.4 + 0.6
      };
    });
  }, [engineActive]);



  return (
    <div className={`opt-card ${engineActive ? 'glow-panel-acid' : ''}`}>
      <div className="opt-card-header" style={{ marginBottom: '16px' }}>
        <Mic size={20} />
        <span className="opt-card-title">AUDIO SETTINGS</span>
      </div>

      <StealthSelect
        options={deviceOptions.length > 0 ? [...deviceOptions, 'Get Voicemeeter'] : ['No devices found', 'Get Voicemeeter']}
        value={switching ? 'SWITCHING...' : selectedDisplay}
        onChange={(val) => void handleDeviceChange(val)}
      />

      <div className="frequency-visualizer">
        {/* Level meter on the left */}
        <div className="level-meter-wrapper" style={{ paddingLeft: '8px' }}>
          <div className="level-meter">
            <div className="level-bar-container">
              <div
                className="level-bar-fill"
                style={{
                  height: engineActive ? '36%' : '10%'
                }}
              ></div>
            </div>
          </div>
        </div>

        {/* Frequency bars */}
        <div className="mic-bars-container">
          {barData.map((bar, i) => (
            <div key={i} className="mic-bar-wrapper">
              <div
                className={`mic-signal-level ${bar.shadeClass} ${engineActive ? 'gain-low' : 'gain-standby'}`}
                style={{
                  height: engineActive ? `${bar.activeHeight}%` : `${bar.inactiveHeight}%`,
                  animationDelay: `${bar.delay}s`,
                  animationDuration: `${bar.duration}s`
                }}
              ></div>
            </div>
          ))}
        </div>
      </div>

      <div
        className="mic-controls"
        style={{
          opacity: engineActive ? 1 : 0.3,
          pointerEvents: engineActive ? 'auto' : 'none',
          transition: 'opacity 0.3s ease'
        }}
      >

      </div>



      <div className="eq-status" style={{ marginTop: 'auto' }}>
        <div className={`eq-status-indicator ${engineActive ? 'clean' : ''}`}></div>
        <span style={{ color: engineActive ? 'var(--text-primary)' : 'var(--text-secondary)' }}>
          {switching
            ? 'Switching output device...'
            : engineActive
              ? 'Audio processing enabled'
              : 'Standby'}
        </span>
      </div>
    </div>
  );
};
