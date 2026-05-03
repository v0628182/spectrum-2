import { Card } from '../shared/Card';
import { AudioEngineNode } from '../features/audio-engine/AudioEngineNode';
import './dashboard.css';

export const DashboardPage = () => {
  return (
    <div className="dashboard-container">
      <header className="dashboard-header">
        <div>
          <h1 className="dashboard-title">System Overview</h1>
          <p className="dashboard-subtitle">Control the fastest engine in esports.</p>
        </div>
      </header>

      <div className="dashboard-scroll-area">
        <AudioEngineNode />
        
        <div className="dashboard-grid">
          <Card title="Hardware Monitor" className="hw-monitor-card">
            <div className="hw-stat-grid">
              <div className="hw-stat">
                <span className="hw-label">DSP LOAD</span>
                <span className="hw-value">4.2%</span>
                <div className="progress-bar"><div className="progress-bar-fill" style={{ width: '4.2%' }}></div></div>
              </div>
              <div className="hw-stat">
                <span className="hw-label">MEMORY</span>
                <span className="hw-value">12MB</span>
                <div className="progress-bar"><div className="progress-bar-fill" style={{ width: '15%' }}></div></div>
              </div>
              <div className="hw-stat">
                <span className="hw-label">SAMPLE RATE</span>
                <span className="hw-value">192kHz</span>
                <div className="progress-bar"><div className="progress-bar-fill yellow" style={{ width: '100%' }}></div></div>
              </div>
            </div>
          </Card>

          <Card title="Active Profile">
            <div className="profile-selector">
              <div className="profile-item active">
                <span className="profile-name">WARZONE META</span>
                <span className="badge">GOD TIER</span>
              </div>
              <div className="profile-item">
                <span className="profile-name">APEX LEGENDS</span>
              </div>
              <div className="profile-item">
                <span className="profile-name">CS2 FOOTSTEPS</span>
              </div>
            </div>
          </Card>
        </div>
      </div>
    </div>
  );
};
