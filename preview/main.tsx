import React from "react";
import ReactDOM from "react-dom/client";

import App from "../src/App";
import { I18nProvider } from "../src/i18n";
import "../src/styles.css";
import { LOCALE, SCENE, runScene } from "./mocks/scene";

// The language picker reads localStorage, so seeding it before mount is all
// the screenshot script needs to shoot a locale. Always written, never left
// to chance: the browser profile is shared between captures, and one Arabic
// screenshot would otherwise turn every later shot Arabic too.
localStorage.setItem("coldmill.locale", LOCALE ?? "en");
localStorage.removeItem("coldmill.outputDir");
localStorage.setItem("coldmill.view", SCENE === "grid" ? "grid" : "list");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  // No StrictMode here: its double-mount would fire the scene twice.
  <I18nProvider>
    <App />
  </I18nProvider>,
);

void runScene();
