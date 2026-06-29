import { LogicalSize } from "@tauri-apps/api/dpi";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import {
  clearCredentials,
  getConfig,
  getProjectKey,
  refreshNow,
  saveCredentials,
  setProjectKey,
} from "../api";
import type { FieldChoice, JiraError } from "../types";

/** Pick the best story-points field from a list of candidates.
 *  Priority:
 *    1. Name contains both "story" and "point" (case-insensitive)  — score 2
 *    2. ID contains "story" or "point"                             — score 1
 *  When multiple candidates tie on score, prefer the one with the
 *  lowest customfield number (older fields tend to have populated data
 *  in classic Jira projects).
 *  Returns null if no confident match is found. */
function bestStoryPointsMatch(candidates: FieldChoice[]): FieldChoice | null {
  const score = (c: FieldChoice): number => {
    const name = c.name.toLowerCase();
    const id   = c.id.toLowerCase();
    if (name.includes("story") && name.includes("point")) return 2;
    if (id.includes("story")   || id.includes("point"))   return 1;
    return 0;
  };
  // Extract numeric suffix from customfield_XXXXX for tie-breaking
  const fieldNum = (id: string): number => {
    const m = id.match(/customfield_(\d+)/);
    return m ? parseInt(m[1], 10) : Number.MAX_SAFE_INTEGER;
  };
  let best: { c: FieldChoice; s: number } | null = null;
  for (const c of candidates) {
    const s = score(c);
    if (s === 0) continue;
    if (!best || s > best.s || (s === best.s && fieldNum(c.id) < fieldNum(best.c.id))) {
      best = { c, s };
    }
  }
  return best ? best.c : null;
}

