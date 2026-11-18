import * as THREE from "./vendor/libs/three.js/build/three.module.js";

// The original Potree renderer and measurement objects are used throughout.
// This file owns only editor interaction, native-world XYZ data and persistence.
const $id = (id) => document.getElementById(id);
const number = new Intl.NumberFormat("nl-NL", { maximumFractionDigits: 3 });
const integer = new Intl.NumberFormat("nl-NL", { maximumFractionDigits: 0 });
const clone = (value) => JSON.parse(JSON.stringify(value));
const HISTORY_LIMIT = 80;
const DEFAULT_COLOR = 0x2563eb;
const SELECTED_COLOR = 0xf97316;
const ENDPOINTS = [
  { purpose: "trench_start", prefix: "trench-start", name: "Beginpunt", color: 0x2563eb },
  { purpose: "trench_end", prefix: "trench-end", name: "Eindpunt", color: 0x16a34a },
];
const state = {
  source: null, viewer: null, cloud: null, ready: false,
  catalog: { types: [], materials: [] }, workflow: "trench",
  creation: { object_type: "", type_key: "", material: "" },
  endpoints: {}, axis: null,
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
function defaultName(kind, objectType = "") {
  let base = "", bytes = 0;
  // The editable name has a smaller byte limit than a project type label.
  // Preserve the full type separately and truncate only this suggested name.
  for (const character of objectType) {
    const code = character.codePointAt(0);
    bytes += code <= 0x7f ? 1 : code <= 0x7ff ? 2 : code <= 0xffff ? 3 : 4;
    if (bytes > 480) break;
    base += character;
  }
  if (!base) base = kind === "point" ? "Punt" : "Lijn";
  let index = 1;
  const names = new Set([...state.items.values()].map((item) => item.name));
  while (names.has(`${base} ${index}`)) index += 1;
  return `${base} ${index}`;
}
function nameLooksAutomatic(item) {
  // Keep a deliberately edited name. Older, untyped records may still have
  // their original Punt/Lijn label, which should also follow a type change.
  const prefixes = [defaultName(item.kind, item.object_type), defaultName(item.kind)]
    .map((name) => name.replace(/ [1-9][0-9]*$/, ""));
  return prefixes.some((prefix) => {
    if (!item.name.startsWith(prefix + " ")) return false;
    return /^[1-9][0-9]*$/.test(item.name.slice(prefix.length + 1));
  });
}
function endpointFor(purpose) {
  for (const item of state.items.values()) if (item.purpose === purpose) return item;
  return null;
}
function sourceUnit() { return state.source?.crs === "rd_new" ? "m" : "broneenheden"; }
function itemColor(item) {
  const endpoint = ENDPOINTS.find((entry) => entry.purpose === item.purpose);
  if (endpoint) return endpoint.color;
  const type = state.catalog.types.find((entry) => entry.label === item.object_type && entry.key === item.type_key)
    || state.catalog.types.find((entry) => item.type_key && entry.key === item.type_key);
  return type ? (type.color[0] << 16) | (type.color[1] << 8) | type.color[2] : DEFAULT_COLOR;
}
function updateMeasureColors() {
  for (const [id, measure] of state.measures) {
    const item = state.items.get(id);
    if (item) measure.color.setHex(id === state.selected ? SELECTED_COLOR : itemColor(item));
  }
}
function typeValue(item) { return item.object_type || item.type_key ? JSON.stringify([item.object_type, item.type_key]) : ""; }
function readType(value) {
  const [object_type, type_key] = value ? JSON.parse(value) : ["", ""];
  return { object_type, type_key };
}
function fillTypeSelect(select, current) {
  const values = new Map([["", "Type kiezen…"]]);
  for (const type of state.catalog.types) values.set(JSON.stringify([type.label, type.key]), type.label);
  // Keep project values that are no longer present in the current catalog.
  for (const item of [...state.items.values(), current]) {
    const value = typeValue(item);
    if (value && !values.has(value)) values.set(value, `${item.object_type || item.type_key} (bewaard)`);
  }
  select.replaceChildren();
  for (const [value, label] of values) {
    const option = document.createElement("option"); option.value = value; option.textContent = label; select.append(option);
  }
  select.value = typeValue(current);
}
function fillMaterialSelect(select, current) {
  const values = new Set(state.catalog.materials);
  for (const item of state.items.values()) if (item.attributes[0]) values.add(item.attributes[0]);
  if (current) values.add(current);
  select.replaceChildren();
  const empty = document.createElement("option"); empty.value = ""; empty.textContent = "Materiaal kiezen…"; select.append(empty);
  for (const value of values) {
    if (!value) continue;
    const option = document.createElement("option"); option.value = value;
    option.textContent = state.catalog.materials.includes(value) ? value : `${value} (bewaard)`;
    select.append(option);
  }
  select.value = current;
}
function renderCreationOptions() {
  fillTypeSelect($id("new-object-type"), state.creation);
  fillMaterialSelect($id("new-material"), state.creation.material);
}
function showWorkflow(workflow) {
  state.workflow = workflow;
  for (const step of ["trench", "objects"]) {
    const active = workflow === step;
    $id(`${step}-panel`).hidden = !active;
    $id(`${step}-tab`).classList.toggle("active", active);
    $id(`${step}-tab`).setAttribute("aria-selected", String(active));
    $id(`${step}-tab`).tabIndex = active ? 0 : -1;
  }
}
function switchWorkflow(workflow) {
  if (state.closeInProgress) return;
  blurField(); finishDrag();
  if (!finishDrawing()) return;
  const selected = state.items.get(state.selected);
  if (selected && (selected.purpose === "object" ? "objects" : "trench") !== workflow) selectItem(null);
  showWorkflow(workflow);
}
function refreshTrench() {
  for (const endpoint of ENDPOINTS) {
    const item = endpointFor(endpoint.purpose);
    state.endpoints[endpoint.purpose] = item;
    const prefix = endpoint.prefix;
    $id(`${prefix}-status`).textContent = item ? "Geplaatst" : "Ontbreekt";
    $id(`${prefix}-status`).classList.toggle("complete", !!item);
    $id(`${prefix}-card`).classList.toggle("selected", !!item && state.selected === item.id);
    $id(`${prefix}-action`).textContent = `${endpoint.name} ${item ? "opnieuw plaatsen" : "plaatsen"}`;
    $id(`${prefix}-select`).disabled = !item || !state.ready || state.closeInProgress || !!state.drawing;
    $id(`${prefix}-select`).textContent = item
      ? `X ${number.format(item.vertices[0].x)} · Y ${number.format(item.vertices[0].y)} · Z ${number.format(item.vertices[0].z)}`
      : "Nog niet geplaatst";
  }
  const start = state.endpoints.trench_start?.vertices[0];
  const end = state.endpoints.trench_end?.vertices[0];
  const complete = !!(start && end);
  $id("trench-statusbar").textContent = `${start ? "✓" : "—"} Beginpunt · ${end ? "✓" : "—"} Eindpunt`;
  $id("trench-statusbar").setAttribute("aria-label", `Beginpunt ${start ? "geplaatst" : "ontbreekt"}, eindpunt ${end ? "geplaatst" : "ontbreekt"}`);
  $id("trench-axis-legend").hidden = !complete;
  if (complete) {
    const dx = end.x - start.x, dy = end.y - start.y, dz = end.z - start.z;
    const horizontal = Math.hypot(dx, dy), spatial = Math.hypot(dx, dy, dz), unit = sourceUnit();
    $id("trench-length").textContent = `${number.format(horizontal)} ${unit}`;
    $id("trench-height").textContent = `3D-afstand ${number.format(spatial)} ${unit} · hoogteverschil ${number.format(dz)} ${unit}`;
    $id("trench-axis-text").textContent = `Sleufas · ${number.format(horizontal)} ${unit} horizontaal`;
    $id("trench-status").textContent = horizontal > 0
      ? "Begin- en eindpunt staan klaar. Je kunt nu kabels en leidingen vastleggen."
      : "Beide punten hebben dezelfde X/Y-positie. Verplaats een punt om de sleufrichting vast te leggen.";
    if (state.viewer) {
      if (!state.axis) {
        const geometry = new THREE.BufferGeometry();
        geometry.setAttribute("position", new THREE.Float32BufferAttribute(new Float32Array(6), 3));
        geometry.setAttribute("lineDistance", new THREE.Float32BufferAttribute(new Float32Array(2), 1));
        state.axis = new THREE.Line(geometry, new THREE.LineDashedMaterial({
          color: SELECTED_COLOR, depthTest: false, depthWrite: false, transparent: true, opacity: .9,
        }));
        state.axis.frustumCulled = false;
        state.axis.raycast = () => {}; // A guide never intercepts annotation picks.
        // Use Potree's overlay pass. This is not a measurement or annotation,
        // and is deliberately absent from state.items and every save payload.
        state.viewer.measuringTool.scene.add(state.axis);
      }
      // Small local vertices plus a double-precision world translation retain
      // precision for large source coordinates, including Dutch RD New.
      state.axis.position.set(start.x, start.y, start.z);
      state.axis.geometry.attributes.position.array.set([0, 0, 0, dx, dy, dz]);
      state.axis.geometry.attributes.position.needsUpdate = true;
      state.axis.geometry.attributes.lineDistance.array[1] = spatial;
      state.axis.geometry.attributes.lineDistance.needsUpdate = true;
      state.axis.material.dashSize = Math.max(spatial / 35, .001);
      state.axis.material.gapSize = state.axis.material.dashSize * .65;
      state.axis.visible = spatial > 0;
    }
  } else {
    $id("trench-length").textContent = "—";
    $id("trench-height").textContent = "De lengte verschijnt na het plaatsen van beide punten.";
    $id("trench-status").textContent = start ? "Plaats nu het eindpunt op maaiveld."
      : end ? "Plaats nu het beginpunt op maaiveld." : "Beginpunt en eindpunt ontbreken.";
    if (state.axis) state.axis.visible = false;
  }
  updateEndpointLabels();
}
function updateEndpointLabels() {
  if (!state.viewer) return;
  const camera = state.viewer.scene.getActiveCamera();
  const viewport = $id("viewport");
  for (const endpoint of ENDPOINTS) {
    const label = $id(`${endpoint.prefix}-label`);
    const point = state.endpoints[endpoint.purpose]?.vertices[0];
    if (!point || !state.ready || state.drawing) { label.hidden = true; continue; }
    const screen = new THREE.Vector3(point.x, point.y, point.z).project(camera);
    const visible = Number.isFinite(screen.x) && Number.isFinite(screen.y)
      && screen.z >= -1 && screen.z <= 1 && Math.abs(screen.x) <= 1 && Math.abs(screen.y) <= 1;
    label.hidden = !visible;
    label.disabled = state.closeInProgress;
    if (visible) {
      label.style.left = `${(screen.x + 1) * viewport.clientWidth / 2}px`;
      label.style.top = `${(1 - screen.y) * viewport.clientHeight / 2}px`;
    }
  }
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
    // Measurement markers use our annotation selection. A native "select"
    // listener makes Potree pass this mesh to TransformationTool, which expects
    // a volume boundingBox and blacklists the marker from ordinary vertex drags.
    sphere.addEventListener("click", (event) => {
      if (state.drawing) return;
      event.consume?.();
      selectItem(id);
    });
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
    if (item.purpose !== "object") refreshTrench();
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
  measure.color.setHex(item.id === state.selected ? SELECTED_COLOR : itemColor(item));
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
  renderCreationOptions(); renderList(); renderInspector(); updateButtons();
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
  for (const id of ["navigate-button", "point-button", "line-button", "trench-start-button", "trench-end-button", "to-objects-button", "fit-button", "top-button", "perspective-button", "color-mode", "point-size", "object-filter"]) {
    $id(id).disabled = !ready;
  }
  $id("drawing-defaults").disabled = !ready || !!state.drawing;
  $id("save-button").disabled = !state.source || state.closeInProgress;
  $id("undo-button").disabled = !ready || (!state.undo.length && !state.drawing);
  $id("redo-button").disabled = !ready || !state.redo.length || !!state.drawing;
  $id("delete-button").disabled = !ready || !state.selected || !!state.drawing;
  $id("focus-object-button").disabled = !ready || !state.selected;
  $id("properties-fields").disabled = !ready || !state.selected || !!state.drawing;
  for (const [id, kind, purpose] of [["navigate-button", null, null], ["point-button", "point", "object"],
    ["line-button", "polyline", "object"], ["trench-start-button", "point", "trench_start"], ["trench-end-button", "point", "trench_end"]]) {
    const active = kind === null ? !state.drawing : state.drawing?.kind === kind && state.drawing.purpose === purpose;
    $id(id).classList.toggle("active", active);
    $id(id).setAttribute("aria-pressed", String(active));
  }
  $id("drawing-hint").hidden = !state.drawing;
  if (state.drawing) {
    const count = state.drawing.confirmed;
    const endpoint = ENDPOINTS.find((entry) => entry.purpose === state.drawing.purpose);
    $id("drawing-message").textContent = endpoint
      ? `${endpoint.name}: klik op het maaiveld aan dit uiteinde van de proefsleuf.`
      : state.drawing.kind === "point"
      ? "Klik op een punt in de scan om het vast te leggen."
      : `${count} ${count === 1 ? "punt" : "punten"} · Klik om verder te tekenen. Enter of rechtsklik rondt de lijn af.`;
    $id("finish-button").hidden = state.drawing.kind === "point";
    $id("finish-button").disabled = count < 2;
  }
  refreshTrench();
  updateSaveStatus();
}

function renderList() {
  const filter = $id("object-filter").value.toLocaleLowerCase("nl-NL");
  const list = $id("object-list");
  const scroll = list.scrollTop;
  list.replaceChildren();
  let visible = 0, total = 0;
  for (const item of state.items.values()) {
    if (item.purpose !== "object") continue;
    total += 1;
    if (![item.name, item.object_type, item.type_key, item.attributes[0]].join(" ").toLocaleLowerCase("nl-NL").includes(filter)) continue;
    visible += 1;
    const row = document.createElement("button");
    row.type = "button";
    row.className = `object-row${state.selected === item.id ? " selected" : ""}`;
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", String(state.selected === item.id));
    row.title = item.name;
    row.style.setProperty("--object-color", `#${itemColor(item).toString(16).padStart(6, "0")}`);
    // Static icon markup only; all file/user-derived strings use textContent.
    row.innerHTML = `<svg aria-hidden="true"><use href="#${item.kind === "point" ? "icon-point" : "icon-line"}"/></svg>`;
    const labels = document.createElement("span"); labels.className = "row-text";
    const name = document.createElement("span"); name.className = "row-name"; name.textContent = item.name;
    const meta = document.createElement("span"); meta.className = "row-meta";
    meta.textContent = [item.object_type || "Geen type", item.kind === "point" ? "Punt" : `Polylijn · ${integer.format(item.vertices.length)} punten`, item.attributes[0]].filter(Boolean).join(" · ");
    labels.append(name, meta); row.append(labels);
    row.addEventListener("click", () => { if (finishDrawing()) selectItem(item.id); });
    row.addEventListener("dblclick", () => { if (finishDrawing()) focusItem(item.id); });
    list.append(row);
  }
  list.scrollTop = scroll;
  $id("object-count").textContent = integer.format(total);
  $id("empty-list").hidden = visible > 0;
  $id("empty-list").textContent = total && filter
    ? "Geen objecten met deze naam, dit type of dit materiaal."
    : "Plaats een punt of teken een lijn. Je kabels en leidingen verschijnen hier.";
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
  const endpoint = ENDPOINTS.find((entry) => entry.purpose === item.purpose);
  $id("object-summary").textContent = endpoint ? `${endpoint.name} van de proefsleuf · maaiveld`
    : item.kind === "point" ? "Punt met X-, Y- en Z-coördinaat"
    : `${integer.format(item.vertices.length)} punten · 3D-lengte ${number.format(length)} ${sourceUnit()}${item.vertices.length > 200 ? " · eerste 200 coördinaten getoond" : ""}`;
}

function renderInspector() {
  const item = state.items.get(state.selected);
  $id("no-selection").hidden = !!item;
  $id("properties-fields").hidden = !item;
  const endpoint = item && ENDPOINTS.find((entry) => entry.purpose === item.purpose);
  $id("properties-title").textContent = endpoint ? `${endpoint.name} bewerken` : "Eigenschappen";
  if (item) {
    $id("object-name").value = item.name;
    $id("object-type-fields").hidden = !!endpoint;
    fillTypeSelect($id("object-type"), item);
    fillMaterialSelect($id("attribute-0"), item.attributes[0]);
    for (let index = 1; index < 3; index += 1) $id(`attribute-${index}`).value = item.attributes[index] || "";
    $id("geometry-hint").textContent = endpoint
      ? "Sleep de bol naar een ander maaiveldpunt, of kies opnieuw plaatsen. De sleufas en lengte volgen automatisch."
      : "Sleep een geselecteerde bol naar een ander punt op de scan. Z is de bronhoogte.";
    renderCoordinates(item);
    showCoordinate(item.vertices[0]);
  } else $id("coordinate-readout").hidden = true;
  updateButtons();
}

function selectItem(id) {
  if (state.selected !== id) blurField();
  state.selected = state.items.has(id) ? id : null;
  const item = state.items.get(state.selected);
  if (item) showWorkflow(item.purpose === "object" ? "objects" : "trench");
  updateMeasureColors();
  renderList(); renderInspector();
}
function deleteSelected() {
  blurField(); finishDrag();
  if (state.drawing || !state.selected) return;
  const before = snapshot();
  disposeMeasure(state.measures.get(state.selected));
  state.measures.delete(state.selected); state.items.delete(state.selected);
  state.selected = null;
  remember(before); renderCreationOptions(); renderList(); renderInspector(); queueSave(0);
}

function showCoordinate(point) {
  if (!finitePoint(point)) return;
  $id("coordinate-readout").textContent = `X ${number.format(point.x)}   Y ${number.format(point.y)}   Z ${number.format(point.z)}`;
  $id("coordinate-readout").hidden = false;
}

function startDrawing(kind, purpose = "object") {
  if (!state.ready || state.closeInProgress) return;
  blurField(); finishDrag();
  if (!finishDrawing()) return;
  clearError();
  const before = snapshot();
  const endpoint = ENDPOINTS.find((entry) => entry.purpose === purpose);
  const existing = endpoint ? endpointFor(purpose) : null;
  const template = existing ? clone(existing) : {
    id: createId(), name: endpoint ? endpoint.name : defaultName(kind, state.creation.object_type),
    kind, purpose, object_type: endpoint ? "" : state.creation.object_type,
    type_key: endpoint ? "" : state.creation.type_key, attributes: [endpoint ? "" : state.creation.material, "", ""],
  };
  const name = template.name;
  showWorkflow(endpoint ? "trench" : "objects");
  const measure = state.viewer.measuringTool.startInsertion({
    name, maxMarkers: kind === "point" ? 1 : Infinity,
    showCoordinates: false, showDistances: false, showArea: false,
    showEdges: kind === "polyline", closed: false,
  });
  measure.color.setHex(SELECTED_COLOR);
  state.drawing = { kind, purpose, measure, confirmed: 0, before, template };
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
  const item = { ...drawing.template, vertices };
  // Replacing an endpoint is one normal history entry and retains its identity.
  // The previous position remains untouched until this placement is confirmed.
  disposeMeasure(state.measures.get(item.id));
  state.items.set(item.id, item); state.measures.set(item.id, drawing.measure);
  bindMeasure(drawing.measure, item.id);
  remember(drawing.before); renderCreationOptions(); selectItem(item.id); queueSave(0); updateButtons();
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
    updateButtons();
  }, true);
  // Potree's InputHandler registered its bubble listener in the Viewer
  // constructor, before this one. Finish a point after its drop/drag handling;
  // a microtask in the capture listener can run before those handlers.
  // Polylines keep their own later insertion listener and preview marker.
  canvas.addEventListener("mouseup", (event) => {
    const drawing = state.drawing;
    if (event.button === 0 && drawing?.kind === "point" && drawing.confirmed === 1) finishDrawing();
  });
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
window.sleufbaseReloadEditor = async function () {
  try {
    if (await saveNow()) { state.allowClose = true; location.reload(); }
  } catch (_) {
    // A failed save already displays its error; keep the current edits open.
  }
};

