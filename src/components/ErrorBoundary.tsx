import { Component, ErrorInfo, ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
  errorInfo: ErrorInfo | null;
}

class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { error: null, errorInfo: null };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    this.setState({ error, errorInfo });
    console.error("[ErrorBoundary] Uncaught error:", error, errorInfo);
  }

  render() {
    if (this.state.error) {
      return (
        <div
          style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            height: "100vh",
            width: "100vw",
            background: "#222327",
            color: "#E0E1E6",
            fontFamily: "monospace",
            padding: "2rem",
            boxSizing: "border-box",
          }}
        >
          <div
            style={{
              maxWidth: "640px",
              width: "100%",
              background: "#1A1A1E",
              border: "1px solid #33353A",
              borderRadius: "6px",
              padding: "1.5rem",
            }}
          >
            <h1
              style={{
                margin: "0 0 0.5rem",
                fontSize: "16px",
                fontWeight: 600,
                color: "#F8F8FA",
              }}
            >
              Application error
            </h1>
            <p
              style={{
                margin: "0 0 1rem",
                fontSize: "13px",
                color: "#C8CAD0",
              }}
            >
              {this.state.error.message}
            </p>
            <details
              style={{
                fontSize: "11px",
                color: "#888",
                whiteSpace: "pre-wrap",
                wordBreak: "break-word",
              }}
            >
              <summary style={{ cursor: "pointer", marginBottom: "0.5rem" }}>
                Stack trace
              </summary>
              <pre style={{ margin: 0 }}>
                {this.state.error.stack}
                {"\n\nComponent stack:"}
                {this.state.errorInfo?.componentStack}
              </pre>
            </details>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

export default ErrorBoundary;
