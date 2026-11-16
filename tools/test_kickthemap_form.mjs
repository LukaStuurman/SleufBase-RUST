// Offline DOM contract checks. No provider account, browser profile or network.
// Install jsdom@26.1.0 outside the repository and set
// SLEUFBASE_FORM_TEST_NODE_MODULES to that installation's node_modules directory.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import test from 'node:test';

const require = createRequire(import.meta.url);
const { JSDOM } = process.env.SLEUFBASE_FORM_TEST_NODE_MODULES
  ? require(path.join(process.env.SLEUFBASE_FORM_TEST_NODE_MODULES, 'jsdom'))
  : require('jsdom');
const source = readFileSync(new URL('../rust/src/kickthemap_form.js', import.meta.url), 'utf8');
const jobUrl = 'https://www.my.kickthemap.com/viewJob/123_2026-11-16';
const names = ['Name', 'Attribute 1', 'Attribute 2', 'Attribute 3'];
const defaults = ['speciale leiding', 'Gietijzer / custom', ' 2 × Ø110 / 63,5 mm ', '3 + reserve'];

function panel(prefix = 'a', explicit = true) {
  return `<form id="${prefix === 'a' ? 'sidebar_properties' : prefix + '_properties'}"><h2>Polyline ${prefix}</h2>`
    + names.map((name, index) => explicit
      ? `<div class="field"><label for="${prefix}${index}">${name}</label><input id="${prefix}${index}"></div>`
      : `<div class="field"><span>${name}</span><div><input id="${prefix}${index}"></div></div>`).join('')
    + `<input type="hidden" id="${prefix}-object-id" value="${prefix}"><button type="submit">Save</button></form>`;
}

function fixture(t, { html = panel(), url = jobUrl } = {}) {
  const dom = new JSDOM(`<!doctype html><body><div id="map"><canvas></canvas></div>${html}</body>`, {
    url, runScripts: 'outside-only', pretendToBeVisual: true,
  });
  t.after(() => dom.window.close());
  const { window } = dom;
  // jsdom has no layout engine. Supply only the missing geometry primitive;
  // the bridge still checks real styles, hidden ancestors and disabled fields.
  window.HTMLElement.prototype.getClientRects = function () { return [{ width: 100, height: 24 }]; };
  for (let index = 0; index < 4; index++) {
    const input = window.document.getElementById(`a${index}`);
    if (input) input.value = defaults[index];
  }
  const run = request => JSON.parse(window.eval(`${source}(${JSON.stringify(request)})`));
  const snapshot = () => run({ action: 'snapshot' });
  const apply = (old, fields = ['ls', 'PE', '63,5', '4'], changes = {}) => run({
    action: 'apply', token: old.token, url: old.url, original: old.values, fields, ...changes,
  });
  const raw = () => [0, 1, 2, 3].map(index => window.document.getElementById(`a${index}`).value);
  return { window, document: window.document, snapshot, apply, raw, run };
}

test('reading preserves custom raw values, has no input/save side effects and keeps a stable token', t => {
  const f = fixture(t);
  let events = 0;
  for (const name of ['input', 'change', 'keyup', 'submit']) f.document.addEventListener(name, () => events++);
  const first = f.snapshot().snapshot;
  assert.deepEqual(first.values, {
    name: defaults[0], material: defaults[1], diameter: defaults[2], bundle: defaults[3], cover_band: false,
  });
  assert.equal(f.snapshot().snapshot.token, first.token);
  assert.deepEqual(f.raw(), defaults);
  assert.equal(events, 0);
});

test('explicit apply uses native input events, treats text as data and never submits the site', t => {
  const f = fixture(t);
  const first = f.snapshot().snapshot;
  const events = [];
  let submits = 0;
  for (const type of ['input', 'change', 'keyup']) f.document.addEventListener(type, event => events.push(`${event.target.id}:${type}`));
  f.document.addEventListener('submit', event => { submits++; event.preventDefault(); });
  const fields = ['ms', '\" ); globalThis.injected = true; // </script>', 'Ø 42,5 mm', '2+1'];
  const changed = f.apply(first, fields);
  assert.ok(changed.snapshot);
  assert.notEqual(changed.snapshot.token, first.token);
  assert.deepEqual(f.raw(), fields);
  assert.equal(f.window.injected, undefined);
  assert.deepEqual(events, [0, 1, 2, 3].flatMap(index => ['input', 'change', 'keyup'].map(type => `a${index}:${type}`)));
  assert.equal(submits, 0);
  assert.equal(f.apply(first).snapshot, null, 'a consumed token cannot overwrite the new form');
  assert.deepEqual(f.raw(), fields);
});

