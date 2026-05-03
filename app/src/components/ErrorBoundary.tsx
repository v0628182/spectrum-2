import { Component, type ErrorInfo, type ReactNode } from 'react';

interface ErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

/**
 * Global error boundary — catches any unhandled React render errors
 * and shows a recovery UI instead of a white screen of death.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('[ErrorBoundary] Caught render crash:', error.message);
    console.error('[ErrorBoundary] Component stack:', info.componentStack);
  }

  handleReload = () => {
    this.setState({ hasError: false, error: null });
    window.location.reload();
  };

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) return this.props.fallback;

      return (
        <div style={{
          position: 'fixed',
          inset: 0,
          display: 'flex',
          flexDirection: 'column',
          alignItems: 'center',
          justifyContent: 'center',
          background: '#0a0a0e',
          color: '#fff',
          fontFamily: "'Inter', 'Segoe UI', sans-serif",
          zIndex: 99999,
          padding: '40px',
        }}>
          <div style={{
            fontSize: '48px',
            marginBottom: '16px',
            opacity: 0.5,
          }}>⚠</div>

          <h1 style={{
            fontSize: '22px',
            fontWeight: 700,
            marginBottom: '8px',
            color: '#eeff00',
            letterSpacing: '-0.5px',
          }}>
            Something went sideways
          </h1>

          <p style={{
            fontSize: '14px',
            color: '#666',
            maxWidth: '400px',
            textAlign: 'center',
            lineHeight: 1.6,
            marginBottom: '24px',
          }}>
            VanySound hit an unexpected error. Your audio settings are safe — just restart and you're back in business.
          </p>

          {this.state.error && (
            <pre style={{
              background: '#111',
              border: '1px solid #222',
              borderRadius: '8px',
              padding: '12px 16px',
              fontSize: '11px',
              color: '#ff4444',
              maxWidth: '500px',
              overflow: 'auto',
              marginBottom: '24px',
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-word',
            }}>
              {this.state.error.message}
            </pre>
          )}

          <button
            onClick={this.handleReload}
            style={{
              background: '#eeff00',
              color: '#0a0a0e',
              border: 'none',
              borderRadius: '8px',
              padding: '12px 32px',
              fontSize: '14px',
              fontWeight: 700,
              cursor: 'pointer',
              transition: 'opacity 0.2s',
            }}
            onMouseOver={(e) => (e.currentTarget.style.opacity = '0.85')}
            onMouseOut={(e) => (e.currentTarget.style.opacity = '1')}
          >
            RESTART APP
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}
