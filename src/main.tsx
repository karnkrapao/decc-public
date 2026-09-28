import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

const colorScheme = window.matchMedia("(prefers-color-scheme: dark)");

function syncTheme() {
  const root = document.documentElement;
  const mode = root.dataset.themeMode ?? "system";
  root.classList.toggle(
    "dark",
    mode === "dark" || (mode === "system" && colorScheme.matches),
  );
}

syncTheme();
colorScheme.addEventListener("change", syncTheme);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