test('an incompatible number diameter is rejected before any real value or event changes', t => {
  const f = fixture(t);
  const diameter = f.document.getElementById('a2');
  diameter.type = 'number';
  diameter.value = '40';
  const original = f.raw();
  const old = f.snapshot().snapshot;
  const events = [];
  for (const type of ['input', 'change', 'keyup', 'submit']) {
    f.document.addEventListener(type, event => events.push(`${event.target.id}:${type}`));
  }
  const rejected = f.apply(old, ['ms', 'PVC', '2 × 40', '6']);
  assert.equal(rejected.snapshot, null);
  assert.match(rejected.message, /Diameter/);
  assert.match(rejected.message, /geen velden gewijzigd/);
  assert.deepEqual(f.raw(), original);
  assert.deepEqual(events, []);
  assert.equal(f.snapshot().snapshot.token, old.token, 'a read-only rejection does not consume the selection');
});

test('a number diameter that preserves its native value can still be applied', t => {
  const f = fixture(t);
  const diameter = f.document.getElementById('a2');
  diameter.type = 'number';
  diameter.value = '40';
  const old = f.snapshot().snapshot;
  const fields = ['ms', 'PVC', '63.5', '6'];
  const changed = f.apply(old, fields);
  assert.ok(changed.snapshot);
  assert.deepEqual(f.raw(), fields);
  assert.equal(changed.snapshot.values.diameter, '63.5');
});

test('nested sibling rows from the original label-based sidebar are recognized', t => {
  const f = fixture(t, { html: panel('a', false) });
  assert.ok(f.snapshot().snapshot);
});

test('a hidden duplicate panel does not make the visible selection ambiguous', t => {
  const f = fixture(t, { html: panel() + `<div style="display:none">${panel('b')}</div>` });
  assert.ok(f.snapshot().snapshot);
});

test('an unrelated job Name field outside the complete properties sidebar is not guessed or changed', t => {
  const f = fixture(t, { html: '<div><label for="job-name">Name</label><input id="job-name" value="Project title"></div>' + panel() });
  const old = f.snapshot().snapshot;
  assert.ok(old);
  assert.ok(f.apply(old).snapshot);
  assert.equal(f.document.getElementById('job-name').value, 'Project title');
});

test('two visible property forms are rejected without writing either form', t => {
  const f = fixture(t, { html: panel() + panel('b') });
  assert.equal(f.snapshot().snapshot, null);
  assert.deepEqual(f.raw(), defaults);
});

for (const reason of ['disabled', 'readonly', 'hidden', 'missing']) {
  test(`a ${reason} required field is not replaced with an unrelated nearby input`, t => {
    const f = fixture(t);
    const field = f.document.getElementById('a1');
    if (reason === 'missing') field.remove();
    else field.setAttribute(reason, '');
    field.parentElement?.insertAdjacentHTML('beforeend', '<input id="unrelated" value="leave alone">');
    assert.equal(f.snapshot().snapshot, null);
    assert.equal(f.document.getElementById('unrelated')?.value ?? 'leave alone', 'leave alone');
  });
}

test('ambiguous fallback rows with two editable inputs are rejected', t => {
  const f = fixture(t, { html: panel('a', false) });
  f.document.getElementById('a1').insertAdjacentHTML('afterend', '<input value="not the material">');
  assert.equal(f.snapshot().snapshot, null);
});

test('taking a snapshot before the dynamic sidebar exists does not change later inputs', t => {
  const f = fixture(t, { html: '' });
  assert.equal(f.snapshot().snapshot, null);
  f.document.body.insertAdjacentHTML('beforeend', panel());
  for (let i = 0; i < 4; i++) f.document.getElementById(`a${i}`).value = defaults[i];
  assert.ok(f.snapshot().snapshot);
  assert.deepEqual(f.raw(), defaults);
});

