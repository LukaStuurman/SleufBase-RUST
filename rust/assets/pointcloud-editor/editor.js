import * as THREE from "./vendor/libs/three.js/build/three.module.js";

// The original Potree renderer and measurement objects are used throughout.
// This file owns only editor interaction, native-world XYZ data and persistence.
const $id = (id) => document.getElementById(id);
const number = new Intl.NumberFormat("nl-NL", { maximumFractionDigits: 3 });
const integer = new Intl.NumberFormat("nl-NL", { maximumFractionDigits: 0 });
const clone = (value) => JSON.parse(JSON.stringify(value));
const HISTORY_LIMIT = 80;
const DEFAULT_COLOR = 0x6ed9ce;
const SELECTED_COLOR = 0xff8a37;
const state = {
  source: null, viewer: null, cloud: null, ready: false,
  items: new Map(), measures: new Map(), selected: null,
  drawing: null, restoring: false, dragBefore: null, fieldBefore: null,
  undo: [], redo: [], camera: "perspective", revision: 0,
  serial: 0, savedSerial: 0, pendingSave: null, uncertainSave: null,
  savePromise: null, saveTimer: null, saveError: null,
  closeInProgress: false, allowClose: false, loadFailed: false,
};

function snapshot() { return clone([...state.items.values()]); }
function equal(a, b) { return JSON.stringify(a) === JSON.stringify(b); }
function dirty() {
  return state.serial !== state.savedSerial || !!state.pendingSave || !!state.uncertainSave
    || !!state.dragBefore || !!(state.drawing && state.drawing.confirmed > 0);
}
function pointObject(vector) { return { x: vector.x, y: vector.y, z: vector.z }; }
function finitePoint(point) { return point && [point.x, point.y, point.z].every(Number.isFinite); }
function measurementVertices(measure) { return measure.points.map((point) => pointObject(point.position)); }
function createId() {
  if (crypto.randomUUID) return crypto.randomUUID();
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}
function defaultName(kind) {
  const base = kind === "point" ? "Punt" : "Lijn";
  let index = 1;
  const names = new Set([...state.items.values()].map((item) => item.name));
  while (names.has(`${base} ${index}`)) index += 1;
  return `${base} ${index}`;
}
function setError(message, saveRelated = false) {
  $id("error-message").textContent = String(message);
  $id("error-banner").hidden = false;
  $id("retry-save-button").hidden = !saveRelated;
}
function clearError() { $id("error-banner").hidden = true; }
window.sleufbaseReportError = (message) => setError(message);

function updateSaveStatus() {
  const status = $id("save-status");
  status.classList.toggle("failed", !!state.saveError);
  status.classList.toggle("pending", !state.saveError && (dirty() || !!state.savePromise));
  if (state.saveError) status.textContent = "Nog niet opgeslagen — je wijzigingen blijven in dit venster";
  else if (state.closeInProgress) status.textContent = "Wijzigingen bewaren en venster sluiten…";
  else if (state.savePromise) status.textContent = "Wijzigingen opslaan in SleufBase…";
  else if (state.drawing && state.drawing.confirmed > 0) status.textContent = "Tekening bezig — rond af om te bewaren";
  else if (dirty()) status.textContent = "Wijzigingen worden opgeslagen…";
  else if (state.ready) status.textContent = "Opgeslagen in SleufBase";
  else status.textContent = state.loadFailed ? "Puntenwolk kon niet worden geladen" : "Puntenwolk laden…";
}

function queueSave(delay = 250) {
  if (!state.source || state.restoring) return;
  state.serial += 1;
  state.pendingSave = { serial: state.serial, annotations: snapshot() };
  clearTimeout(state.saveTimer);
  // A failed request may already have committed server-side. Keep it intact
  // until an idempotent retry recovers its ACK, then send the latest snapshot.
  if (!state.saveError) state.saveTimer = setTimeout(() => drainSaves().catch(() => {}), delay);
  updateSaveStatus();
}

