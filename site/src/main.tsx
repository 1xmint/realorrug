// SPDX-License-Identifier: Apache-2.0
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
// The four faces, before the stylesheet that names them (design 0025 §5).
// Latin subsets only for the two static faces: this page is English, and the
// others are bytes nobody requests. The two variable faces ship every subset
// behind `unicode-range`, so a reader still fetches only the latin file.
import "@fontsource/anton/latin-400.css";
import "@fontsource/special-elite/latin-400.css";
import "@fontsource-variable/ibm-plex-sans/wght.css";
import "@fontsource-variable/jetbrains-mono/wght.css";
import "./index.css";

const root = document.getElementById("root");
if (!root) throw new Error("no #root");
createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