test('changed input properties invalidate a snapshot even without DOM events', t => {
  const f = fixture(t);
  const old = f.snapshot().snapshot;
  f.document.getElementById('a1').value = 'changed by provider';
  assert.equal(f.apply(old).snapshot, null);
  assert.equal(f.raw()[1], 'changed by provider');
});

test('replacement elements with identical values cannot reuse a selection token', t => {
  const f = fixture(t);
  const old = f.snapshot().snapshot;
  const field = f.document.getElementById('a0');
  const replacement = field.cloneNode();
  replacement.value = field.value;
  field.replaceWith(replacement);
  assert.equal(f.apply(old).snapshot, null);
  assert.deepEqual(f.raw(), defaults);
});

for (const event of ['mousedown', 'keydown', 'change']) {
  test(`a ${event} selection action invalidates reused inputs even when all text is identical`, t => {
    const f = fixture(t);
    const old = f.snapshot().snapshot;
    f.document.getElementById('map').dispatchEvent(new f.window.Event(event, { bubbles: true }));
    assert.equal(f.apply(old).snapshot, null);
    assert.deepEqual(f.raw(), defaults);
    assert.notEqual(f.snapshot().snapshot.token, old.token);
  });
}

test('selection metadata mutations in the scoped sidebar invalidate unchanged inputs', async t => {
  const f = fixture(t);
  const old = f.snapshot().snapshot;
  f.document.getElementById('a-object-id').setAttribute('value', 'other-line');
  await Promise.resolve();
  assert.equal(f.apply(old).snapshot, null);
  assert.deepEqual(f.raw(), defaults);
});

test('map animation mutations outside the properties panel do not invalidate the form', async t => {
  const f = fixture(t);
  const old = f.snapshot().snapshot;
  f.document.querySelector('canvas').style.transform = 'translate(10px, 20px)';
  f.document.getElementById('map').append(f.document.createElement('div'));
  await Promise.resolve();
  assert.equal(f.snapshot().snapshot.token, old.token);
  assert.ok(f.apply(old).snapshot);
});

test('moving to a different job rejects an old token before any field is changed', t => {
  const f = fixture(t);
  const old = f.snapshot().snapshot;
  f.window.history.pushState({}, '', '/viewJob/other_2026-11-16');
  assert.equal(f.apply(old).snapshot, null);
  assert.deepEqual(f.raw(), defaults);
});

test('a modified original snapshot cannot be used as an apply precondition', t => {
  const f = fixture(t);
  const old = f.snapshot().snapshot;
  assert.equal(f.apply(old, undefined, { original: { ...old.values, name: 'different' } }).snapshot, null);
  assert.deepEqual(f.raw(), defaults);
});

test('dekband is a reserved Attribute 3 value while diameter stays literal', t => {
  const f = fixture(t);
  f.document.getElementById('a3').value = '  DeKBand  ';
  const old = f.snapshot().snapshot;
  assert.equal(old.values.cover_band, true);
  assert.equal(old.values.bundle, '');
  assert.equal(f.raw()[3], '  DeKBand  ', 'reading must not normalize the real field');
  const next = f.apply(old, ['ls', 'PE', ' 63,5 mm ', 'dekband']).snapshot;
  assert.equal(next.values.cover_band, true);
  assert.equal(next.values.diameter, ' 63,5 mm ');
});

for (const url of [
  'https://example.com/viewJob/1', 'https://my.kickthemap.com.evil.test/viewJob/1',
  'http://www.my.kickthemap.com/viewJob/1', 'https://www.my.kickthemap.com/signin',
  'https://www.my.kickthemap.com/viewJob/', 'https://www.my.kickthemap.com/viewJob/1/other',
  'https://www.my.kickthemap.com/viewJob/1?other=2', 'https://www.my.kickthemap.com/viewJob/1#other',
  'https://www.my.kickthemap.com:444/viewJob/1', 'https://user@www.my.kickthemap.com/viewJob/1',
]) {
  test(`no form access outside the exact job origin/route: ${url}`, t => {
    const f = fixture(t, { url });
    assert.equal(f.snapshot().snapshot, null);
    assert.deepEqual(f.raw(), defaults);
  });
}
