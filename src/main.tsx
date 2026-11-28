import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { initAppearance } from "./stores/appearanceStore";

initAppearance();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
