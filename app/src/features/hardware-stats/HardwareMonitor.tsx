import { useState, useEffect } from 'react';
import './hardware-metrics.css';
import { Activity, Cpu } from 'lucide-react';

export const HardwareMonitor = () => {
  const [dspLoad, setDspLoad] = useState(4.2);
  const [latency, setLatency] = useState(0.12);

  useEffect(() => {
    const interval = setInterval(() => {
      // Simulate microscopic hardware jitter
      setDspLoad(+(4.0 + Math.random() * 0.9).toFixed(1));
      setLatency(+(0.11 + Math.random() * 0.04).toFixed(2));
    }, 1200);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="hud-panel left">
      <div className="hud-panel-header">
        <Cpu size={16} />
        <span>SYSTEM TELEMETRY</span>
      </div>

      <div className="telemetry-block">
        <div className="metric-row">
          <span className="metric-label">DSP LOAD</span>
          <span className="metric-value">{dspLoad}%</span>
        </div>
        <div className="metric-bar"><div className="metric-fill" style={{ width: `${dspLoad}%`, transition: 'width 0.3s' }}></div></div>
      </div>

      <div className="telemetry-block">
        <div className="metric-row">
          <span className="metric-label">MEMORY [L3 CACHE]</span>
          <span className="metric-value">12MB</span>
        </div>
        <div className="metric-bar"><div className="metric-fill" style={{ width: '15%' }}></div></div>
      </div>

      <div className="telemetry-block highlight">
        <div className="metric-row">
          <span className="metric-label"><Activity size={12} style={{ display: 'inline', marginRight: 4 }}/> LATENCY</span>
          <span className="metric-value text-yellow">{latency}ms</span>
        </div>
        <div className="metric-bar"><div className="metric-fill yellow" style={{ width: `${Math.max(10, latency * 100)}%`, transition: 'width 0.3s' }}></div></div>
      </div>

      <div className="telemetry-stats">
        <div className="stat-pill">
          <span className="pill-val">192kHz</span>
          <span className="pill-lbl">SAMPLE RATE</span>
        </div>
        <div className="stat-pill">
          <span className="pill-val">32-BIT</span>
          <span className="pill-lbl">DEPTH</span>
        </div>
      </div>
    </div>
  );
};
