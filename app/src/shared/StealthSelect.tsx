import React, { useState, useRef, useEffect } from 'react';
import { ChevronDown } from 'lucide-react';

interface StealthSelectProps {
  options: string[];
  value: string;
  onChange: (val: string) => void;
  variant?: 'default' | 'minimal';
}

export const StealthSelect: React.FC<StealthSelectProps> = ({ options, value, onChange, variant = 'default' }) => {
  const [isOpen, setIsOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  return (
    <div className={`stealth-select-wrapper ${variant}`} ref={containerRef}>
      <div 
        className={`stealth-select-display ${variant === 'minimal' ? 'minimal' : ''} ${isOpen ? 'open' : ''}`}
        onClick={() => setIsOpen(!isOpen)}
      >
        <span>{value}</span>
        <ChevronDown size={14} className={`stealth-select-icon ${isOpen ? 'rotated' : ''}`} />
      </div>
      
      {isOpen && (
        <div className="stealth-select-dropdown">
          {options.map((opt) => (
            <div 
              key={opt}
              className={`stealth-select-option ${opt === value ? 'selected' : ''}`}
              onClick={() => {
                onChange(opt);
                setIsOpen(false);
              }}
            >
              {opt}
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