async function postSnapshot(attempt) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 20000);
  try {
    const response = await fetch("./api/annotations", {
      method: "POST", headers: { "Content-Type": "application/json" },
      body: attempt.body, signal: controller.signal, cache: "no-store",
    });
    let result;
    try { result = await response.json(); } catch (_) { throw new Error("SleufBase gaf geen geldig antwoord bij het opslaan."); }
    if (!response.ok || result.ok !== true) {
      const error = new Error(response.status === 409
        ? "Er is intussen een andere wijziging opgeslagen. Je eigen wijzigingen blijven in dit venster bewaard; het venster is niet gesloten."
        : result.error || "Opslaan in SleufBase is niet gelukt.");
      error.httpStatus = response.status;
      throw error;
    }
    if (!Number.isSafeInteger(result.revision) || result.revision < state.revision) {
      throw new Error("SleufBase bevestigde geen geldige opslagversie. Je wijzigingen blijven hier bewaard.");
    }
    return result;
  } catch (error) {
    if (error.name === "AbortError") throw new Error("SleufBase heeft het opslaan nog niet bevestigd. Probeer opnieuw; je wijzigingen blijven hier bewaard.");
    throw error;
  } finally { clearTimeout(timer); }
}

async function drainSaves() {
  clearTimeout(state.saveTimer);
  if (state.savePromise) {
    await state.savePromise;
    if (state.pendingSave || state.uncertainSave) return drainSaves();
    return;
  }
  if (!state.pendingSave && !state.uncertainSave) return;
  state.savePromise = (async () => {
    while (state.uncertainSave || state.pendingSave) {
      if (!state.uncertainSave) {
        const next = state.pendingSave;
        state.pendingSave = null;
        state.uncertainSave = {
          serial: next.serial,
          body: JSON.stringify({ source_id: state.source.id, base_revision: state.revision, annotations: next.annotations }),
        };
      }
      const attempt = state.uncertainSave;
      const result = await postSnapshot(attempt);
      state.revision = result.revision;
      state.savedSerial = attempt.serial;
      state.uncertainSave = null;
      state.saveError = null;
    }
  })();
  updateSaveStatus();
  try {
    await state.savePromise;
    if (!$id("retry-save-button").hidden) clearError();
  } catch (error) {
    // Validation/size rejections are known not to have committed. Retrying a
    // corrected newer snapshot is then safe; transport errors stay uncertain.
    if ([400, 413, 422].includes(error.httpStatus) && state.uncertainSave) {
      if (!state.pendingSave) state.pendingSave = {
        serial: state.uncertainSave.serial,
        annotations: JSON.parse(state.uncertainSave.body).annotations,
      };
      state.uncertainSave = null;
    }
    state.saveError = error;
    setError(error.message || "Opslaan is niet gelukt. Je wijzigingen blijven in dit venster bewaard.", true);
    throw error;
  } finally {
    state.savePromise = null;
    updateSaveStatus();
  }
}

function remember(before) {
  if (state.restoring || equal(before, snapshot())) return;
  state.undo.push(before);
  if (state.undo.length > HISTORY_LIMIT) state.undo.shift();
  state.redo = [];
  updateButtons();
}
function finishFieldEdit() {
  if (state.fieldBefore) {
    const before = state.fieldBefore;
    state.fieldBefore = null;
    remember(before);
  }
}
function blurField() {
  if ($id("properties-fields").contains(document.activeElement)) document.activeElement.blur();
  finishFieldEdit();
}
function finishDrag() {
  if (!state.dragBefore) return;
  const before = state.dragBefore;
  state.dragBefore = null;
  remember(before);
  queueSave();
}

function disposeMeasure(measure) {
  if (!measure) return;
  state.viewer.scene.removeMeasurement(measure);
  const disposed = new Set();
  measure.traverse((object) => {
    if (object.geometry && !disposed.has(object.geometry)) {
      disposed.add(object.geometry); object.geometry.dispose();
    }
    for (const material of (Array.isArray(object.material) ? object.material : [object.material])) {
      if (!material || disposed.has(material)) continue;
      disposed.add(material);
      if (material.map && !disposed.has(material.map)) { disposed.add(material.map); material.map.dispose(); }
      material.dispose();
    }
  });
}

