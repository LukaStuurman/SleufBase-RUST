(function (config) {
  'use strict';
  const trustedJob = () => {
    const url = new URL(window.location.href);
    return window.top === window && url.protocol === 'https:' && (!url.port || url.port === '443')
      && ['www.my.kickthemap.com', 'my.kickthemap.com'].includes(url.hostname)
      && !url.username && !url.password && !url.search && !url.hash && /^\/viewJob\/[^/]+$/.test(url.pathname);
  };
  if (!trustedJob()) return false;
  const key = '__sleufbaseKickTheMapPageEditorV1';
  const signature = JSON.stringify(config);
  if (window[key]?.signature === signature) { window[key].schedule(); return true; }
  window[key]?.dispose?.();

  const profiles = Array.isArray(config?.profile_options) ? config.profile_options : [];
  const materials = Array.isArray(config?.material_options) ? config.material_options : [];
  const captions = ['Kabel/Leiding afkorting', 'Materiaal', 'Diameter', 'Bundel (aantal)'];
  const normalize = value => String(value || '').replace(/\s+/g, ' ').trim().toLowerCase();
  const names = new Map([
    ['name', 0], ['kabel/leiding afkorting', 0],
    ['attribute 1', 1], ['attribute1', 1], ['materiaal', 1],
    ['attribute 2', 2], ['attribute2', 2], ['diameter', 2],
    ['attribute 3', 3], ['attribute3', 3], ['bundel (aantal)', 3],
  ]);
  const own = '[data-sleufbase-page-control]';
  const labels = 'label, span, div, p, td, th, dt';
  const panels = '#sidebar_properties, [id*="properties"], [class*="properties"], [id*="sidebar"], [class*="sidebar"], form';
  let records = [], current = null, timer = null, disposed = false, serial = 0;
  const visible = element => {
    if (!element?.isConnected || !element.getClientRects().length) return false;
    for (let parent = element; parent; parent = parent.parentElement) {
      const style = getComputedStyle(parent);
      if (parent.hidden || parent.inert || style.display === 'none' || ['hidden', 'collapse'].includes(style.visibility)) return false;
    }
    return true;
  };
  const recordFor = field => records.find(record => record.field === field);
  const writable = field => (field instanceof HTMLInputElement || field instanceof HTMLTextAreaElement)
    && !field.disabled && !field.readOnly && !field.matches(':disabled') && !field.closest(own)
    && (field instanceof HTMLTextAreaElement || ['text', 'number', 'search', 'tel'].includes(field.type))
    && visible(recordFor(field)?.control || field);
  const kindOf = label => {
    if (label.closest(own)) return undefined;
    const record = records.find(record => record.label === label);
    return record ? record.kind : (label.tagName === 'LABEL' || label.children.length === 0)
      ? names.get(normalize(label.textContent)) : undefined;
  };
  const hasLabel = element => kindOf(element) !== undefined
    || [...element.querySelectorAll(labels)].some(label => kindOf(label) !== undefined && visible(label));
  const inputs = element => [element, ...element.querySelectorAll('input, textarea')]
    .filter(field => field.matches('input, textarea') && !field.closest(own));
  function fieldFor(label) {
    const attached = records.find(record => record.label === label);
    if (attached?.field.isConnected) return writable(attached.field) ? [attached.field] : [];
    if (label.tagName === 'LABEL' && label.hasAttribute('for')) {
      const field = document.getElementById(label.htmlFor);
      return writable(field) ? [field] : [];
    }
    let cursor = label;
    for (let depth = 0; cursor && cursor !== document.body && depth < 3; depth++, cursor = cursor.parentElement) {
      const contained = inputs(cursor);
      if (contained.length) return contained.filter(writable);
      let sibling = cursor.nextElementSibling;
      for (let count = 0; sibling && count < 3; count++, sibling = sibling.nextElementSibling) {
        if (hasLabel(sibling)) break;
        const nearby = inputs(sibling);
        if (nearby.length) return nearby.filter(writable);
      }
      if (cursor.parentElement && [...cursor.parentElement.children].some(child => child !== cursor && hasLabel(child))) break;
    }
    return [];
  }
  function detect() {
    const complete = [...document.querySelectorAll(panels)].filter(panel => visible(panel)
      && new Set([...panel.querySelectorAll(labels)].filter(visible).map(kindOf).filter(kind => kind !== undefined)).size === 4);
    const independent = complete.filter(panel => !complete.some(other => other !== panel && panel.contains(other)));
    if (independent.length > 1) return null;
    const root = independent[0] || document;
    const found = [new Set(), new Set(), new Set(), new Set()], matched = [];
    for (const label of root.querySelectorAll(labels)) {
      const kind = kindOf(label);
      if (kind === undefined || !visible(label)) continue;
      const fields = fieldFor(label);
      if (fields.length !== 1) return null;
      found[kind].add(fields[0]);
      matched[kind] ||= label;
    }
    if (found.some(set => set.size !== 1)) return null;
    const fields = found.map(set => [...set][0]);
    if (new Set(fields).size !== 4) return null;
    let scope = fields[0].parentElement;
    while (scope && !fields.every(field => scope.contains(field))) scope = scope.parentElement;
    if (!scope || scope === document.body || scope === document.documentElement) return null;
    return { fields, labels: matched, scope };
  }
  function style(record, element, properties) {
    for (const [name, value] of Object.entries(properties)) {
      const previous = element.style.getPropertyValue(name), priority = element.style.getPropertyPriority(name);
      element.style.setProperty(name, value, 'important');
      record.undo.push(() => {
        if (element.style.getPropertyValue(name) === value) {
          if (previous) element.style.setProperty(name, previous, priority); else element.style.removeProperty(name);
        }
      });
    }
  }
  function attribute(record, element, name, value) {
    const previous = element.getAttribute(name);
    element.setAttribute(name, value);
    record.undo.push(() => { if (element.getAttribute(name) === value) {
      if (previous === null) element.removeAttribute(name); else element.setAttribute(name, previous);
    } });
  }
  function listen(record, element, event, callback) {
    element.addEventListener(event, callback);
    record.undo.push(() => element.removeEventListener(event, callback));
  }
  function relabel(record) {
    const walker = document.createTreeWalker(record.label, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      const node = walker.currentNode;
      if (node.parentElement?.closest(own) || names.get(normalize(node.nodeValue)) !== record.kind) continue;
      const previous = node.nodeValue;
      if (previous === captions[record.kind]) continue;
      node.nodeValue = captions[record.kind];
      record.undo.push(() => { if (node.nodeValue === captions[record.kind]) node.nodeValue = previous; });
    }
  }
  function compact(record, element) {
    style(record, element, { 'box-sizing': 'border-box', width: '104px', 'min-width': '104px',
      'max-width': '104px', position: 'relative', left: '-14px', 'margin-left': '0px' });
  }
  function control(record, tag) {
    const element = document.createElement(tag), appearance = getComputedStyle(record.field);
    element.dataset.sleufbasePageControl = '1';
    element.id = `sleufbase-page-field-${++serial}`;
    element.setAttribute('aria-label', captions[record.kind]);
    Object.assign(element.style, { font: appearance.font, color: appearance.color,
      background: appearance.backgroundColor || '#fff', border: appearance.border || '1px solid #bbb',
      borderRadius: appearance.borderRadius || '3px',
      height: `${Math.max(30, record.field.getBoundingClientRect().height || 30)}px` });
    compact(record, element);
    return element;
  }
  function replace(record, replacement) {
    record.control = replacement;
    record.field.insertAdjacentElement('afterend', replacement);
    record.undo.push(() => replacement.remove());
    style(record, record.field, { position: 'absolute', left: '-10000px', width: '1px', height: '1px', opacity: '0' });
    attribute(record, record.field, 'tabindex', '-1');
    attribute(record, record.field, 'aria-hidden', 'true');
    if (record.label.tagName === 'LABEL' && record.label.htmlFor === record.field.id) {
      attribute(record, record.label, 'for', replacement.id);
    }
  }
  function setValue(record, value) {
    const form = trustedJob() && detect();
    if (!form || form.fields[record.kind] !== record.field || !writable(record.field)) return false;
    const prototype = record.field instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(prototype, 'value')?.set;
    if (!setter) return false;
    try {
      const probe = record.field.cloneNode(false);
      setter.call(probe, value);
      if (probe.value !== value) return false;
      record.writing = true;
      setter.call(record.field, value);
      record.field.dispatchEvent(new Event('input', { bubbles: true }));
      record.field.dispatchEvent(new Event('change', { bubbles: true }));
      record.field.dispatchEvent(new KeyboardEvent('keyup', { bubbles: true, key: value }));
      return true;
    } catch (_) { return false;
    } finally { record.writing = false; schedule(); }
  }
  function dropdown(record, choices) {
    const select = control(record, 'select');
    // Match the original SleufBase select padding. Provider-wide select rules
    // otherwise leave a large empty strip before our compact choice text.
    // These are owned nodes, removed with the replacement control on cleanup.
    const alignText = element => {
      for (const [name, value] of Object.entries({ 'padding-left': '2px', 'text-indent': '0px',
        'text-align': 'left', 'text-align-last': 'left' })) element.style.setProperty(name, value, 'important');
    };
    alignText(select);
    const add = (value, label) => { const option = document.createElement('option'); option.value = value;
      option.textContent = label; option.title = value; alignText(option); select.appendChild(option); return option; };
    add('', '');
    choices.forEach(option => add(option.code, option.label));
    let custom = null, last = null, selected = '';
    record.sync = () => {
      if (!record.field.isConnected) return;
      const raw = record.field.value, normalized = normalize(raw);
      if (raw !== last) {
        const match = normalized ? choices.find(option => [option.code, option.label, ...option.keywords.split(',')]
          .some(value => normalize(value) === normalized)) : undefined;
        if (custom) { custom.remove(); custom = null; }
        if (raw && !match) custom = add(raw, raw);
        selected = match ? match.code : raw; last = raw;
      }
      if (select.value !== selected) select.value = selected;
    };
    listen(record, select, 'change', () => { setValue(record, select.value); record.sync(); });
    for (const event of ['input', 'change']) listen(record, record.field, event, record.sync);
    replace(record, select); record.sync();
  }
  function bundle(record) {
    const row = document.createElement('div'); row.dataset.sleufbasePageControl = '1';
    row.id = `sleufbase-page-field-${++serial}`;
    Object.assign(row.style, { display: 'inline-flex', flexDirection: 'column', alignItems: 'flex-start', gap: '7px', verticalAlign: 'top' });
    compact(record, row);
    const count = control(record, 'input'); count.type = 'text'; count.inputMode = 'numeric'; count.autocomplete = 'off';
    count.style.setProperty('left', '0px', 'important');
    const label = document.createElement('label');
    Object.assign(label.style, { display: 'inline-flex', alignItems: 'center', gap: '6px', minHeight: '22px', whiteSpace: 'nowrap', cursor: 'pointer' });
    const check = document.createElement('input'); check.type = 'checkbox'; check.setAttribute('aria-label', 'Dekband');
    Object.assign(check.style, { width: '16px', height: '16px', margin: '0' });
    label.append(check, document.createTextNode('Dekband')); row.append(count, label);
    record.sync = () => {
      const raw = record.field.value, cover = normalize(raw) === 'dekband';
      const value = cover ? '' : raw;
      if (count.value !== value) count.value = value;
      if (count.disabled !== cover) count.disabled = cover;
      check.checked = cover;
    };
    const write = () => {
      setValue(record, check.checked ? 'dekband' : count.value);
      record.sync();
    };
    for (const event of ['input', 'change']) listen(record, count, event, write);
    listen(record, check, 'change', write);
    for (const event of ['input', 'change']) listen(record, record.field, event, record.sync);
    replace(record, row); record.sync();
  }
  function clear() {
    for (const record of records) for (const undo of record.undo.reverse()) undo();
    records = []; current = null;
  }
  function observe() {
    if (!disposed) observer.observe(document.documentElement || document, {
      childList: true, subtree: true, characterData: true, attributes: true,
      attributeFilter: ['class', 'style', 'hidden', 'disabled', 'readonly', 'value'],
    });
  }
  function refresh() {
    timer = null;
    if (disposed) return;
    observer.disconnect();
    try {
      if (records.some(record => !record.field.isConnected || !record.label.isConnected || !record.control.isConnected)) clear();
      const form = trustedJob() ? detect() : null;
      if (!form) { clear(); return; }
      if (!current || form.scope !== current.scope || form.fields.some((field, index) => field !== current.fields[index])
          || form.labels.some((label, index) => label !== current.labels[index])) {
        clear(); current = form;
        form.fields.forEach((field, kind) => {
          const record = { field, kind, label: form.labels[kind], control: field, undo: [], sync: () => {}, writing: false };
          records.push(record); relabel(record);
          if (!record.label.contains(field)) style(record, record.label, { display: 'inline-block', width: '96px',
            'min-width': '96px', 'max-width': '96px', 'white-space': 'nowrap', 'vertical-align': 'top', 'box-sizing': 'border-box' });
          if (kind === 0 && profiles.length) dropdown(record, profiles);
          else if (kind === 1 && materials.length) dropdown(record, materials.map(value => ({ code: value, label: value, keywords: value })));
          else if (kind === 3) bundle(record);
          else compact(record, field);
        });
      } else { for (const record of records) { relabel(record); record.sync(); } }
    } finally { observe(); }
  }
  function schedule() { if (!disposed && timer === null) timer = setTimeout(refresh, 80); }
  const observer = new MutationObserver(schedule);
  const interval = setInterval(() => {
    if (!trustedJob()) { if (records.length) schedule(); return; }
    if (!records.length || records.some(record => !record.field.isConnected || !record.label.isConnected || !record.control.isConnected)) schedule();
    else for (const record of records) record.sync();
  }, 500);
  window[key] = { signature, schedule, dispose() {
    disposed = true; observer.disconnect(); clearTimeout(timer); clearInterval(interval); clear();
    if (window[key]?.signature === signature) delete window[key];
  } };
  observe(); schedule();
  return true;
})