export function renderSetup(root: HTMLElement, onDone: () => void) {
  // Grow window for the form
  void getCurrentWindow().setSize(new LogicalSize(320, 380));

  root.innerHTML = "";
  const wrap = document.createElement("div");
  wrap.className = "setup";
  wrap.dataset.tauriDragRegion = "";

  wrap.innerHTML = `
    <h1>Connect to Jira</h1>
    <p class="hint">Enter your Jira URL and an API token (Cloud) or
      Personal Access Token (Server / Data Center). Token is stored
      in macOS Keychain.</p>

    <label>Jira URL
      <input id="url" type="url" placeholder="https://impact.atlassian.net"
             autocomplete="off" spellcheck="false">
    </label>
    <label>Email or username
      <input id="user" type="text" placeholder="name.surname@impact.com"
             autocomplete="off" spellcheck="false">
    </label>
    <label>API token / PAT
      <input id="token" type="password" autocomplete="off"
             spellcheck="false">
      <span class="hint token-hint">
        <a id="token-link" href="#">Get your API token ↗</a>
      </span>
    </label>
    <label>Project key (optional)
      <input id="project-key" type="text" placeholder="e.g. IRD"
             autocomplete="off" spellcheck="false"
             style="text-transform: uppercase">
      <span class="hint">Leave empty to count Done issues across all projects.</span>
    </label>

    <div id="field-pick" style="display:none">
      <label>Story points field (auto-detect failed)
        <select id="field-select"></select>
      </label>
    </div>

    <div id="error" class="error" style="display:none"></div>

    <button id="submit" class="primary">Test &amp; save</button>
    <p class="hint">
      Note: <code>assignee = currentUser()</code> matches the
      <em>current</em> assignee. Tickets reassigned after resolution
      drop out of the count.
    </p>
  `;

  root.append(wrap);

  const urlInput = wrap.querySelector<HTMLInputElement>("#url")!;
  const userInput = wrap.querySelector<HTMLInputElement>("#user")!;
  const tokenInput = wrap.querySelector<HTMLInputElement>("#token")!;
  const projectInput = wrap.querySelector<HTMLInputElement>("#project-key")!;
  const fieldPick = wrap.querySelector<HTMLDivElement>("#field-pick")!;
  const fieldSelect = wrap.querySelector<HTMLSelectElement>("#field-select")!;
  const submitBtn = wrap.querySelector<HTMLButtonElement>("#submit")!;
  const errorBox = wrap.querySelector<HTMLDivElement>("#error")!;

  const showError = (msg: string) => {
    errorBox.textContent = msg;
    errorBox.style.display = "block";
  };
  const hideError = () => {
    errorBox.style.display = "none";
  };

  const populateCandidates = (candidates: FieldChoice[]) => {
    fieldSelect.innerHTML = "";
    for (const c of candidates) {
      const opt = document.createElement("option");
      opt.value = c.id;
      opt.textContent = `${c.name} (${c.id})`;
      fieldSelect.append(opt);
    }
    fieldPick.style.display = "block";
  };

  // Pre-fill non-secret config if it exists (only token is needed on re-setup)
  void getConfig().then((cfg) => {
    if (cfg) {
      urlInput.value = cfg.base_url;
      userInput.value = cfg.user;
      tokenInput.placeholder = "Enter your API token";
      tokenInput.focus();
    } else {
      // Default URL for new setups
      urlInput.value = "https://impact.atlassian.net";
    }
  });
  // Pre-fill saved project key (preserved across reconfigure)
  void getProjectKey().then((pk) => { projectInput.value = pk ?? ""; });

  // Open API token page in system browser
  const tokenLink = wrap.querySelector<HTMLAnchorElement>("#token-link")!;
  tokenLink.addEventListener("click", (e) => {
    e.preventDefault();
    void openUrl("https://id.atlassian.com/manage-profile/security/api-tokens");
  });

  submitBtn.addEventListener("click", async () => {
    hideError();
    const url = urlInput.value.trim();
    const user = userInput.value.trim();
    const token = tokenInput.value;
    if (!url || !user || !token) {
      showError("All fields are required.");
      return;
    }

    submitBtn.disabled = true;
    submitBtn.textContent = "Connecting…";

    // Save project key + refresh + finish, used by every success path.
    const finishSetup = async () => {
      const pk = projectInput.value.trim().toUpperCase();
      projectInput.value = pk; // reflect uppercase
      await setProjectKey(pk === "" ? null : pk);
      submitBtn.textContent = "Loading…";
      await refreshNow();
      onDone();
    };

    try {
      const fieldId = fieldPick.style.display === "block"
        ? fieldSelect.value
        : undefined;

      const result = await saveCredentials({ url, user, token, fieldId });

      if (result.kind === "needs_field_pick") {
        // Try to auto-match a candidate whose name contains both "story" and "point"
        const autoMatch = bestStoryPointsMatch(result.candidates);
        if (autoMatch) {
          // Retry immediately with the matched field
          const retry = await saveCredentials({ url, user, token, fieldId: autoMatch.id });
          if (retry.kind === "ok") {
            await finishSetup();
            return;
          }
        }
        // No confident match — show the picker
        populateCandidates(result.candidates);
        showError(
          "Could not auto-detect the Story Points field. " +
          "Please pick it from the list and try again.",
        );
        submitBtn.disabled = false;
        submitBtn.textContent = "Test & save";
        return;
      }

      await finishSetup();
    } catch (err) {
      const e = err as JiraError;
      submitBtn.disabled = false;
      submitBtn.textContent = "Test & save";
      if (e?.kind === "Auth") {
        showError("Authentication failed — check your URL, email, and token.");
      } else if (e?.kind === "Network") {
        showError(`Network error: ${e.message}`);
      } else if (e?.kind === "NoStoryPointsField" && e.candidates) {
        const autoMatch = bestStoryPointsMatch(e.candidates);
        if (autoMatch) {
          // Retry silently with the best match
          submitBtn.disabled = true;
          submitBtn.textContent = "Connecting…";
          try {
            const retry = await saveCredentials({ url, user, token, fieldId: autoMatch.id });
            if (retry.kind === "ok") {
              await finishSetup();
              return;
            }
          } catch (_) {/* fall through to picker */}
        }
        populateCandidates(e.candidates);
        showError("Pick the Story Points field manually.");
        submitBtn.disabled = false;
        submitBtn.textContent = "Test & save";
      } else {
        showError(e?.message ?? String(err));
      }
    }
  });

  // Provide a "clear" affordance for re-setup case
  const clearBtn = document.createElement("button");
  clearBtn.className = "icon-btn";
  clearBtn.textContent = "Clear saved credentials";
  clearBtn.style.marginTop = "4px";
  clearBtn.style.fontSize = "11px";
  clearBtn.style.width = "auto";
  clearBtn.style.height = "auto";
  clearBtn.style.padding = "4px 6px";
  clearBtn.addEventListener("click", async () => {
    await clearCredentials();
    urlInput.value = "";
    userInput.value = "";
    tokenInput.value = "";
    projectInput.value = "";
    fieldPick.style.display = "none";
    hideError();
  });
  wrap.append(clearBtn);
}
