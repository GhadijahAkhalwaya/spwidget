import confetti from "canvas-confetti";
import { LogicalSize, PhysicalPosition } from "@tauri-apps/api/dpi";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  clearCredentials,
  getMode,
  getPoints,
  quitApp,
  refreshNow,
  sendToBack,
  sendToFront,
  setMode,
} from "../api";
import { renderIssueList } from "../components/issueList";
import { renderSetup } from "./setup";
import type { RefreshFailed, Snapshot } from "../types";

// Window dimensions with 1.4× zoom: 400×287 expanded, 286×205 shrunk.
// The visible widget is 280×198 logical pre-zoom (= 392×277 post-zoom),
// plus a 6×7 transparent buffer on right+bottom to avoid edge-clip artefacts.
const EXPANDED = new LogicalSize(400, 287);
const SHRINK   = new LogicalSize(286, 205);

const THRESHOLDS = {
  days90:  { smug: 120, boss: 135 },
  monthly: { smug: 39,  boss: 45  },
} as const;

const SMUG_CATS      = ["😼", "😤", "🐱", "😾", "🙀", "😹"];
const BOSS_CATS      = ["😎", "👑", "🔥", "💎", "⚡"];
const MEOWNSTER_CATS = ["✨", "💫", "🌟", "🦋", "🌈", "🪄", "⭐", "🫧"];
const LION_ROAR_BUFFER = 16; // points above boss threshold that triggers the 🦁 easter egg
let catFrame = 0;

type ThemeOverride = "auto" | "smug" | "boss";
type Format = "chill" | "motivate";

// ── Working-day helpers (used by Motivate format) ────────────────────────
const MOTIVATE_SMUG_PTS = THRESHOLDS.monthly.smug; // keep in sync with threshold table

function workingDaysInMonth(year: number, month: number): number {
  const days = new Date(year, month + 1, 0).getDate();
  let n = 0;
  for (let d = 1; d <= days; d++) {
    const dow = new Date(year, month, d).getDay();
    if (dow > 0 && dow < 6) n++;
  }
  return n;
}

function workingDaysUpTo(year: number, month: number, day: number): number {
  let n = 0;
  for (let d = 1; d <= day; d++) {
    const dow = new Date(year, month, d).getDay();
    if (dow > 0 && dow < 6) n++;
  }
  return n;
}

function isOnTrack(totalPoints: number): boolean {
  if (totalPoints >= MOTIVATE_SMUG_PTS) return true;
  const now   = new Date();
  const year  = now.getFullYear();
  const month = now.getMonth();
  const today = now.getDate();
  const totalWD   = workingDaysInMonth(year, month);
  const elapsedWD = workingDaysUpTo(year, month, today);
  if (elapsedWD === 0 || totalWD === 0) return false;
  // Project current pace to end of month
  const projected = (totalPoints / elapsedWD) * totalWD;
  return projected >= MOTIVATE_SMUG_PTS;
}

