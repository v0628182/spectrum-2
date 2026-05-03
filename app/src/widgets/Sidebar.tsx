import React from 'react';
import { Activity, Headphones, Mic, Settings, Zap } from 'lucide-react';
import './Sidebar.css';

interface SidebarProps {
  activeTab: string;
  setActiveTab: (tab: string) => void;
}

export const Sidebar: React.FC<SidebarProps> = ({ activeTab, setActiveTab }) => {
  const tabs = [
    { id: 'dashboard', label: 'Dashboard', icon: Activity },
    { id: 'spatial', label: 'Spatial Audio', icon: Headphones },
    { id: 'mic', label: 'Mic Clarity', icon: Mic },
    { id: 'settings', label: 'Config', icon: Settings }
  ];

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <div className="status-badge">
          <div className="status-dot pulses"></div>
          ENGINE ACTIVE
        </div>
      </div>
      
      <nav className="sidebar-nav">
        {tabs.map((tab) => {
          const Icon = tab.icon;
          const isActive = activeTab === tab.id;
          return (
            <button 
              key={tab.id}
              className={`sidebar-nav-item ${isActive ? 'active' : ''}`}
              onClick={() => setActiveTab(tab.id)}
            >
              <Icon size={20} className="sidebar-icon" />
              <span>{tab.label}</span>
              {isActive && <div className="active-indicator" />}
            </button>
          );
        })}
      </nav>
      
      <div className="sidebar-footer">
        <div className="premium-box">
          <Zap size={16} color="var(--clr-acid)" />
          <div className="premium-text">
            <strong>PRO TIER</strong>
            <span>Active</span>
          </div>
        </div>
      </div>
    </aside>
  );
};
