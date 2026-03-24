const LANGUAGES = [
  { value: "auto", label: "自动检测" },
  { value: "zh-CHS", label: "中文简体" },
  { value: "zh-CHT", label: "中文繁体" },
  { value: "en", label: "英语" },
  { value: "ja", label: "日语" },
  { value: "ko", label: "韩语" },
  { value: "fr", label: "法语" },
  { value: "de", label: "德语" },
  { value: "ru", label: "俄语" },
  { value: "es", label: "西班牙语" }
];

const app = document.querySelector("#app");
const mode = window.__APP_MODE__ || "main";
document.body.dataset.mode = mode.startsWith("capture") ? "capture" : mode.startsWith("overlay") ? "overlay" : "main";

let invoke;
let listen;

const state = {
  settings: null,
  history: [],
  overlay: null,
  status: "",
  statusType: "",
  loading: false,
  listenersBound: false
};

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function renderFatal(message) {
  if (!app) return;
  app.innerHTML = `
    <div class="shell app-main">
      <div class="status error">启动失败: ${escapeHtml(message)}</div>
    </div>
  `;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function ensureTauriApi() {
  if (invoke && listen) {
    return;
  }

  for (let attempt = 0; attempt < 100; attempt += 1) {
    const tauri = window.__TAURI__;
    if (tauri?.core?.invoke && tauri?.event?.listen) {
      invoke = tauri.core.invoke;
      listen = tauri.event.listen;
      return;
    }
    await delay(20);
  }

  throw new Error("Tauri runtime unavailable");
}

function languageOptions(current) {
  return LANGUAGES.map(
    (lang) => `<option value="${lang.value}" ${lang.value === current ? "selected" : ""}>${lang.label}</option>`
  ).join("");
}

function setStatus(message, type = "") {
  state.status = message;
  state.statusType = type;
  if (mode === "main") {
    renderMain();
  }
}

async function loadSettings() {
  state.settings = await invoke("load_settings");
}

async function saveSettings() {
  state.settings = await invoke("save_settings", { settings: state.settings });
}

async function loadHistory() {
  state.history = await invoke("list_history", { query: { limit: 20 } });
}

async function bindMainListeners() {
  if (state.listenersBound || mode !== "main") {
    return;
  }

  await listen("workflow:state", (event) => {
    const payload = event.payload || {};
    state.loading = Boolean(payload.busy);
    if (typeof payload.message === "string") {
      state.status = payload.message;
      state.statusType = payload.type || "";
    }
    renderMain();
  });

  state.listenersBound = true;
}

function renderMain() {
  if (!state.settings) {
    app.innerHTML = `<div class="shell app-main"><div class="status">正在加载…</div></div>`;
    return;
  }

  const statusHtml = state.status
    ? `<div class="status ${state.statusType}">${escapeHtml(state.status)}</div>`
    : `<div class="status"></div>`;

  const loadingDots = `<span class="dot-loading"><span></span><span></span><span></span></span>`;

  app.innerHTML = `
    <div class="shell app-main">
      <div class="app-header">
        <span class="app-title">Glance</span>
        ${statusHtml}
      </div>
      <hr class="divider" />
      <div class="settings-row">
        <div class="field-inline">
          <label for="from-lang">源语言</label>
          <select id="from-lang">${languageOptions(state.settings.fromLang)}</select>
        </div>
        <div class="field-inline">
          <label for="to-lang">目标</label>
          <select id="to-lang">${languageOptions(state.settings.toLang)}</select>
        </div>
      </div>
      <div class="settings-row">
        <div class="field-inline">
          <label for="hotkey">快捷键</label>
          <input id="hotkey" class="hotkey-input" type="text" value="${escapeHtml(state.settings.hotkey)}" readonly />
        </div>
        <div class="field-inline autostart-inline">
          <label for="autostart">开机自启</label>
          <button class="toggle ${state.settings.autostart ? 'on' : ''}" id="autostart" aria-pressed="${state.settings.autostart}"></button>
        </div>
      </div>
      <hr class="divider" />
      <div class="actions-row">
        <button class="button primary" id="start-capture" ${state.loading ? "disabled" : ""}>${state.loading ? loadingDots : "截图翻译"}</button>
      </div>
    </div>
  `;

  document.querySelector("#from-lang").addEventListener("change", (e) => { state.settings.fromLang = e.target.value; saveSettings().catch(() => {}); });
  document.querySelector("#to-lang").addEventListener("change", (e) => { state.settings.toLang = e.target.value; saveSettings().catch(() => {}); });
  setupHotkeyRecorder(document.querySelector("#hotkey"));
  document.querySelector("#autostart").addEventListener("click", (e) => {
    state.settings.autostart = !state.settings.autostart;
    e.currentTarget.classList.toggle("on", state.settings.autostart);
    e.currentTarget.setAttribute("aria-pressed", state.settings.autostart);
    saveSettings().catch((err) => setStatus(String(err), "error"));
  });
  document.querySelector("#start-capture").addEventListener("click", startCaptureFlow);
}

async function startCaptureFlow() {
  try {
    state.loading = true;
    state.status = "正在启动系统截图…";
    state.statusType = "";
    renderMain();
    await saveSettings();
    await invoke("begin_capture", {
      options: {
        fromLang: state.settings.fromLang,
        toLang: state.settings.toLang
      }
    });
  } catch (error) {
    state.loading = false;
    setStatus(String(error), "error");
  }
}

async function renderCapture() {
  // capture.html is now a standalone page; this code path should not be reached.
  app.innerHTML = `<div class="shell app-main"><div class="status">请使用主窗口的截图按钮。</div></div>`;
}

function renderOverlayRegions() {
  const stage = document.querySelector("#overlay-stage");
  if (!stage || !state.overlay) return;

  const left = state.overlay.selection.x;
  const top = state.overlay.selection.y;
  const width = state.overlay.selection.width;
  const height = state.overlay.selection.height;
  const imageSrc = `data:image/jpeg;base64,${state.overlay.renderedImageBase64}`;

  stage.innerHTML = `
    <img
      class="overlay-image"
      src="${imageSrc}"
      alt="translated selection"
      style="
        left:${left}px;
        top:${top}px;
        width:${width}px;
        height:${height}px;
        opacity:${state.overlay.overlayOpacity};
      "
    />
  `;
}

async function renderOverlay() {
  app.innerHTML = `
    <div class="overlay-root">
      <div id="overlay-stage"></div>
    </div>
  `;

  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      invoke("close_overlay");
    }
  });

  try {
    state.overlay = await invoke("load_overlay_payload");
    if (state.overlay.closeOnOutsideClick) {
      document.querySelector(".overlay-root").addEventListener("click", () => invoke("close_overlay"));
    }
    renderOverlayRegions();
  } catch (error) {
    renderFatal(`覆盖层初始化失败: ${error}`);
  }
}

