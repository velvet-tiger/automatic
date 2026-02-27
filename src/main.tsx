import React from "react";
import ReactDOM from "react-dom/client";
import { ClerkProvider } from "@clerk/clerk-react";
import { dark } from "@clerk/themes";
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

const PUBLISHABLE_KEY = import.meta.env.VITE_CLERK_PUBLISHABLE_KEY;

if (!PUBLISHABLE_KEY) {
  throw new Error("Missing VITE_CLERK_PUBLISHABLE_KEY in environment variables");
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <ClerkProvider
        publishableKey={PUBLISHABLE_KEY}
        allowedRedirectOrigins={["tauri://localhost", "https://tauri.localhost"]}
        appearance={{
          baseTheme: dark,
          variables: {
            colorPrimary: "#5E6AD2",
            colorBackground: "#1A1A1E",
            colorInputBackground: "#222327",
            colorInputText: "#F8F8FA",
            borderRadius: "0.375rem",
          },
        }}
      >
        <App />
      </ClerkProvider>
    </ErrorBoundary>
  </React.StrictMode>,
);
