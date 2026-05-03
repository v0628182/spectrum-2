import React, { useEffect, useRef, useState } from 'react';
import { Sliders } from 'lucide-react';
import './audio-eq.css';

interface AudioEQProps {
  engineActive: boolean;
}

export const AudioEQ: React.FC<AudioEQProps> = ({ engineActive }) => {
  const [bars, setBars] = useState<number[]>(Array(32).fill(15));
  const animationRef = useRef<number | null>(null);

  useEffect(() => {
    if (!engineActive) {
      if (animationRef.current) {
        cancelAnimationFrame(animationRef.current);
        animationRef.current = null;
      }
      setBars(Array(32).fill(15));
      return;
    }

    const animate = () => {
      setBars(prev => {
        const time = Date.now() / 1000;
        return prev.map((_, i) => {
          // Create a wave pattern with multiple sine waves
          const wave1 = Math.sin(time * 2 + i * 0.3) * 30;
          const wave2 = Math.sin(time * 3.5 + i * 0.5) * 20;
          const wave3 = Math.sin(time * 1.2 + i * 0.2) * 15;
          const noise = Math.random() * 10 - 5;
          
          // Combine waves and clamp between 5 and 95
          const value = 40 + wave1 + wave2 + wave3 + noise;
          return Math.max(5, Math.min(95, value));
        });
      });
      animationRef.current = requestAnimationFrame(animate);
    };

    animate();

    return () => {
      if (animationRef.current) {
        cancelAnimationFrame(animationRef.current);
      }
    };
  }, [engineActive]);

  return (
    <div className={`opt-card ${engineActive ? 'glow-panel' : ''}`}>
      <div className="opt-card-header">
        <Sliders size={20} />
        <span className="opt-card-title">FREQUENCY OPTIMIZER</span>
      </div>

      <div className="eq-wave-container">
        {bars.map((height, i) => (
          <div
            key={i}
            className={`eq-wave-bar ${engineActive ? 'active' : ''}`}
            style={{ 
              height: `${height}%`,
              transitionDelay: `${i * 0.02}s`
            }}
          />
        ))}
      </div>

      <div className="eq-status">
        <div className="eq-status-indicator"></div>
        <span>EQ adjustments: {engineActive ? 'Active' : 'Off'}</span>
      </div>
    </div>
  );
};