// Key name map: JS KeyboardEvent.key → Tauri accelerator token
const KEY_MAP = {
  " ": "Space", "ArrowUp": "Up", "ArrowDown": "Down", "ArrowLeft": "Left", "ArrowRight": "Right",
  "Escape": "Escape", "Enter": "Return", "Tab": "Tab", "Backspace": "Backspace",
  "Delete": "Delete", "Insert": "Insert", "Home": "Home", "End": "End",
  "PageUp": "PageUp", "PageDown": "PageDown",
  "F1":"F1","F2":"F2","F3":"F3","F4":"F4","F5":"F5","F6":"F6",
  "F7":"F7","F8":"F8","F9":"F9","F10":"F10","F11":"F11","F12":"F12",
};

function setupHotkeyRecorder(input) {
  if (!input) return;

  input.readOnly = true;

  input.addEventListener("focus", () => {
    input.classList.add("recording");
    input.dataset.prev = input.value;
    input.value = "按下快捷键…";
  });

  input.addEventListener("blur", () => {
    input.classList.remove("recording");
    // Restore previous value if nothing was recorded during this session.
    if (input.value === "按下快捷键…") {
      input.value = input.dataset.prev || "";
    }
  });

  input.addEventListener("keydown", (e) => {
    e.preventDefault();
    e.stopPropagation();

    const mods = [];
    if (e.ctrlKey)  mods.push("Ctrl");
    if (e.altKey)   mods.push("Alt");
    if (e.shiftKey) mods.push("Shift");
    if (e.metaKey)  mods.push("Super");

    // Ignore lone modifier keys
    if (["Control","Alt","Shift","Meta"].includes(e.key)) return;

    // Map key to Tauri token
    let key = KEY_MAP[e.key] || (e.key.length === 1 ? e.key.toUpperCase() : null);
    if (!key) return;

    // Must have at least one modifier
    if (mods.length === 0) return;

    const combo = [...mods, key].join("+");
    input.value = combo;
    input.classList.remove("recording");
    state.settings.hotkey = combo;
    input.blur();
    saveSettings().catch((err) => setStatus(String(err), "error"));
  });
}

async function boot() {
  await ensureTauriApi();
  await bindMainListeners();

  if (mode.startsWith("capture")) {
    await renderCapture();
    return;
  }

  if (mode.startsWith("overlay")) {
    await renderOverlay();
    return;
  }

  await loadSettings();
  renderMain();
}

window.addEventListener("error", (event) => {
  renderFatal(event.error?.message || event.message || "unknown error");
});

window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason instanceof Error ? event.reason.message : String(event.reason);
  renderFatal(reason);
});

boot().catch((error) => {
  renderFatal(error instanceof Error ? error.message : String(error));
});
