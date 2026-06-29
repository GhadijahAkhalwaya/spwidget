import { listen } from "@tauri-apps/api/event";
import { isConfigured } from "./api";
import { renderSetup } from "./views/setup";
import { renderWidget } from "./views/widget";

async function bootstrap() {
  const root = document.getElementById("app");
  if (!root) throw new Error("missing #app");

  const configured = await isConfigured();
  if (configured) {
    renderWidget(root);
  } else {
    renderSetup(root, () => renderWidget(root));
  }

  // Re-open setup if backend signals expired auth
  await listen<string>("auth-expired", () => {
    renderSetup(root, () => renderWidget(root));
  });
}

bootstrap().catch((err) => {
  const root = document.getElementById("app");
  if (root) {
    root.innerHTML = `<div class="setup"><div class="error">Startup error: ${
      err?.message ?? String(err)
    }</div></div>`;
  }
});
