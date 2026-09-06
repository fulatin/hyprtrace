import { Component, type ReactNode } from 'react';
import { AlertTriangle } from 'lucide-react';

interface Props {
  children: ReactNode;
  /**
   * When any value in this array changes (e.g. the current route path), the
   * error state is reset. This lets a user recover from a render error by
   * navigating to another route instead of forcing a full page reload.
   */
  resetKeys?: unknown[];
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export default class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: React.ErrorInfo) {
    console.error('ErrorBoundary caught:', error, errorInfo);
  }

  componentDidUpdate(prevProps: Props) {
    // If a reset key changed (e.g. the user navigated to another route), clear
    // the error so the new view can render instead of showing the fallback.
    if (
      this.state.hasError &&
      prevProps.resetKeys !== this.props.resetKeys &&
      !shallowEqualKeys(prevProps.resetKeys, this.props.resetKeys)
    ) {
      this.setState({ hasError: false, error: null });
    }
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="min-h-screen bg-gray-950 flex items-center justify-center p-8">
          <div className="bg-gray-900 border border-gray-800 rounded-lg p-8 max-w-md text-center space-y-4">
            <AlertTriangle size={48} className="text-red-400 mx-auto" />
            <h2 className="text-lg font-bold text-red-400">Something went wrong</h2>
            <p className="text-sm text-gray-400">
              {this.state.error?.message || 'An unexpected error occurred.'}
            </p>
            <button
              onClick={() => {
                this.setState({ hasError: false, error: null });
                window.location.reload();
              }}
              className="bg-cyan-600 hover:bg-cyan-700 text-white rounded-md px-4 py-2 text-sm transition-colors"
            >
              Reload Page
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

/** Compare two arrays of reset keys by value (shallow). */
function shallowEqualKeys(a?: unknown[], b?: unknown[]): boolean {
  if (a === b) return true;
  if (!a || !b) return false;
  if (a.length !== b.length) return false;
  return a.every((v, i) => Object.is(v, b[i]));
}
