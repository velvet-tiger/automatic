import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import ErrorBoundary from "./ErrorBoundary";

// Surface unhandled JS errors and unhandled promise rejections so they are
// visible in Tauri's webview inspector rather than causing a silent black screen.
window.onerror = (message, source, lineno, colno, error) => {
  console.error("[window.onerror]", { message, source, lineno, colno, error });
};

window.addEventListener("unhandledrejection", (event) => {
  console.error("[unhandledrejection]", event.reason);
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
