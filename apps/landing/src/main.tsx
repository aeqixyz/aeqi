import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter, Routes, Route } from "react-router-dom";
import "./index.css";
import App from "./App";
import Enterprise from "./Enterprise";
import Terms from "./Terms";
import Privacy from "./Privacy";
import Brand from "./Brand";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <BrowserRouter>
      <Routes>
        <Route path="/" element={<App />} />
        <Route path="/pricing" element={<Enterprise />} />
        <Route path="/terms" element={<Terms />} />
        <Route path="/privacy" element={<Privacy />} />
        <Route path="/brand" element={<Brand />} />
      </Routes>
    </BrowserRouter>
  </StrictMode>
);