function bindMeasure(measure, id) {
  measure.userData.sleufbaseId = id;
  const bindSphere = (sphere) => {
    sphere.userData.sleufbaseId = id;
    sphere.addEventListener("click", () => { if (!state.drawing) selectItem(id); });
    sphere.addEventListener("select", () => { if (!state.drawing) selectItem(id); });
  };
  for (const sphere of measure.spheres) bindSphere(sphere);
  measure.addEventListener("marker_added", (event) => bindSphere(event.sphere));
  measure.addEventListener("marker_moved", () => {
    if (state.restoring || (state.drawing && state.drawing.measure === measure)) return;
    const item = state.items.get(id);
    if (!item) return;
    const vertices = measurementVertices(measure);
    if (!vertices.every(finitePoint) || equal(item.vertices, vertices)) return;
    if (!state.dragBefore) state.dragBefore = snapshot();
    item.vertices = vertices;
    if (state.selected !== id) selectItem(id);
    renderCoordinates(item);
    showCoordinate(vertices[vertices.length - 1]);
    queueSave(350);
  });
  measure.addEventListener("marker_dropped", () => {
    if (state.drawing && state.drawing.measure === measure) return;
    finishDrag();
  });
}

function createMeasure(item) {
  const measure = new Potree.Measure();
  measure.name = item.name;
  measure.closed = false;
  measure.showDistances = false;
  measure.showCoordinates = false;
  measure.showEdges = item.kind === "polyline";
  measure.maxMarkers = item.kind === "point" ? 1 : Infinity;
  measure.color.setHex(item.id === state.selected ? SELECTED_COLOR : DEFAULT_COLOR);
  for (const point of item.vertices) measure.addMarker(new THREE.Vector3(point.x, point.y, point.z));
  state.viewer.scene.addMeasurement(measure);
  bindMeasure(measure, item.id);
  state.measures.set(item.id, measure);
  return measure;
}

function restoreSnapshot(records) {
  state.restoring = true;
  try {
    state.viewer.inputHandler.deselectAll();
    state.viewer.inputHandler.drag = null;
    for (const measure of state.measures.values()) disposeMeasure(measure);
    state.measures.clear();
    state.items = new Map(records.map((item) => [item.id, clone(item)]));
    if (!state.items.has(state.selected)) state.selected = null;
    for (const item of state.items.values()) createMeasure(item);
  } finally { state.restoring = false; }
  renderList(); renderInspector(); updateButtons();
}

function undo() {
  blurField();
  if (state.drawing) { cancelDrawing(); return; }
  finishDrag();
  if (!state.undo.length) return;
  state.redo.push(snapshot());
  restoreSnapshot(state.undo.pop());
  queueSave(0);
}
function redo() {
  blurField();
  if (!finishDrawing()) return;
  finishDrag();
  if (!state.redo.length) return;
  state.undo.push(snapshot());
  restoreSnapshot(state.redo.pop());
  queueSave(0);
}

function updateButtons() {
  const ready = state.ready && !state.closeInProgress;
  $id("potree_render_area").style.pointerEvents = state.closeInProgress ? "none" : "";
  for (const id of ["navigate-button", "point-button", "line-button", "fit-button", "top-button", "perspective-button", "color-mode", "point-size", "object-filter"]) {
    $id(id).disabled = !ready;
  }
  $id("save-button").disabled = !state.source || state.closeInProgress;
  $id("undo-button").disabled = !ready || (!state.undo.length && !state.drawing);
  $id("redo-button").disabled = !ready || !state.redo.length || !!state.drawing;
  $id("delete-button").disabled = !ready || !state.selected || !!state.drawing;
  $id("focus-object-button").disabled = !ready || !state.selected;
  $id("properties-fields").disabled = !ready || !state.selected || !!state.drawing;
  for (const [id, kind] of [["navigate-button", null], ["point-button", "point"], ["line-button", "polyline"]]) {
    const active = (state.drawing?.kind || null) === kind;
    $id(id).classList.toggle("active", active);
    $id(id).setAttribute("aria-pressed", String(active));
  }
  $id("drawing-hint").hidden = !state.drawing;
  if (state.drawing) {
    const count = state.drawing.confirmed;
    $id("drawing-message").textContent = state.drawing.kind === "point"
      ? "Klik op een punt in de scan om het vast te leggen."
      : `${count} ${count === 1 ? "vertex" : "vertices"} · Klik om verder te tekenen. Enter of rechtsklik rondt de lijn af.`;
    $id("finish-button").hidden = state.drawing.kind === "point";
    $id("finish-button").disabled = count < 2;
  }
  updateSaveStatus();
}