function installUI() {
  for (const step of ["trench", "objects"]) {
    $id(`${step}-tab`).addEventListener("click", () => switchWorkflow(step));
    $id(`${step}-tab`).addEventListener("keydown", (event) => {
      if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
      event.preventDefault();
      const target = event.key === "Home" ? "trench" : event.key === "End" ? "objects" : step === "trench" ? "objects" : "trench";
      switchWorkflow(target);
      $id(`${state.workflow}-tab`).focus();
    });
  }
  $id("to-objects-button").addEventListener("click", () => switchWorkflow("objects"));
  for (const endpoint of ENDPOINTS) {
    $id(`${endpoint.prefix}-button`).addEventListener("click", () => startDrawing("point", endpoint.purpose));
    const select = () => {
      if (state.closeInProgress || !finishDrawing()) return;
      const item = endpointFor(endpoint.purpose);
      if (item) selectItem(item.id);
    };
    $id(`${endpoint.prefix}-select`).addEventListener("click", select);
    $id(`${endpoint.prefix}-label`).addEventListener("click", select);
    $id(`${endpoint.prefix}-select`).addEventListener("dblclick", () => {
      if (state.closeInProgress || !finishDrawing()) return;
      const item = endpointFor(endpoint.purpose);
      if (item) focusItem(item.id);
    });
  }
  $id("new-object-type").addEventListener("change", (event) => Object.assign(state.creation, readType(event.target.value)));
  $id("new-material").addEventListener("change", (event) => { state.creation.material = event.target.value; });
  for (const id of ["object-type", "attribute-0"]) {
    $id(id).addEventListener("change", (event) => {
      const item = state.items.get(state.selected);
      if (!item || item.purpose !== "object" || state.closeInProgress || state.drawing) return;
      finishFieldEdit(); finishDrag();
      const before = snapshot();
      if (id === "object-type") {
        const rename = nameLooksAutomatic(item);
        Object.assign(item, readType(event.target.value));
        if (rename) {
          item.name = defaultName(item.kind, item.object_type);
          const measure = state.measures.get(item.id);
          if (measure) measure.name = item.name;
        }
      } else item.attributes[0] = event.target.value;
      updateMeasureColors();
      remember(before); renderList(); renderCreationOptions(); renderInspector(); queueSave(0);
    });
  }
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
  $id("help-button").addEventListener("click", () => $id("help-dialog").showModal());
  $id("close-help-button").addEventListener("click", () => $id("help-dialog").close());
  $id("point-size").addEventListener("input", (event) => {
    if (state.cloud) state.cloud.material.size = Number(event.target.value);
    $id("point-size-value").textContent = number.format(Number(event.target.value));
  });
  $id("color-mode").addEventListener("change", (event) => {
    if (state.cloud) state.cloud.material.activeAttributeName = event.target.value;
  });
  for (const field of [$id("object-name"), ...[1, 2].map((index) => $id(`attribute-${index}`))]) {
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
    if (editing || !state.ready || state.closeInProgress || event.altKey) return;
    let handled = true;
    if (modifier && key === "z") event.shiftKey ? redo() : undo();
    else if (modifier && key === "y") redo();
    else if (event.key === "Escape") { cancelDrawing(); clearError(); }
    else if (event.key === "Enter" && state.drawing) finishDrawing();
    else if (event.key === "Delete") deleteSelected();
    else if (!modifier && key === "b") startDrawing("point", "trench_start");
    else if (!modifier && key === "e") startDrawing("point", "trench_end");
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
  const endpoints = new Set();
  for (const item of source.annotations) {
    if (!item || typeof item.id !== "string" || ids.has(item.id) || typeof item.name !== "string"
        || !["point", "polyline"].includes(item.kind) || !Array.isArray(item.vertices)
        || !item.vertices.every(finitePoint) || !Array.isArray(item.attributes) || item.attributes.length !== 3
        || !item.attributes.every((value) => typeof value === "string")
        || (item.kind === "point" ? item.vertices.length !== 1 : item.vertices.length < 2)) {
      throw new Error("Een bewaard tekenobject is ongeldig. De bestaande gegevens zijn niet gewijzigd.");
    }
    // Old projects predate explicit endpoint roles and the shared type catalog.
    if (item.purpose === undefined) item.purpose = "object";
    if (item.object_type === undefined) item.object_type = "";
    if (item.type_key === undefined) item.type_key = "";
    if (!["object", "trench_start", "trench_end"].includes(item.purpose)
        || typeof item.object_type !== "string" || typeof item.type_key !== "string"
        || (item.purpose !== "object" && (item.kind !== "point" || endpoints.has(item.purpose)))) {
      throw new Error("De bewaarde begin-/eindpunten of objecttypes zijn ongeldig. De bestaande gegevens zijn niet gewijzigd.");
    }
    if (item.purpose !== "object") endpoints.add(item.purpose);
    ids.add(item.id);
  }
}

function validateOptions(options) {
  if (!options || !Array.isArray(options.types) || !Array.isArray(options.materials)
      || !options.materials.every((value) => typeof value === "string")
      || !options.types.every((type) => type && typeof type.label === "string" && typeof type.key === "string"
        && Array.isArray(type.color) && type.color.length === 3 && type.color.every((value) => Number.isInteger(value) && value >= 0 && value <= 255))) {
    throw new Error("De type- en materiaalkeuzes van dit project konden niet worden gelezen.");
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
  try {
    window.sleufbaseDiagnostics?.setStage("Editor initialiseren");
    installUI();
    if (!window.Potree?.Viewer) throw new Error("De lokale Potree-viewer ontbreekt in deze installatie.");
    window.sleufbaseDiagnostics?.setStage("Brongegevens ophalen");
    const response = await fetch("./api/source", { cache: "no-store" });
    if (!response.ok) throw new Error("De verbinding met de lokale SleufBase-editor is niet beschikbaar.");
    const source = await response.json();
    validateSource(source);
    state.source = source;
    state.revision = Number.isSafeInteger(source.revision) ? source.revision : 0;
    window.sleufbaseDiagnostics?.setStage("Type- en materiaalkeuzes ophalen");
    const optionsResponse = await fetch("./api/options", { cache: "no-store" });
    if (!optionsResponse.ok) throw new Error(`De projectkeuzes voor type en materiaal zijn niet beschikbaar (HTTP ${optionsResponse.status}).`);
    const options = await optionsResponse.json();
    validateOptions(options);
    state.catalog = options;
    $id("source-title").textContent = source.title || "Puntenwolk";
    document.title = `${source.title || "Puntenwolk"} · SleufBase`;
    $id("source-path").textContent = source.source_path || "Lokaal bestand";
    $id("source-points").textContent = integer.format(source.point_count);
    $id("source-crs").textContent = source.crs === "rd_new" ? "RD New (EPSG:28992)"
      : source.projection || (source.crs === "unknown" ? "Niet vastgelegd in het bestand" : "Coördinatenstelsel uit het bronbestand");
    $id("coordinate-system-status").textContent = source.crs === "rd_new" ? "RD New · originele XYZ" : "Originele XYZ · broncoördinaten";
    window.sleufbaseDiagnostics?.setStage("3D-viewer initialiseren");
    state.viewer = new Potree.Viewer($id("potree_render_area"));
    state.viewer.setEDLEnabled(true);
    state.viewer.setFOV(60);
    state.viewer.setPointBudget(2_000_000);
    state.viewer.setBackground("gradient");
    state.viewer.setControls(state.viewer.orbitControls);
    state.viewer.measuringTool.showLabels = false;
    state.viewer.addEventListener("update", updateEndpointLabels);
    installDrawingCapture();
    restoreSnapshot(source.annotations);
    window.sleufbaseDiagnostics?.setStage("Puntenwolk laden");
    await loadCloud();
    state.ready = true;
    window.sleufbaseDiagnostics?.setStage("Editor gereed");
    $id("loading-overlay").hidden = true;
    $id("cloud-status").textContent = `${integer.format(source.point_count)} bronpunten · dynamische weergave`;
    updateButtons();
    new ResizeObserver(() => state.viewer?.renderer?.setSize($id("viewport").clientWidth, $id("viewport").clientHeight)).observe($id("viewport"));
  } catch (error) {
    state.loadFailed = true;
    if (window.sleufbaseDiagnostics) {
      window.sleufbaseDiagnostics.record("Opstartfout", error);
      window.sleufbaseDiagnostics.showFailure("Puntenwolk kon niet worden geopend",
        error.message || "Er ging iets mis bij het laden van de lokale puntenwolk.");
    } else {
      $id("loading-title").textContent = "Puntenwolk kon niet worden geopend";
      $id("loading-message").textContent = error.message || "Er ging iets mis bij het laden van de lokale puntenwolk.";
      $id("loading-spinner").hidden = true;
      $id("reload-button").hidden = false;
    }
    updateButtons();
  }
}

boot().catch((error) => {
  // Also retain failures while presenting an earlier startup error.
  if (window.sleufbaseDiagnostics) {
    window.sleufbaseDiagnostics.record("Editor starten", error);
    window.sleufbaseDiagnostics.showFailure("De editor kon niet starten",
      "Het starten van de editor is onderbroken. Bekijk de technische details.");
  }
});
