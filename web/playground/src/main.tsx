import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Playground } from "./Playground";

// Mounts into the book's <div id="hv-play">. The coastlines' URL comes from
// its data-land attribute, relative to the page.
const root = document.getElementById("hv-play");
if (root) {
  const land = new URL(root.dataset.land ?? "playground/land-110m.json", document.baseURI).href;
  createRoot(root).render(
    <StrictMode>
      <Playground landUrl={land} />
    </StrictMode>,
  );
}
