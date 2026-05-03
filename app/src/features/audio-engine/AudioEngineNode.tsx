import { useState } from 'react';
import './core-reactor.css';

export const AudioEngineNode = () => {
  const [active, setActive] = useState(false);

  return (
    <div className={`reactor-wrapper ${active ? 'is-active' : ''}`} onClick={() => setActive(!active)}>
      {/* Visual rings */}
      <div className="reactor-ring outer"></div>
      <div className="reactor-ring inner">
        {Array.from({ length: 12 }).map((_, i) => (
          <div key={i} className="dial-tick" style={{ transform: `rotate(${i * 30}deg)` }}></div>
        ))}
      </div>
      
      {/* The main button */}
      <button className="reactor-core" type="button">
        <div className="core-background"></div>
        <div className="core-content">
          <span className="core-status">{active ? '0.1ms latency' : 'Ready'}</span>
          <span className="core-title">{active ? 'RUNNING' : 'START'}</span>
        </div>
      </button>
      
      {/* Shockwaves */}
      {active && (
        <>
          <div className="shockwave sw-1"></div>
          <div className="shockwave sw-2"></div>
        </>
      )}
    </div>
  );
};