export async function renderWidget(root: HTMLElement) {
  // Load size preference from localStorage; default to expanded
  const isExpanded = (localStorage.getItem("jpw-expanded") ?? "true") === "true";
  await getCurrentWindow().setSize(isExpanded ? EXPANDED : SHRINK);
  document.documentElement.style.zoom = isExpanded ? "1.4" : "1.0";
  root.classList.toggle("shrink-mode", !isExpanded);

  // ── Persisted window position ──────────────────────────────────────────
  // macOS occasionally shifts windows when level changes (Mission Control,
  // display config changes, Spaces). We snapshot the user's chosen position
  // and re-assert it after every wake to keep the widget exactly where they
  // placed it.
  const win = getCurrentWindow();
  const POS_KEY = "jpw-window-pos";

  const savePosition = async () => {
    try {
      const p = await win.outerPosition();
      localStorage.setItem(POS_KEY, JSON.stringify({ x: p.x, y: p.y }));
    } catch { /* ignore */ }
  };
  const restorePosition = async () => {
    try {
      const raw = localStorage.getItem(POS_KEY);
      if (!raw) return;
      const { x, y } = JSON.parse(raw);
      if (typeof x === "number" && typeof y === "number") {
        await win.setPosition(new PhysicalPosition(x, y));
      }
    } catch { /* ignore */ }
  };

  // Restore the user's chosen position immediately on widget load.
  await restorePosition();

  root.innerHTML = "";
  const widget = document.createElement("div");
  widget.className = "widget";
  widget.dataset.tauriDragRegion = "";
  root.append(widget);

  // Dev mode banner — shown at top of widget when dev mode triggered
  const devModeBanner = document.createElement("div");
  devModeBanner.id = "dev-banner";
  devModeBanner.className = "widget__dev-banner";
  devModeBanner.textContent = "Hey Dijah :)";
  devModeBanner.style.display = "none";
  widget.append(devModeBanner);

  // Mode/theme banner — hidden until a threshold is crossed
  const catBanner = document.createElement("div");
  catBanner.className = "widget__cat-banner";
  catBanner.textContent = "smug thug mode";
  widget.append(catBanner);

  const catAscii = document.createElement("div");
  catAscii.className = "widget__cat-ascii";
  catAscii.textContent = "😼";
  widget.append(catAscii);

  // Right-side: refresh + settings
  const buttons = document.createElement("div");
  buttons.className = "widget__buttons";
  buttons.dataset.tauriDragRegion = "false";
  buttons.innerHTML = `
    <button id="refresh" class="icon-btn" title="Refresh">↻</button>
    <button id="settings" class="icon-btn" title="Settings">⚙</button>
  `;
  widget.append(buttons);

  // Left-side: close + resize buttons (harmonised with right-side icons)
  const winButtons = document.createElement("div");
  winButtons.className = "widget__win-buttons";
  winButtons.dataset.tauriDragRegion = "false";
  winButtons.innerHTML = `
    <button id="win-close" class="icon-btn" title="Close">×</button>
    <button id="win-resize" class="icon-btn" title="Shrink/Expand">↔</button>
  `;
  widget.append(winButtons);

  winButtons.querySelector<HTMLButtonElement>("#win-resize")!.addEventListener("click", async () => {
    const isCurrentlyExpanded = document.documentElement.style.zoom === "1.4" || !document.documentElement.style.zoom;
    const newExpanded = !isCurrentlyExpanded;
    localStorage.setItem("jpw-expanded", newExpanded ? "true" : "false");
    await getCurrentWindow().setSize(newExpanded ? EXPANDED : SHRINK);
    document.documentElement.style.zoom = newExpanded ? "1.4" : "1.0";
    root.classList.toggle("shrink-mode", !newExpanded);
  });

  winButtons.querySelector<HTMLButtonElement>("#win-close")!.addEventListener("click", async () => {
    // Force the whole process to exit (not just close the window).
    // ActivationPolicy::Accessory keeps the app alive otherwise.
    await quitApp();
  });

  // Settings menu — appears below the ⚙ button
  const settingsMenu = document.createElement("div");
  settingsMenu.className = "smenu";
  settingsMenu.dataset.tauriDragRegion = "false";
  settingsMenu.innerHTML = `
    <div id="theme-section" style="display:none">
      <div class="smenu__label">Mode</div>
      <div class="smenu__pills" id="theme-pills">
        <button class="smenu__pill smenu__pill--active" data-theme="auto">Auto</button>
        <button class="smenu__pill" data-theme="smug">Thug</button>
        <button class="smenu__pill" data-theme="boss">Meownster</button>
      </div>
      <div class="smenu__divider"></div>
    </div>
    <div class="smenu__label">Period</div>
    <div class="smenu__pills" id="period-pills">
      <button class="smenu__pill smenu__pill--active" data-period="days90">90 Days</button>
      <button class="smenu__pill" data-period="monthly">This Month</button>
    </div>
    <div class="smenu__divider"></div>
    <div class="smenu__label">Setting</div>
    <div class="smenu__pills" id="format-pills">
      <button class="smenu__pill smenu__pill--active" data-format="chill">Chill</button>
      <button class="smenu__pill" data-format="motivate">Motivate</button>
    </div>
    <div id="thr-section">
      <div class="smenu__divider"></div>
      <div class="smenu__label">Thresholds</div>
      <div class="smenu__thr-row">
        <span class="smenu__thr-label">Thug</span>
        <input type="text" id="thr-smug" class="smenu__thr-input" min="1">
      </div>
      <div class="smenu__thr-row">
        <span class="smenu__thr-label">Meownster</span>
        <input type="text" id="thr-boss" class="smenu__thr-input" min="1">
      </div>
    </div>
    <div class="smenu__divider"></div>
    <button class="smenu__action" id="reconfigure-btn">⚙ Reconfigure login</button>
  `;
  widget.append(settingsMenu);

  const points = document.createElement("div");
  points.className = "widget__points";
  points.textContent = "—";
  widget.append(points);

  const sub = document.createElement("div");
  sub.className = "widget__sub";
  sub.textContent = "loading…";
  widget.append(sub);

  const footer = document.createElement("div");
  footer.className = "widget__footer";
  footer.innerHTML = `
    <span><span id="status-dot" class="widget__status status-stale"></span><span id="status-text">—</span></span>
    <span id="updated">—</span>
  `;
  widget.append(footer);

  // ── State ──────────────────────────────────────────────────────────────
  let issuesEl: HTMLElement | null = null;
  let expanded = false;
  let isToggling = false;         // reentrancy guard for toggleExpanded
  let lastSnapshot: Snapshot | null = null;
  let updatedTimer: number | null = null;
  let catTimer: number | null = null;
  let catTimerMs   = 10_000;   // current interval; meownster uses 25 s
  let wasBoss      = false;
  let wasMeownster = false;
  let themeOverride: ThemeOverride = "auto";
  let currentMode: "days90" | "monthly" = "days90";
  let format: Format = (localStorage.getItem("jpw-format") as Format | null) ?? "chill";

  // Custom threshold overrides — null means "use default"
  type ModeThr = { smug: number | null; boss: number | null };
  const loadCustomThr = (): { days90: ModeThr; monthly: ModeThr } => {
    try { return JSON.parse(localStorage.getItem("jpw-thresholds") || "null")
            ?? { days90: { smug: null, boss: null }, monthly: { smug: null, boss: null } };
    } catch { return { days90: { smug: null, boss: null }, monthly: { smug: null, boss: null } }; }
  };
  let customThr = loadCustomThr();

  const getEffThr = (mode: "days90" | "monthly") => ({
    smug: customThr[mode].smug ?? THRESHOLDS[mode].smug,
    boss: customThr[mode].boss ?? THRESHOLDS[mode].boss,
  });

  // ── Settings menu helpers ───────────────────────────────────────────────
  const setMenuOpen = (open: boolean) => {
    settingsMenu.classList.toggle("open", open);
  };

  const syncThresholdInputs = () => {
    const smugInput = settingsMenu.querySelector<HTMLInputElement>("#thr-smug");
    const bossInput = settingsMenu.querySelector<HTMLInputElement>("#thr-boss");
    if (!smugInput || !bossInput) return;
    smugInput.placeholder = String(THRESHOLDS[currentMode].smug);
    bossInput.placeholder = String(THRESHOLDS[currentMode].boss);
    smugInput.value = customThr[currentMode].smug != null ? String(customThr[currentMode].smug) : "";
    bossInput.value = customThr[currentMode].boss != null ? String(customThr[currentMode].boss) : "";
  };

  const syncPills = () => {
    settingsMenu.querySelectorAll<HTMLButtonElement>("[data-theme]").forEach(btn => {
      btn.classList.toggle("smenu__pill--active", btn.dataset.theme === themeOverride);
    });
    settingsMenu.querySelectorAll<HTMLButtonElement>("[data-period]").forEach(btn => {
      btn.classList.toggle("smenu__pill--active", btn.dataset.period === currentMode);
    });
    settingsMenu.querySelectorAll<HTMLButtonElement>("[data-format]").forEach(btn => {
      btn.classList.toggle("smenu__pill--active", btn.dataset.format === format);
    });
    // Thresholds always visible regardless of format
    syncThresholdInputs();
  };

  // ── Confetti ────────────────────────────────────────────────────────────
  const fireConfetti = () => {
    confetti({
      particleCount: 120,
      spread: 110,
      origin: { x: 0.5, y: 0.55 },
      colors: ["#ff6b6b", "#ffd700", "#00cfff", "#ff69b4", "#7b68ee", "#32cd32"],
      startVelocity: 32,
      gravity: 0.85,
    });
    setTimeout(() => {
      confetti({ particleCount: 60, spread: 55, origin: { x: 0.2, y: 0.75 }, colors: ["#ffd700", "#ff6b6b"] });
      confetti({ particleCount: 60, spread: 55, origin: { x: 0.8, y: 0.75 }, colors: ["#00cfff", "#7b68ee"] });
    }, 280);
  };

  const fireSparkleConfetti = () => {
    const rect = widget.getBoundingClientRect();
    const cx = (rect.left + rect.width / 2) / window.innerWidth;
    const cy = (rect.top + rect.height / 2) / window.innerHeight;
    confetti({
      particleCount: 55,
      spread: 75,
      origin: { x: cx, y: cy },
      colors: ["#ffffff", "#c084fc", "#a78bfa", "#67e8f9", "#fde68a", "#f9a8d4"],
      startVelocity: 18,
      gravity: 0.45,
      scalar: 0.85,
      shapes: ["star"],
    });
  };

  // ── Theme application ───────────────────────────────────────────────────
  const applyTheme = (totalPoints: number) => {
    let isBoss      = false;
    let isSmug      = false;
    let isMeownster = false;

    if (format === "motivate") {
      isMeownster = isOnTrack(totalPoints);
    } else {
      // Chill format: static thresholds + optional manual override
      if (themeOverride === "boss") {
        isBoss = true;
      } else if (themeOverride === "smug") {
        isSmug = true;
      } else {
        const thr = getEffThr(currentMode);
        isBoss = totalPoints > thr.boss;
        isSmug = !isBoss && totalPoints > thr.smug;
      }
    }

    // Lion easter egg: triggers when SP tally is LION_ROAR_BUFFER above the boss threshold
    const lionThr  = getEffThr(currentMode).boss + LION_ROAR_BUFFER;
    const isLionRoar = isBoss && totalPoints >= lionThr;

    root.classList.toggle("boss-mode",      isBoss);
    root.classList.toggle("smug-thug",      isSmug);
    root.classList.toggle("meownster-mode", isMeownster);
    root.classList.toggle("lion-roar",      isLionRoar);
    widget.classList.toggle("boss-mode",      isBoss);
    widget.classList.toggle("smug-thug",      isSmug);
    widget.classList.toggle("meownster-mode", isMeownster);
    widget.classList.toggle("lion-roar",      isLionRoar);

    // Update subtext based on currently-active mode (themeOverride aware)
    if (lastSnapshot) {
      const n = lastSnapshot.issue_count;
      sub.textContent = isMeownster
        ? `${n} issue${n === 1 ? "" : "s"} · in the zone`
        : isBoss
          ? `${n} issue${n === 1 ? "" : "s"} · crushin' 🔥`
          : isSmug
            ? `${n} issue${n === 1 ? "" : "s"} · flexin' 😤`
            : `${n} issue${n === 1 ? "" : "s"} · warmin' up`;
    }

    if (isBoss      && !wasBoss)      fireConfetti();
    if (isMeownster && !wasMeownster) fireSparkleConfetti();
    wasBoss      = isBoss;
    wasMeownster = isMeownster;

    const anyActive = isBoss || isSmug || isMeownster;
    if (anyActive) {
      // Banner names are swapped between boss and meownster CSS states so the
      // Chill progression reads as: smug thug → meownster → lyin'.
      // In Motivate, the meownster CSS triggers (on-track / "in the zone"),
      // and its banner now reads "total boss mode".
      catBanner.textContent = isLionRoar
        ? "YOU'RE LYIN'!"
        : isBoss
          ? "✨ meownster mode ✨"
          : isMeownster
            ? "Totally locked in"
            : "smug thug mode";

      // Lion easter egg: lock the emoji to 🦁, no rotation
      if (isLionRoar) {
        if (catTimer) { window.clearInterval(catTimer); catTimer = null; }
        catAscii.textContent = "🦁";
      } else {
        const cats     = isBoss ? BOSS_CATS : isMeownster ? MEOWNSTER_CATS : SMUG_CATS;
        const neededMs = isMeownster ? 25_000 : 10_000;
        // Restart timer if not running or interval changed (e.g. smug ↔ meownster)
        if (!catTimer || catTimerMs !== neededMs) {
          if (catTimer) { window.clearInterval(catTimer); catTimer = null; }
          catTimerMs = neededMs;
          catAscii.textContent = cats[catFrame % cats.length]!;
          catTimer = window.setInterval(() => {
            // Pause rotation if we've crossed into lion-roar territory
            if (root.classList.contains("lion-roar")) return;
            const cur = root.classList.contains("boss-mode")      ? BOSS_CATS
                      : root.classList.contains("meownster-mode") ? MEOWNSTER_CATS
                      : SMUG_CATS;
            catFrame = (catFrame + 1) % cur.length;
            catAscii.textContent = cur[catFrame]!;
          }, catTimerMs);
        } else {
          catAscii.textContent = cats[catFrame % cats.length]!;
        }
      }
    } else {
      if (catTimer) { window.clearInterval(catTimer); catTimer = null; }
      catAscii.textContent = "😼";
      catFrame = 0;
      catBanner.textContent = "smug thug mode";
    }
  };

  // ── Status helpers ──────────────────────────────────────────────────────
  const setStatus = (state: "fresh" | "stale" | "error", label: string) => {
    const dot = footer.querySelector<HTMLSpanElement>("#status-dot")!;
    const txt = footer.querySelector<HTMLSpanElement>("#status-text")!;
    dot.className = `widget__status status-${state}`;
    txt.textContent = label;
  };

  const setUpdated = (snap: Snapshot | null) => {
    const el = footer.querySelector<HTMLSpanElement>("#updated")!;
    el.textContent = snap ? formatRelative(snap.fetched_at) : "—";
  };

  // ── Snapshot rendering ──────────────────────────────────────────────────
  const renderSnapshot = (snap: Snapshot) => {
    lastSnapshot = snap;
    points.textContent = formatPoints(snap.total_points);

    // Reset manual theme override so the data-driven theme takes over
    themeOverride = "auto";
    syncPills();

    // applyTheme handles both CSS classes and subtext (uses lastSnapshot)
    applyTheme(snap.total_points);
    setUpdated(snap);

    const ageHours = (Date.now() - new Date(snap.fetched_at).getTime()) / 3.6e6;
    setStatus(ageHours < 26 ? "fresh" : "stale", ageHours < 26 ? "Up to date" : "Stale");

    if (expanded && issuesEl) {
      const fresh = renderIssueList(snap.issues, () => { void toggleExpanded(); });
      issuesEl.replaceWith(fresh);
      issuesEl = fresh;
      void sizeToContent();
    }
  };

  // ── Window sizing ───────────────────────────────────────────────────────
  const sizeToContent = async () => {
    await new Promise(r => requestAnimationFrame(r));
    await new Promise(r => requestAnimationFrame(r));
    const needed = widget.scrollHeight;
    const capped = Math.min(needed, window.screen.height - 80);
    // +10 for the transparent right/bottom buffer (7px × 1.4 zoom ≈ 10)
    await getCurrentWindow().setSize(new LogicalSize(400, capped + 10));
  };

  const toggleExpanded = async () => {
    if (isToggling) return;   // drop re-entrant calls while resize is in flight
    isToggling = true;
    expanded = !expanded;
    widget.classList.toggle("expanded", expanded);
    if (expanded) {
      if (lastSnapshot) {
        issuesEl = renderIssueList(lastSnapshot.issues, () => { void toggleExpanded(); });
        widget.append(issuesEl);
        await sizeToContent();
      } else {
        await getCurrentWindow().setSize(new LogicalSize(400, 458));
      }
    } else {
      issuesEl?.remove();
      issuesEl = null;
      await getCurrentWindow().setSize(SHRINK);
    }
    isToggling = false;
  };

  // ── Event wiring ────────────────────────────────────────────────────────
  // Settings cog toggles the menu
  buttons.querySelector("#refresh")!.addEventListener("click", () => { void doRefresh(); });
  buttons.querySelector("#settings")!.addEventListener("click", (e) => {
    e.stopPropagation();
    setMenuOpen(!settingsMenu.classList.contains("open"));
  });

  // Theme pills
  settingsMenu.querySelectorAll<HTMLButtonElement>("[data-theme]").forEach(btn => {
    btn.addEventListener("click", () => {
      themeOverride = btn.dataset.theme as ThemeOverride;
      syncPills();
      applyTheme(lastSnapshot?.total_points ?? 0);
      setMenuOpen(false); // close immediately so clicking away won't also expand
    });
  });

  // Period pills
  settingsMenu.querySelectorAll<HTMLButtonElement>("[data-period]").forEach(btn => {
    btn.addEventListener("click", async () => {
      const newMode = btn.dataset.period as "days90" | "monthly";
      if (newMode === currentMode) return;
      currentMode = newMode;
      syncPills();
      setMenuOpen(false); // close before the async call so no gap while menu is open
      await setMode(newMode);
      void doRefresh();
    });
  });

  // Format pills (Chill / Motivate)
  settingsMenu.querySelectorAll<HTMLButtonElement>("[data-format]").forEach(btn => {
    btn.addEventListener("click", () => {
      format = btn.dataset.format as Format;
      localStorage.setItem("jpw-format", format);
      syncPills();
      setMenuOpen(false);
      applyTheme(lastSnapshot?.total_points ?? 0);
    });
  });

  // Threshold inputs
  const onThrChange = () => {
    const smugInput = settingsMenu.querySelector<HTMLInputElement>("#thr-smug")!;
    const bossInput = settingsMenu.querySelector<HTMLInputElement>("#thr-boss")!;

    const parseThreshold = (inputValue: string): number | null => {
      // Extract just the numeric part (remove "mw" suffix if present)
      const numStr = inputValue.replace(/[mw]/g, "");
      const val = parseInt(numStr);
      // Must be a valid number >= 20, or null if empty
      if (isNaN(val)) return null;
      if (val < 20) return null;  // Reject values below 20
      return val;
    };

    const smugVal = parseThreshold(smugInput.value);
    const bossVal = parseThreshold(bossInput.value);

    // Reset inputs if they fail validation
    if (smugVal === null && smugInput.value.replace(/[mw]/g, "") !== "") {
      smugInput.value = "";
    }
    if (bossVal === null && bossInput.value.replace(/[mw]/g, "") !== "") {
      bossInput.value = "";
    }

    customThr[currentMode].smug = smugVal;
    customThr[currentMode].boss = bossVal;
    localStorage.setItem("jpw-thresholds", JSON.stringify(customThr));
    applyTheme(lastSnapshot?.total_points ?? 0);
  };
  settingsMenu.querySelector("#thr-smug")!.addEventListener("change", onThrChange);
  settingsMenu.querySelector("#thr-boss")!.addEventListener("change", onThrChange);

  // Reconfigure — two-step inline confirmation (window.confirm is blocked in WKWebView)
  const reconfigureBtn = settingsMenu.querySelector<HTMLButtonElement>("#reconfigure-btn")!;
  let reconfigurePending = false;
  reconfigureBtn.addEventListener("click", async (e) => {
    e.stopPropagation(); // prevent menu from closing on first click
    if (!reconfigurePending) {
      reconfigurePending = true;
      reconfigureBtn.textContent = "⚠ Tap again to sign out";
      reconfigureBtn.style.color = "#f87171";
      // Auto-reset if user doesn't confirm within 3 seconds
      setTimeout(() => {
        reconfigurePending = false;
        reconfigureBtn.textContent = "⚙ Reconfigure login";
        reconfigureBtn.style.color = "";
      }, 3000);
    } else {
      reconfigurePending = false;
      setMenuOpen(false);
      await clearCredentials();
      renderSetup(root, () => renderWidget(root));
    }
  });

  // Click outside closes menu
  document.addEventListener("click", () => setMenuOpen(false));
  settingsMenu.addEventListener("click", (e) => e.stopPropagation());

  const doRefresh = async () => {
    setStatus("stale", "Refreshing…");
    try {
      const snap = await refreshNow();
      renderSnapshot(snap);
    } catch (err: any) {
      const kind = err?.kind ?? "Network";
      setStatus("error", kind === "Auth" ? "Auth failed — click ⚙" : "Offline");
    }
  };

  // ── Initial load ────────────────────────────────────────────────────────
  try { currentMode = await getMode(); } catch { /* ignore, default days90 */ }
  syncPills();

  try {
    const cached = await getPoints();
    if (cached) renderSnapshot(cached);
  } catch { /* ignore */ }
  void doRefresh();

  // ── Live events ─────────────────────────────────────────────────────────
  const unsubs: UnlistenFn[] = [];
  unsubs.push(await listen<Snapshot>("points-updated", (e) => renderSnapshot(e.payload)));
  unsubs.push(await listen<RefreshFailed>("refresh-failed", (e) => {
    setStatus("error", e.payload.reason || "refresh failed");
  }));
  // Menu-bar icon click → wake widget (same as clicking the widget directly)
  unsubs.push(await listen("tray-wake", () => { void wakeUp(); }));
  // Save user-initiated drags so they survive hibernation. Skip moves that
  // happen while hibernating — those are macOS shifts, not user intent.
  let movedTimer: number | null = null;
  unsubs.push(await win.onMoved(() => {
    if (hibernating) return;
    if (movedTimer !== null) window.clearTimeout(movedTimer);
    // Debounce so a single drag (which fires many move events) only writes once.
    movedTimer = window.setTimeout(() => { void savePosition(); }, 150);
  }));

  updatedTimer = window.setInterval(() => setUpdated(lastSnapshot), 60_000);

  // ── Developer Mode (secret: type "meow" in threshold field to toggle)
  // Toggles on first password, stays on across refreshes, toggles off when
  // the password is typed a second time.
  const themeSection = settingsMenu.querySelector("#theme-section") as HTMLElement;
  let devMode = false;

  const setDevMode = (on: boolean) => {
    devMode = on;
    if (themeSection) themeSection.style.display = on ? "block" : "none";
    const devBanner = widget.querySelector("#dev-banner") as HTMLElement;
    if (devBanner) {
      if (on) {
        // Replay the fade-in animation each time it's turned on.
        devBanner.style.animation = "none";
        void devBanner.offsetHeight;
        devBanner.style.animation = "";
        devBanner.style.display = "block";
      } else {
        devBanner.style.display = "none";
      }
    }
  };

  // Detect "meow" in threshold inputs to toggle dev mode
  const thrSmugInput = settingsMenu.querySelector<HTMLInputElement>("#thr-smug");
  const thrBossInput = settingsMenu.querySelector<HTMLInputElement>("#thr-boss");

  const setupSecretDetection = (input: HTMLInputElement) => {
    input.addEventListener("input", () => {
      // Filter: only allow digits and m/e/o/w (for "meow" pattern)
      let filtered = input.value.replace(/[^0-9meow]/g, "");
      input.value = filtered;

      if (input.value.includes("meow")) {
        setDevMode(!devMode); // toggle on/off
        if (devMode) {
          requestAnimationFrame(() => { settingsMenu.scrollTop = 0; });
        }
        // Restore inputs to saved/default values (clear password residue)
        syncThresholdInputs();
      }
    });
  };

  if (thrSmugInput) setupSecretDetection(thrSmugInput);
  if (thrBossInput) setupSecretDetection(thrBossInput);

  // ── Hibernation ─────────────────────────────────────────────────────────
  const HIBERNATE_SECS = 4 * 60;
  let hibernating = false;

  const setHibernating = async (on: boolean) => {
    if (on === hibernating) return;
    hibernating = on;
    root.classList.toggle("hibernating", on);
    if (on) {
      // Snapshot the user's chosen position so we can re-assert it on wake.
      await savePosition();
      // Use native level change directly — bypasses Tauri's alwaysOnTop state
      // machine which can silently re-assert level 3 after sendToBack sets -1.
      await sendToBack();   // level -1 → behind every normal window
    } else {
      await sendToFront();  // level 3  → above every normal window
      // Position restore is handled by wakeUp() which is the only caller.
    }
  };

  // ── Hibernate after 2 min of no widget interaction ──────────────────────
  // Uses a one-shot setTimeout that resets on every click, which is more
  // reliable than setInterval polling (avoids WebKit timer throttling issues).
  let hibernateTimer: number | null = null;

  const scheduleHibernate = () => {
    if (hibernateTimer !== null) window.clearTimeout(hibernateTimer);
    hibernateTimer = window.setTimeout(() => {
      hibernateTimer = null;
      void setHibernating(true);
    }, HIBERNATE_SECS * 1000);
  };

  // Start the 2-min countdown immediately on widget load.
  scheduleHibernate();

  const wakeUp = async () => {
    scheduleHibernate(); // restart 2-min countdown
    if (hibernating) {
      await setHibernating(false); // removes CSS fade + calls sendToFront
    } else {
      await sendToFront(); // not hibernating but might be behind other windows
    }
    // ALWAYS restore the saved position after waking, regardless of source
    // (timer wake, tray click, Dock click, widget click). macOS may have
    // shifted the window during the level change or Space switching.
    await restorePosition();
  };

  // Wake and reset timer ONLY on a direct click on the widget.
  widget.addEventListener("click", () => {
    scheduleHibernate();
    void wakeUp();
  });

  // ── Cleanup ─────────────────────────────────────────────────────────────
  const observer = new MutationObserver(() => {
    if (!root.contains(widget)) {
      unsubs.forEach((u) => u());
      if (updatedTimer) window.clearInterval(updatedTimer);
      if (catTimer) window.clearInterval(catTimer);
      if (hibernateTimer !== null) window.clearTimeout(hibernateTimer);
      observer.disconnect();
    }
  });
  observer.observe(root, { childList: true });
}

function formatPoints(n: number): string {
  return Number.isInteger(n) ? n.toString() : n.toFixed(1);
}

function formatRelative(iso: string): string {
  const diffMin = Math.round((Date.now() - new Date(iso).getTime()) / 60000);
  if (diffMin < 1) return "Just now";
  if (diffMin < 60) return `${diffMin}m ago`;
  const diffHr = Math.round(diffMin / 60);
  if (diffHr < 24) return `${diffHr}h ago`;
  return `${Math.round(diffHr / 24)}d ago`;
}
