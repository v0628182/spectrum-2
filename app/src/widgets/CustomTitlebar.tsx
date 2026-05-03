import { useState } from 'react';
import { X, Minus, Square, Settings } from 'lucide-react';
import { SettingsModal } from './SettingsModal';
import { controlDesktopWindow, type WindowControlCommand } from '../shared/desktopWindow';
import logoUrl from '../assets/Group_Logos/Group 48.svg';

interface CustomTitlebarProps {
  busyLabel?: string | null;
  onRequestRepair?: () => void;
  runtime?: unknown;
}

export const CustomTitlebar: React.FC<CustomTitlebarProps> = ({
  busyLabel,
  onRequestRepair,
  runtime: _runtime,
} = {}) => {
  const [settingsOpen, setSettingsOpen] = useState(false);
  const handleControl = (command: WindowControlCommand) => {
    void controlDesktopWindow(command);
  };

  return (
    <>
      <div className="titlebar" data-tauri-drag-region>
        <div className="titlebar-brand" data-tauri-drag-region>
        <img 
          src={logoUrl} 
          alt="Vany Sound" 
          data-tauri-drag-region 
          style={{ width: 20, height: 'auto', display: 'block' }} 
        />
        <span data-tauri-drag-region style={{ color: '#FFFFFF', textTransform: 'lowercase', letterSpacing: '0.5px', fontSize: '13px', fontWeight: 600 }}>vanysound.com</span>
      </div>
      
      <div className="titlebar-controls">
        <button className="titlebar-btn" onClick={() => setSettingsOpen(true)} aria-label="Settings">
          <Settings size={15} />
        </button>
        <button className="titlebar-btn" onClick={() => handleControl('min')} aria-label="Minimize">
          <Minus size={16} />
        </button>
        <button className="titlebar-btn" onClick={() => handleControl('max')} aria-label="Maximize">
          <Square size={14} />
        </button>
        <button className="titlebar-btn close" onClick={() => handleControl('close')} aria-label="Close">
          <X size={18} />
        </button>
      </div>
      </div>

      <SettingsModal
        busyLabel={busyLabel}
        isOpen={settingsOpen}
        onClose={() => setSettingsOpen(false)}
        onRequestRepair={onRequestRepair}
      />
    </>
  );
};
