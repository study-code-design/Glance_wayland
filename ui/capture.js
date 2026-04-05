const app = document.querySelector("#app");
document.body.dataset.mode = "capture";

let invoke;

const state = {
  payload: null,
  dragging: false,
  dragStart: null,
  dragCurrent: null,
  selectionCss: null,
  selectionImage: null,
  loading: false,
  resultImageBase64: "",
  error: ""
};

function escapeHtml(v) {
  return String(v).replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;").replaceAll("'", "&#39;");
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function renderFatal(msg) {
  app.innerHTML = `<div class="capture-error-screen">${escapeHtml(msg)}</div>`;
}

async function ensureTauriApi() {
  if (invoke) return;
  for (let i = 0; i < 100; i++) {
    const t = window.__TAURI__;
    if (t?.core?.invoke) {
      invoke = t.core.invoke;
      return;
    }
    await delay(20);
  }
  throw new Error("Tauri runtime unavailable");
}

function getRootRect() {
  return document.querySelector("#capture-root")?.getBoundingClientRect() || new DOMRect(0, 0, window.innerWidth, window.innerHeight);
}

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

function normalizeRect(a, b, bounds) {
  const x1 = clamp(Math.min(a.x, b.x), 0, bounds.width);
  const y1 = clamp(Math.min(a.y, b.y), 0, bounds.height);
  const x2 = clamp(Math.max(a.x, b.x), 0, bounds.width);
  const y2 = clamp(Math.max(a.y, b.y), 0, bounds.height);
  return { x: x1, y: y1, width: x2 - x1, height: y2 - y1 };
}

function cssRectToImageRect(rect) {
  const bounds = getRootRect();
  return {
    x: Math.round(rect.x * state.payload.imageWidth / bounds.width),
    y: Math.round(rect.y * state.payload.imageHeight / bounds.height),
    width: Math.round(rect.width * state.payload.imageWidth / bounds.width),
    height: Math.round(rect.height * state.payload.imageHeight / bounds.height)
  };
}

function imageRectToCssRect(rect) {
  const bounds = getRootRect();
  return {
    x: rect.x * bounds.width / state.payload.imageWidth,
    y: rect.y * bounds.height / state.payload.imageHeight,
    width: rect.width * bounds.width / state.payload.imageWidth,
    height: rect.height * bounds.height / state.payload.imageHeight
  };
}

function setError(message) {
  state.error = message ? String(message) : "";
  const errorEl = document.querySelector("#capture-error");
  if (!errorEl) return;
  errorEl.hidden = !state.error;
  errorEl.textContent = state.error;
}

function updateSelectionLayer() {
  const selectionEl = document.querySelector("#capture-selection");
  const resultEl = document.querySelector("#capture-selection-result");
  const spinnerEl = document.querySelector("#capture-spinner");
  const hintEl = document.querySelector("#capture-hint");
  const dimEl = document.querySelector("#capture-dim");

  if (!selectionEl || !resultEl || !spinnerEl || !hintEl || !dimEl) return;

  if (!state.selectionCss) {
    selectionEl.hidden = true;
    dimEl.hidden = false;
    hintEl.textContent = "拖拽选择区域 · Esc/右键取消";
    return;
  }

  selectionEl.hidden = false;
  dimEl.hidden = true;
  selectionEl.style.left = `${state.selectionCss.x}px`;
  selectionEl.style.top = `${state.selectionCss.y}px`;
  selectionEl.style.width = `${state.selectionCss.width}px`;
  selectionEl.style.height = `${state.selectionCss.height}px`;

  spinnerEl.hidden = !state.loading;
  if (state.resultImageBase64) {
    resultEl.src = `data:image/jpeg;base64,${state.resultImageBase64}`;
    resultEl.hidden = false;
    hintEl.textContent = "Esc/右键关闭截图模式，或重新拖拽选择新区域";
  } else {
    resultEl.hidden = true;
    hintEl.textContent = state.loading ? "正在翻译…" : "拖拽选择区域 · Esc/右键取消";
  }
}

async function cancelCapture() {
  try {
    await invoke("cancel_capture");
  } catch (err) {
    renderFatal(err);
  }
}

function pointFromEvent(event) {
  const bounds = getRootRect();
  return {
    x: event.clientX - bounds.left,
    y: event.clientY - bounds.top
  };
}

async function submitSelection(rectCss) {
  const rect = cssRectToImageRect(rectCss);
  if (rect.width <= 4 || rect.height <= 4) {
    state.selectionCss = null;
    state.selectionImage = null;
    updateSelectionLayer();
    return;
  }

  state.selectionCss = rectCss;
  state.selectionImage = rect;
  state.loading = true;
  state.resultImageBase64 = "";
  setError("");
  updateSelectionLayer();

  try {
    const result = await invoke("submit_capture_selection", { selection: rect });
    state.selectionImage = result.selection;
    state.selectionCss = imageRectToCssRect(result.selection);
    state.resultImageBase64 = result.imageBase64;
  } catch (err) {
    setError(err);
  } finally {
    state.loading = false;
    updateSelectionLayer();
  }
}

function bindCaptureEvents() {
  const root = document.querySelector("#capture-root");
  if (!root) return;

  root.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    cancelCapture();
  });

  root.addEventListener("mousedown", (event) => {
    if (event.button !== 0 || state.loading) return;
    state.dragging = true;
    state.dragStart = pointFromEvent(event);
    state.dragCurrent = state.dragStart;
    state.selectionCss = null;
    state.selectionImage = null;
    state.resultImageBase64 = "";
    setError("");
    updateSelectionLayer();
  });

  root.addEventListener("mousemove", (event) => {
    if (!state.dragging) return;
    state.dragCurrent = pointFromEvent(event);
    state.selectionCss = normalizeRect(state.dragStart, state.dragCurrent, getRootRect());
    updateSelectionLayer();
  });

  root.addEventListener("mouseup", async (event) => {
    if (event.button === 2) {
      await cancelCapture();
      return;
    }
    if (event.button !== 0 || !state.dragging) return;
    state.dragging = false;
    state.dragCurrent = pointFromEvent(event);
    const rectCss = normalizeRect(state.dragStart, state.dragCurrent, getRootRect());
    await submitSelection(rectCss);
  });

  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      event.preventDefault();
      cancelCapture();
    }
  });

  window.addEventListener("resize", () => {
    if (state.selectionImage) {
      state.selectionCss = imageRectToCssRect(state.selectionImage);
      updateSelectionLayer();
    }
  });
}

function renderCapture() {
  app.innerHTML = `
    <div class="capture-root" id="capture-root">
      <div class="capture-dim" id="capture-dim"></div>
      <div class="capture-selection" id="capture-selection" hidden>
        <img class="capture-selection-result" id="capture-selection-result" alt="translated result" hidden />
        <div class="capture-spinner" id="capture-spinner" hidden></div>
      </div>
      <div class="capture-hint" id="capture-hint">拖拽选择区域 · Esc/右键取消</div>
      <div class="capture-error" id="capture-error" hidden></div>
    </div>`;

  bindCaptureEvents();
  updateSelectionLayer();
}

async function boot() {
  await ensureTauriApi();
  state.payload = await invoke("load_capture_payload");
  renderCapture();
}

window.addEventListener("error", (event) => renderFatal(event.error?.message || event.message || "unknown"));
window.addEventListener("unhandledrejection", (event) => renderFatal(event.reason instanceof Error ? event.reason.message : String(event.reason)));
boot().catch((err) => renderFatal(err instanceof Error ? err.message : String(err)));
