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
        <div className="flex min-h-screen items-center justify-center bg-bg p-8">
          <div className="card max-w-md space-y-4 p-8 text-center">
            <span className="mx-auto grid h-14 w-14 place-items-center rounded-2xl bg-bad/10 text-bad">
              <AlertTriangle size={28} />
            </span>
            <h2 className="text-lg font-bold text-bad">Something went wrong</h2>
            <p className="break-words text-sm text-fg-muted">
              {this.state.error?.message || 'An unexpected error occurred.'}
            </p>
            <button
              onClick={() => {
                this.setState({ hasError: false, error: null });
                window.location.reload();
              }}
              className="btn btn-accent"
            >
              Reload page
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