function renderList() {
  const filter = $id("object-filter").value.toLocaleLowerCase("nl-NL");
  const list = $id("object-list");
  const scroll = list.scrollTop;
  list.replaceChildren();
  let visible = 0;
  for (const item of state.items.values()) {
    if (!item.name.toLocaleLowerCase("nl-NL").includes(filter)) continue;
    visible += 1;
    const row = document.createElement("button");
    row.type = "button";
    row.className = `object-row${state.selected === item.id ? " selected" : ""}`;
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", String(state.selected === item.id));
    row.title = item.name;
    // Static icon markup only; all file/user-derived strings use textContent.
    row.innerHTML = `<svg aria-hidden="true"><use href="#${item.kind === "point" ? "icon-point" : "icon-line"}"/></svg>`;
    const labels = document.createElement("span"); labels.className = "row-text";
    const name = document.createElement("span"); name.className = "row-name"; name.textContent = item.name;
    const meta = document.createElement("span"); meta.className = "row-meta";
    meta.textContent = item.kind === "point" ? "Punt · XYZ" : `Polylijn · ${integer.format(item.vertices.length)} vertices`;
    labels.append(name, meta); row.append(labels);
    row.addEventListener("click", () => { if (finishDrawing()) selectItem(item.id); });
    row.addEventListener("dblclick", () => { if (finishDrawing()) focusItem(item.id); });
    list.append(row);
  }
  list.scrollTop = scroll;
  $id("object-count").textContent = integer.format(state.items.size);
  $id("empty-list").hidden = visible > 0;
  $id("empty-list").textContent = state.items.size && filter
    ? "Geen objecten met deze naam."
    : "Plaats een punt of teken een lijn op de puntenwolk. Je objecten verschijnen hier.";
}

function renderCoordinates(item) {
  const rows = $id("coordinate-rows");
  rows.replaceChildren();
  // Large polylines remain fully stored; keep the properties panel bounded.
  for (const [index, point] of item.vertices.slice(0, 200).entries()) {
    const row = document.createElement("tr");
    for (const value of [String(index + 1), number.format(point.x), number.format(point.y), number.format(point.z)]) {
      const cell = document.createElement("td"); cell.textContent = value; row.append(cell);
    }
    rows.append(row);
  }
  let length = 0;
  for (let index = 1; index < item.vertices.length; index += 1) {
    const a = item.vertices[index - 1], b = item.vertices[index];
    length += Math.hypot(b.x - a.x, b.y - a.y, b.z - a.z);
  }
  const unit = state.source.crs === "rd_new" ? "m" : "broneenheden";
  $id("object-summary").textContent = item.kind === "point" ? "Punt met X-, Y- en Z-coördinaat"
    : `${integer.format(item.vertices.length)} vertices · 3D-lengte ${number.format(length)} ${unit}${item.vertices.length > 200 ? " · eerste 200 coördinaten getoond" : ""}`;
}

function renderInspector() {
  const item = state.items.get(state.selected);
  $id("no-selection").hidden = !!item;
  $id("properties-fields").hidden = !item;
  if (item) {
    $id("object-name").value = item.name;
    for (let index = 0; index < 3; index += 1) $id(`attribute-${index}`).value = item.attributes[index] || "";
    renderCoordinates(item);
    showCoordinate(item.vertices[0]);
  } else $id("coordinate-readout").hidden = true;
  updateButtons();
}

function selectItem(id) {
  if (state.selected !== id) blurField();
  state.selected = state.items.has(id) ? id : null;
  for (const [key, measure] of state.measures) measure.color.setHex(key === state.selected ? SELECTED_COLOR : DEFAULT_COLOR);
  renderList(); renderInspector();
}
function deleteSelected() {
  blurField(); finishDrag();
  if (state.drawing || !state.selected) return;
  const before = snapshot();
  disposeMeasure(state.measures.get(state.selected));
  state.measures.delete(state.selected); state.items.delete(state.selected);
  state.selected = null;
  remember(before); renderList(); renderInspector(); queueSave(0);
}

