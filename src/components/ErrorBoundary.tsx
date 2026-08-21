import { Component, type ReactNode } from "react";

interface Props {
  children: ReactNode;
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
    console.error("ErrorBoundary caught:", error, errorInfo);
  }

  handleReset = () => {
    this.setState({ hasError: false, error: null });
  };

  render() {
    if (this.state.hasError) {
      return (
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            height: "100vh",
            padding: 32,
            fontFamily: "system-ui, -apple-system, sans-serif",
            color: "var(--text-primary, #333)",
          }}
        >
          <div style={{ fontSize: 48, marginBottom: 16 }}>⚠️</div>
          <h2 style={{ fontSize: 20, marginBottom: 8 }}>应用出现异常</h2>
          <p
            style={{
              fontSize: 14,
              color: "var(--text-secondary, #666)",
              marginBottom: 24,
              maxWidth: 480,
              textAlign: "center",
              wordBreak: "break-all",
            }}
          >
            {this.state.error?.message || "未知错误"}
          </p>
          <button
            onClick={this.handleReset}
            style={{
              padding: "10px 24px",
              borderRadius: 6,
              border: "1px solid var(--border-color, #ddd)",
              background: "var(--card-bg, #fff)",
              cursor: "pointer",
              fontSize: 14,
            }}
          >
            重试
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}
