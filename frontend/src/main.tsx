import React from "react";
import ReactDOM from "react-dom/client";
import { App } from "./app/App";
import { AppProviders } from "./app/AppProviders";
import { createAppI18n } from "./i18n/createAppI18n";
import { resolveInitialLocale } from "./i18n/localeBootstrap";
import { applyWindowChromeMode } from "./layouts/app/WindowTitleBar";
import "./styles/index.css";

async function bootstrap() {
  applyWindowChromeMode();

  const rootElement = document.getElementById("root");
  if (!rootElement) {
    throw new Error("Root element #root not found");
  }

  let storedLocale: string | null = null;
  try {
    storedLocale = window.localStorage.getItem("assetiweave.locale");
  } catch {
    // Ignore storage errors in restricted contexts
  }

  const initialLocale = resolveInitialLocale(storedLocale);

  try {
    const i18n = await createAppI18n(initialLocale);
    ReactDOM.createRoot(rootElement).render(
      <React.StrictMode>
        <AppProviders i18n={i18n}>
          <App />
        </AppProviders>
      </React.StrictMode>,
    );
  } catch (error) {
    console.error("Failed to initialize application i18n:", error);
    ReactDOM.createRoot(rootElement).render(
      <div
        role="alert"
        style={{
          padding: 32,
          fontFamily: "system-ui, -apple-system, sans-serif",
          color: "#ef4444",
          background: "#0f172a",
          minHeight: "100vh",
          boxSizing: "border-box",
        }}
      >
        <h2 style={{ fontSize: 20, marginBottom: 12 }}>
          Application Initialization Error
        </h2>
        <p style={{ color: "#94a3b8", fontSize: 14, lineHeight: 1.6 }}>
          {error instanceof Error ? error.message : String(error)}
        </p>
      </div>,
    );
  }
}

void bootstrap();