function showCoordinate(point) {
  if (!finitePoint(point)) return;
  $id("coordinate-readout").textContent = `X ${number.format(point.x)}   Y ${number.format(point.y)}   Z ${number.format(point.z)}`;
  $id("coordinate-readout").hidden = false;
}

function startDrawing(kind) {
  if (!state.ready) return;
  blurField(); finishDrag();
  if (!finishDrawing()) return;
  clearError();
  const before = snapshot();
  const name = defaultName(kind);
  const measure = state.viewer.measuringTool.startInsertion({
    name, maxMarkers: kind === "point" ? 1 : Infinity,
    showCoordinates: false, showDistances: false, showArea: false,
    showEdges: kind === "polyline", closed: false,
  });
  measure.color.setHex(SELECTED_COLOR);
  state.drawing = { kind, measure, confirmed: 0, before, id: createId(), name };
  state.viewer.renderer.domElement.focus({ preventScroll: true });
  updateButtons();
}

function stopInsertion() {
  // Potree removes the trailing preview marker for an unlimited polyline.
  state.viewer.dispatchEvent({ type: "cancel_insertions" });
  state.viewer.inputHandler.drag = null;
}

function finishDrawing() {
  const drawing = state.drawing;
  if (!drawing) return true;
  if (drawing.confirmed === 0) { cancelDrawing(); return true; }
  if (drawing.kind === "polyline" && drawing.confirmed < 2) {
    setError("Deze lijn heeft nog maar één punt. Klik een tweede punt, of kies Annuleren als je de lijn niet wilt bewaren.");
    return false;
  }
  const vertices = measurementVertices(drawing.measure).slice(0, drawing.confirmed);
  if (!vertices.every(finitePoint)) { setError("Er staat een ongeldige coördinaat in deze tekening. Annuleer en plaats de punten opnieuw."); return false; }
  state.drawing = null;
  stopInsertion();
  while (drawing.measure.points.length > vertices.length) drawing.measure.removeMarker(drawing.measure.points.length - 1);
  const item = { id: drawing.id, name: drawing.name, kind: drawing.kind, vertices, attributes: ["", "", ""] };
  state.items.set(item.id, item); state.measures.set(item.id, drawing.measure);
  bindMeasure(drawing.measure, item.id);
  remember(drawing.before); selectItem(item.id); queueSave(0); updateButtons();
  return true;
}

function cancelDrawing() {
  const drawing = state.drawing;
  if (!drawing) return;
  state.drawing = null;
  stopInsertion(); disposeMeasure(drawing.measure);
  updateButtons(); renderInspector();
}

function installDrawingCapture() {
  const canvas = state.viewer.renderer.domElement;
  canvas.addEventListener("contextmenu", (event) => event.preventDefault());
  canvas.addEventListener("mouseup", (event) => {
    if (!state.drawing) return;
    if (event.button === 2) {
      event.preventDefault(); event.stopImmediatePropagation(); finishDrawing(); return;
    }
    if (event.button !== 0) return;
    const drawing = state.drawing;
    const bounds = canvas.getBoundingClientRect();
    const mouse = new THREE.Vector2(event.clientX - bounds.left, event.clientY - bounds.top);
    const hit = Potree.Utils.getMousePointCloudIntersection(mouse, state.viewer.scene.getActiveCamera(), state.viewer,
      state.viewer.scene.pointclouds, { pickClipped: true });
    if (!hit || !finitePoint(hit.location)) {
      event.preventDefault(); event.stopImmediatePropagation();
      setError("Klik op een zichtbaar punt in de scan. In de lege achtergrond kan geen punt worden vastgelegd.");
      return;
    }
    clearError();
    drawing.measure.setPosition(drawing.measure.points.length - 1, hit.location);
    drawing.confirmed += 1;
    showCoordinate(hit.location);
    // Potree's own mouseup/drop handlers run first; the polyline then has one
    // temporary marker beyond the confirmed vertices. Never persist that one.
    queueMicrotask(() => {
      if (state.drawing !== drawing) return;
      if (drawing.kind === "point") finishDrawing();
      else updateButtons();
    });
  }, true);
}

