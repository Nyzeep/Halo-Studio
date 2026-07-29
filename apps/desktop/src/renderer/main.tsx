import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App.js";
import "./app.css";

const root = document.getElementById("root");

if (root === null) throw new Error("Renderer root is unavailable.");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
