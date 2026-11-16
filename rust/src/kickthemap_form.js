(function (request) {
  'use strict';
  const reply = (snapshot, message) => JSON.stringify({ snapshot, message });
  const stale = () => reply(null, 'De selectie of het formulier is gewijzigd. Lees de kabelgegevens opnieuw.');
  const url = new URL(window.location.href);
  if (window.top !== window || url.protocol !== 'https:' || url.port && url.port !== '443'
      || !['www.my.kickthemap.com', 'my.kickthemap.com'].includes(url.hostname)
      || url.username || url.password || url.search || url.hash || !/^\/viewJob\/[^/]+$/.test(url.pathname)) {
    return reply(null, 'Open eerst een KickTheMap-job en selecteer of teken een kabel of leiding.');
  }

  const key = '__sleufbaseNativeCableFormV1';
  let state = window[key];
  if (!state) {
    state = { revision: 0, current: null, scope: null, writing: false, observer: null };
    const changed = () => { if (!state.writing) { state.revision++; state.current = null; } };
    for (const event of ['pointerdown', 'mousedown', 'touchstart', 'keydown', 'input', 'change']) {
      document.addEventListener(event, changed, true);
    }
    state.observer = new MutationObserver(changed);
    window[key] = state;
  }

  const normalize = text => String(text || '').replace(/\s+/g, ' ').trim().toLowerCase();
  const labels = new Map([
    ['name', 0], ['kabel/leiding afkorting', 0],
    ['attribute 1', 1], ['attribute1', 1], ['materiaal', 1],
    ['attribute 2', 2], ['attribute2', 2], ['diameter', 2],
    ['attribute 3', 3], ['attribute3', 3], ['bundel (aantal)', 3],
  ]);
  const labelSelector = 'label, span, div, p, td, th, dt';
  const panelSelector = '#sidebar_properties, [id*="properties"], [class*="properties"], [id*="sidebar"], [class*="sidebar"], form';
  const visible = element => {
    if (!element?.isConnected || !element.getClientRects().length) return false;
    for (let parent = element; parent; parent = parent.parentElement) {
      const style = getComputedStyle(parent);
      if (parent.hidden || parent.inert || style.display === 'none' || ['hidden', 'collapse'].includes(style.visibility)) return false;
    }
    return true;
  };
  const writable = element => (element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement)
    && !element.disabled && !element.readOnly && !element.matches(':disabled') && visible(element)
    && (element instanceof HTMLTextAreaElement || ['text', 'number', 'search', 'email', 'url', 'tel'].includes(element.type));
  const labelKind = element => (element.tagName === 'LABEL' || element.children.length === 0)
    ? labels.get(normalize(element.textContent)) : undefined;
  const hasLabel = element => labelKind(element) !== undefined
    || [...element.querySelectorAll(labelSelector)].some(child => labelKind(child) !== undefined && visible(child));
  const controls = element => [element, ...element.querySelectorAll('input, textarea')]
    .filter(item => item.matches('input, textarea'));

  function fieldFor(label) {
    if (label.tagName === 'LABEL' && label.hasAttribute('for')) {
      const target = document.getElementById(label.htmlFor);
      return writable(target) ? [target] : [];
    }
    let cursor = label;
    for (let depth = 0; cursor && depth < 3 && cursor !== document.body; depth++, cursor = cursor.parentElement) {
      const contained = controls(cursor);
      if (contained.length) return contained.filter(writable);
      let sibling = cursor.nextElementSibling;
      for (let count = 0; sibling && count < 3; count++, sibling = sibling.nextElementSibling) {
        if (hasLabel(sibling)) break;
        const nearby = controls(sibling);
        if (nearby.length) return nearby.filter(writable);
      }
      // Do not climb out of one field row into a parent containing other labels.
      if (cursor.parentElement && [...cursor.parentElement.children]
        .some(child => child !== cursor && hasLabel(child))) break;
    }
    return [];
  }

  function detect() {
    // Prefer an actual complete properties panel over unrelated labels such as
    // the job's Name field. Multiple complete visible panels remain ambiguous.
    const panels = [...document.querySelectorAll(panelSelector)].filter(panel => visible(panel)
      && new Set([...panel.querySelectorAll(labelSelector)].filter(visible).map(labelKind)
        .filter(kind => kind !== undefined)).size === 4);
    const independent = panels.filter(panel => !panels.some(other => other !== panel && panel.contains(other)));
    if (independent.length > 1) return null;
    const root = independent[0] || document;
    const found = [new Set(), new Set(), new Set(), new Set()];
    for (const label of root.querySelectorAll(labelSelector)) {
      const kind = labelKind(label);
      if (kind === undefined || !visible(label)) continue;
      const candidates = fieldFor(label);
      if (candidates.length !== 1) return null;
      found[kind].add(candidates[0]);
    }
    if (found.some(set => set.size !== 1)) return null;
    const fields = found.map(set => [...set][0]);
    if (new Set(fields).size !== 4) return null;
    let scope = fields[0].parentElement;
    while (scope && !fields.every(field => scope.contains(field))) scope = scope.parentElement;
    if (!scope || scope === document.body || scope === document.documentElement) return null;
    const panel = fields[0].closest('#sidebar_properties') || fields[0].closest(panelSelector);
    if (panel && panel !== document.body && fields.every(field => panel.contains(field))) scope = panel;
    return { fields, scope };
  }

  const values = raw => {
    const cover_band = normalize(raw[3]) === 'dekband';
    return { name: raw[0], material: raw[1], diameter: raw[2], bundle: cover_band ? '' : raw[3], cover_band };
  };
  const sameValues = (a, b) => a && b && ['name', 'material', 'diameter', 'bundle', 'cover_band'].every(key => a[key] === b[key]);
  function take(form) {
    state.observer.disconnect();
    state.scope = form.scope;
    state.observer.observe(form.scope, { childList: true, subtree: true, attributes: true, characterData: true });
    const raw = form.fields.map(field => field.value);
    const token = crypto.randomUUID();
    const snapshot = { token, url: window.location.href, values: values(raw) };
    state.current = { snapshot, fields: form.fields, raw, scope: form.scope, revision: state.revision };
    return snapshot;
  }

  if (request?.action === 'snapshot') {
    if (state.observer.takeRecords().length) { state.revision++; state.current = null; }
    const form = detect();
    if (!form) {
      state.current = null;
      return reply(null, 'Selecteer of teken één kabel of leiding met de vier eigenschappenvelden Naam, Materiaal, Diameter en Bundel.');
    }
    const current = state.current;
    if (current && current.snapshot.url === window.location.href && current.revision === state.revision
        && current.scope === form.scope && form.fields.every((field, index) => field === current.fields[index] && field.value === current.raw[index])) {
      return reply(current.snapshot, 'Kabelgegevens gelezen. Pas ze aan en kies Invullen; opslaan gebeurt in KickTheMap.');
    }
    return reply(take(form), 'Kabelgegevens gelezen. Pas ze aan en kies Invullen; opslaan gebeurt in KickTheMap.');
  }
  if (request?.action !== 'apply' || !Array.isArray(request.fields) || request.fields.length !== 4
      || request.fields.some(field => typeof field !== 'string')) return reply(null, 'Ongeldige formulieropdracht.');

  if (state.observer.takeRecords().length) { state.revision++; state.current = null; }
  const current = state.current;
  const form = detect();
  if (!current || !form || request.token !== current.snapshot.token || request.url !== window.location.href
      || request.url !== current.snapshot.url || current.revision !== state.revision
      || !sameValues(request.original, current.snapshot.values) || form.scope !== current.scope
      || !form.fields.every((field, index) => field === current.fields[index] && field.value === current.raw[index])) return stale();

  const setValue = (field, value) => {
    const prototype = field instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(prototype, 'value').set.call(field, value);
  };
  // Native input types can silently sanitize text, e.g. number turns "2 × 40"
  // into an empty value. Check detached copies before changing any real field.
  try {
    for (let index = 0; index < form.fields.length; index++) {
      const probe = form.fields[index].cloneNode(false);
      setValue(probe, request.fields[index]);
      if (probe.value !== request.fields[index]) {
        const name = ['Naam', 'Materiaal', 'Diameter', 'Bundel'][index];
        return reply(null, `Het KickTheMap-veld ${name} accepteert deze waarde niet. Er zijn geen velden gewijzigd.`);
      }
    }
  } catch (_) {
    return reply(null, 'De waarden konden niet worden gecontroleerd. Er zijn geen velden gewijzigd.');
  }

  state.current = null;
  state.writing = true;
  try {
    for (let index = 0; index < form.fields.length; index++) {
      if (window.location.href !== current.snapshot.url || !form.fields.every(writable)) return stale();
      const field = form.fields[index];
      setValue(field, request.fields[index]);
      field.dispatchEvent(new Event('input', { bubbles: true }));
      field.dispatchEvent(new Event('change', { bubbles: true }));
      field.dispatchEvent(new KeyboardEvent('keyup', { bubbles: true, key: request.fields[index] }));
    }
  } catch (_) {
    return reply(null, 'Het formulier veranderde tijdens invullen. Controleer de velden en lees de kabelgegevens opnieuw.');
  } finally {
    state.writing = false;
  }
  state.observer.takeRecords();
  state.revision++;
  const after = detect();
  if (!after || after.scope !== form.scope || window.location.href !== current.snapshot.url
      || !after.fields.every((field, index) => field === form.fields[index] && field.value === request.fields[index])) return stale();
  return reply(take(after), 'Kabelgegevens ingevuld. Controleer ze en sla op met de knop van KickTheMap.');
})