function setCamera(mode, bounds = null) {
  if (!state.cloud || !finishDrawing()) return;
  state.camera = mode;
  const box = bounds || state.cloud.getBoundingBoxWorld();
  const center = box.getCenter(new THREE.Vector3());
  const size = Math.max(box.getSize(new THREE.Vector3()).length(), 1);
  state.viewer.setCameraMode(mode === "top" ? Potree.CameraMode.ORTHOGRAPHIC : Potree.CameraMode.PERSPECTIVE);
  const eye = mode === "top" ? [center.x, center.y, center.z + size * 1.2]
    : [center.x + size * .7, center.y - size * .7, center.z + size * .6];
  state.viewer.scene.view.setView(eye, center.toArray(), 0);
  state.viewer.setMoveSpeed(Math.max(size / 4, .1));
  for (const [id, candidate] of [["top-button", "top"], ["perspective-button", "perspective"]]) {
    $id(id).classList.toggle("active", mode === candidate);
    $id(id).setAttribute("aria-pressed", String(mode === candidate));
  }
  $id("canvas-hint").textContent = mode === "top"
    ? "Rechts slepen: verplaatsen · Scrollen: zoomen · Klik op een bol om een object te selecteren"
    : "Slepen: draaien · Rechts slepen: verplaatsen · Scrollen: zoomen";
}
function focusItem(id) {
  const item = state.items.get(id);
  if (!item || !finishDrawing()) return;
  selectItem(id);
  const bounds = new THREE.Box3().setFromPoints(item.vertices.map((point) => new THREE.Vector3(point.x, point.y, point.z)));
  bounds.expandByScalar(Math.max(bounds.getSize(new THREE.Vector3()).length() * .15, 1));
  setCamera(state.camera, bounds);
}

async function saveNow() {
  blurField(); finishDrag();
  if (!finishDrawing()) return false;
  await drainSaves();
  return !dirty();
}

window.sleufbaseBeforeClose = async function () {
  if (state.closeInProgress) return;
  state.closeInProgress = true; updateButtons();
  try {
    if (!await saveNow()) return;
    state.allowClose = true;
    if (window.ipc && typeof window.ipc.postMessage === "function") {
      try { window.ipc.postMessage("close"); }
      catch (error) { state.allowClose = false; setError("Het venster kon niet worden gesloten. Je wijzigingen zijn wel opgeslagen; probeer opnieuw."); }
    } else { state.allowClose = false; setError("De verbinding met het SleufBase-venster ontbreekt. Je wijzigingen zijn wel opgeslagen."); }
  } catch (_) {
    // drainSaves already presents the specific error and retains the payload.
  } finally {
    // IPC is acknowledged by the native loop asynchronously. Keep interaction
    // frozen in that interval so a final click cannot create an unsaved edit.
    if (!state.allowClose) { state.closeInProgress = false; updateButtons(); }
  }
};
window.addEventListener("beforeunload", (event) => {
  if (!state.allowClose && dirty()) { event.preventDefault(); event.returnValue = ""; }
});

