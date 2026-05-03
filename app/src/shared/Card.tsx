import React from 'react';

interface CardProps {
  title: string;
  subtitle?: string;
  children: React.ReactNode;
  className?: string;
  glow?: 'acid' | 'yellow' | 'none';
}

export const Card: React.FC<CardProps> = ({ 
  title, 
  subtitle, 
  children, 
  className = '',
  glow = 'none' 
}) => {
  const glowClass = glow !== 'none' ? `glow-${glow}` : '';
  
  return (
    <div className={`ui-card ${glowClass} ${className}`}>
      <div className="ui-card-header">
        <h3 className="ui-card-title">{title}</h3>
        {subtitle && <p className="ui-card-subtitle">{subtitle}</p>}
      </div>
      <div className="ui-card-content">
        {children}
      </div>
    </div>
  );
};
