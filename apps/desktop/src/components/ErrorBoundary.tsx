import { Component, ErrorInfo, ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
  info: string;
}

let logSeq = 0;

/** Forwards JS errors/unhandled rejections to the Rust log file. */
export function installGlobalErrorLogging() {
  window.addEventListener("error", (event) => {
    const loc = event.filename ? `${event.filename}:${event.lineno}` : "";
    invoke("frontend_log_cmd", {
      level: "error",
      message: String(event.message),
      location: loc,
    }).catch(() => {});
  });
  window.addEventListener("unhandledrejection", (event) => {
    invoke("frontend_log_cmd", {
      level: "error",
      message: `unhandled rejection: ${String(event.reason)}`,
      location: "",
    }).catch(() => {});
    logSeq += 0; // keep seq referenced
    void logSeq;
  });
}

export function logInfo(message: string) {
  invoke("frontend_log_cmd", { level: "info", message, location: "" }).catch(
    () => {},
  );
}

/**
 * Renders the error on screen instead of an empty white page, and logs it.
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, info: "" };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    this.setState({ info: errorInfo.componentStack ?? "" });
    invoke("frontend_log_cmd", {
      level: "error",
      message: `render crash: ${error.message}`,
      location: errorInfo.componentStack?.slice(0, 500) ?? "",
    }).catch(() => {});
  }

  render() {
    if (this.state.error) {
      return (
        <div className="p-6 text-sm">
          <h1 className="text-lg font-bold text-red-600">
            Something went wrong rendering this view
          </h1>
          <pre className="mt-2 overflow-auto rounded bg-slate-900 p-3 text-xs text-red-300">
            {this.state.error.message}
            {"\n"}
            {this.state.info}
          </pre>
          <button
            onClick={() => this.setState({ error: null, info: "" })}
            className="mt-3 rounded bg-blue-600 px-3 py-1 text-white"
          >
            Try again
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}