function installUI() {
  $id("point-button").addEventListener("click", () => startDrawing("point"));
  $id("line-button").addEventListener("click", () => startDrawing("polyline"));
  $id("navigate-button").addEventListener("click", () => finishDrawing());
  $id("finish-button").addEventListener("click", () => finishDrawing());
  $id("cancel-button").addEventListener("click", () => { cancelDrawing(); clearError(); });
  $id("undo-button").addEventListener("click", undo);
  $id("redo-button").addEventListener("click", redo);
  $id("delete-button").addEventListener("click", deleteSelected);
  $id("object-filter").addEventListener("input", renderList);
  $id("top-button").addEventListener("click", () => setCamera("top"));
  $id("perspective-button").addEventListener("click", () => setCamera("perspective"));
  $id("fit-button").addEventListener("click", () => setCamera(state.camera));
  $id("focus-object-button").addEventListener("click", () => focusItem(state.selected));
  $id("save-button").addEventListener("click", () => saveNow().catch(() => {}));
  $id("retry-save-button").addEventListener("click", () => saveNow().catch(() => {}));
  $id("close-button").addEventListener("click", () => window.sleufbaseBeforeClose());
  $id("dismiss-error-button").addEventListener("click", clearError);
  $id("reload-button").addEventListener("click", async () => {
    try { if (await saveNow()) { state.allowClose = true; location.reload(); } } catch (_) {}
  });
  $id("help-button").addEventListener("click", () => $id("help-dialog").showModal());
  $id("close-help-button").addEventListener("click", () => $id("help-dialog").close());
  $id("point-size").addEventListener("input", (event) => {
    if (state.cloud) state.cloud.material.size = Number(event.target.value);
    $id("point-size-value").textContent = number.format(Number(event.target.value));
  });
  $id("color-mode").addEventListener("change", (event) => {
    if (state.cloud) state.cloud.material.activeAttributeName = event.target.value;
  });
  for (const field of [$id("object-name"), ...[0, 1, 2].map((index) => $id(`attribute-${index}`))]) {
    field.addEventListener("focus", () => { finishDrag(); if (!state.fieldBefore) state.fieldBefore = snapshot(); });
    field.addEventListener("input", () => {
      const item = state.items.get(state.selected);
      if (!item) return;
      if (field.id === "object-name") {
        // Temporary empty text never replaces the last valid object name.
        if (!field.value.trim()) return;
        item.name = field.value;
        state.measures.get(item.id).name = item.name;
      } else item.attributes[Number(field.id.slice(-1))] = field.value;
      renderList(); queueSave(300);
    });
    field.addEventListener("blur", () => {
      const item = state.items.get(state.selected);
      if (item && field.id === "object-name" && !field.value.trim()) field.value = item.name;
      finishFieldEdit();
    });
  }
  document.addEventListener("keydown", (event) => {
    if ($id("help-dialog").open) return;
    const editing = ["INPUT", "TEXTAREA", "SELECT"].includes(event.target.tagName) || event.target.isContentEditable;
    const modifier = event.ctrlKey || event.metaKey;
    const key = event.key.toLowerCase();
    if (modifier && key === "s") { event.preventDefault(); event.stopImmediatePropagation(); saveNow().catch(() => {}); return; }
    if (editing || !state.ready || state.closeInProgress) return;
    let handled = true;
    if (modifier && key === "z") event.shiftKey ? redo() : undo();
    else if (modifier && key === "y") redo();
    else if (event.key === "Escape") { cancelDrawing(); clearError(); }
    else if (event.key === "Enter" && state.drawing) finishDrawing();
    else if (event.key === "Delete") deleteSelected();
    else if (!modifier && key === "p") startDrawing("point");
    else if (!modifier && key === "l") startDrawing("polyline");
    else if (!modifier && key === "v") finishDrawing();
    else if (!modifier && key === "f") setCamera(state.camera);
    else handled = false;
    if (handled) { event.preventDefault(); event.stopImmediatePropagation(); }
  }, true);
}

function validateSource(source) {
  if (!source || typeof source.id !== "string" || !Array.isArray(source.annotations)) throw new Error("SleufBase gaf geen geldige brongegevens terug.");
  const ids = new Set();
  for (const item of source.annotations) {
    if (!item || typeof item.id !== "string" || ids.has(item.id) || typeof item.name !== "string"
        || !["point", "polyline"].includes(item.kind) || !Array.isArray(item.vertices)
        || !item.vertices.every(finitePoint) || !Array.isArray(item.attributes) || item.attributes.length !== 3
        || !item.attributes.every((value) => typeof value === "string")
        || (item.kind === "point" ? item.vertices.length !== 1 : item.vertices.length < 2)) {
      throw new Error("Een bewaard tekenobject is ongeldig. De bestaande gegevens zijn niet gewijzigd.");
    }
    ids.add(item.id);
  }
}

async function loadCloud() {
  const url = new URL("./cloud/metadata.json", location.href).href;
  const check = await fetch(url, { cache: "no-store" });
  if (!check.ok) throw new Error("De voorbereide puntenwolk ontbreekt. Open het bronbestand opnieuw in SleufBase.");
  const metadata = await check.json();
  if (!metadata.boundingBox || !Array.isArray(metadata.attributes)) throw new Error("De voorbereide puntenwolk heeft geen geldige Potree-metadata.");
  // Direct official loader avoids the duplicate OctreeLoader call in the
  // 1.8.2 convenience loadPointCloud() wrapper, without replacing its engine.
  const loaded = await Potree.OctreeLoader.load(url);
  const cloud = new Potree.PointCloudOctree(loaded.geometry);
  cloud.name = state.source.title;
  state.cloud = cloud;
  state.viewer.scene.addPointCloud(cloud);
  const attributes = cloud.pcoGeometry.pointAttributes.attributes;
  const color = attributes.some((attribute) => ["rgba", "rgb"].includes(attribute.name));
  const intensity = attributes.some((attribute) => attribute.name === "intensity");
  $id("color-mode").querySelector('[value="rgba"]').disabled = !color;
  $id("color-mode").querySelector('[value="intensity"]').disabled = !intensity;
  $id("color-mode").value = color ? "rgba" : "elevation";
  cloud.material.activeAttributeName = $id("color-mode").value;
  cloud.material.pointSizeType = Potree.PointSizeType.ADAPTIVE;
  cloud.material.size = 1;
  cloud.material.minSize = 1;
  const position = attributes.find((attribute) => attribute.name === "position");
  if (position && Array.isArray(position.range[0])) cloud.material.elevationRange = [position.range[0][2], position.range[1][2]];
  setCamera("perspective");
  await new Promise((resolve, reject) => {
    const start = performance.now();
    const inspect = () => {
      if (cloud.pcoGeometry.root.loaded || cloud.root?.isTreeNode?.()) return resolve();
      if (performance.now() - start > 60000) return reject(new Error("De puntenwolk kon niet volledig starten. Controleer of alle lokale viewerbestanden aanwezig zijn en open de bron opnieuw."));
      setTimeout(inspect, 100);
    };
    inspect();
  });
}

async function boot() {
  installUI();
  try {
    if (!window.Potree?.Viewer) throw new Error("De lokale Potree-viewer ontbreekt in deze installatie.");
    const response = await fetch("./api/source", { cache: "no-store" });
    if (!response.ok) throw new Error("De verbinding met de lokale SleufBase-editor is niet beschikbaar.");
    const source = await response.json();
    validateSource(source);
    state.source = source;
    state.revision = Number.isSafeInteger(source.revision) ? source.revision : 0;
    $id("source-title").textContent = source.title || "Puntenwolk";
    document.title = `${source.title || "Puntenwolk"} · SleufBase`;
    $id("source-path").textContent = source.source_path || "Lokaal bestand";
    $id("source-points").textContent = integer.format(source.point_count);
    $id("source-crs").textContent = source.crs === "rd_new" ? "RD New (EPSG:28992)"
      : source.projection || (source.crs === "unknown" ? "Niet vastgelegd in het bestand" : "Coördinatenstelsel uit het bronbestand");
    $id("coordinate-system-status").textContent = source.crs === "rd_new" ? "RD New · originele XYZ" : "Originele XYZ · broncoördinaten";
    state.viewer = new Potree.Viewer($id("potree_render_area"));
    state.viewer.setEDLEnabled(true);
    state.viewer.setFOV(60);
    state.viewer.setPointBudget(2_000_000);
    state.viewer.setBackground("gradient");
    state.viewer.setControls(state.viewer.orbitControls);
    state.viewer.measuringTool.showLabels = false;
    installDrawingCapture();
    restoreSnapshot(source.annotations);
    await loadCloud();
    state.ready = true;
    $id("loading-overlay").hidden = true;
    $id("cloud-status").textContent = `${integer.format(source.point_count)} bronpunten · dynamische weergave`;
    updateButtons();
    new ResizeObserver(() => state.viewer?.renderer?.setSize($id("viewport").clientWidth, $id("viewport").clientHeight)).observe($id("viewport"));
  } catch (error) {
    state.loadFailed = true;
    $id("loading-title").textContent = "Puntenwolk kon niet worden geopend";
    $id("loading-message").textContent = error.message || "Er ging iets mis bij het laden van de lokale puntenwolk.";
    $id("loading-spinner").hidden = true;
    $id("reload-button").hidden = false;
    updateButtons();
  }
}

boot();
