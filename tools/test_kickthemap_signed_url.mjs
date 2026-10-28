// Execute the actual browser helper without an account, browser or network.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const source = fs.readFileSync(new URL('../rust/src/kickthemap_signed_url.js', import.meta.url), 'utf8');
function harness(reply, fastTimer = false, origin = 'https://www.my.kickthemap.com') {
  const calls = [], timers = new Set();
  const context = { AbortController, FormData, URLSearchParams, TextDecoder, location: { origin },
    document: { querySelector: () => ({ content: 'test-csrf' }) },
    fetch: (url, init) => { calls.push({ url, init }); return reply(url, init, calls.length); },
    setTimeout: (fn, ms) => { const timer = setTimeout(fn, fastTimer ? 10 : ms); timers.add(timer); return timer; },
    clearTimeout: timer => { timers.delete(timer); clearTimeout(timer); }
  };
  const result = vm.runInNewContext(`${source}(${JSON.stringify({ projectId: '42', fileName: 'file42.tiff' })})`, context);
  return { calls, timers, result };
}
const ok = (data = { url: 'https://s3.amazonaws.com/file.tiff' }) => new Response(JSON.stringify({ status: true, data }));
let count = 0;
async function scenario(fn) { await fn(); count++; }
await scenario(async () => {
  for (const origin of ['https://foreign.example', 'https://www.my.kickthemap.com:8443']) {
    const h = harness(async () => ok(), false, origin);
    await assert.rejects(h.result, /page origin/); assert.equal(h.calls.length, 0); assert.equal(h.timers.size, 0);
  }
});
await scenario(async () => {
  for (const data of ['https://s3.amazonaws.com/file.tiff', { url: 'https://s3.amazonaws.com/file.tiff' },
    { downloadUrl: 'https://s3.amazonaws.com/file.tiff' }, { download_url: 'https://s3.amazonaws.com/file.tiff' }]) {
    const h = harness(async () => ok(data));
    assert.equal(await h.result, 'https://s3.amazonaws.com/file.tiff');
    assert.equal(h.calls.length, 1); assert.equal(h.calls[0].url, '/jobs/get-file-url');
    const { init } = h.calls[0]; assert.equal(init.method, 'POST'); assert.equal(init.credentials, 'same-origin');
    assert.equal(init.body.get('projectId'), '42'); assert.equal(init.headers['X-CSRF-TOKEN'], 'test-csrf');
    assert.equal(h.timers.size, 0);
  }
});
await scenario(async () => {
  const h = harness(async (_, __, n) => n === 1 ? new Response('', { status: 415 }) : ok());
  await h.result; assert.equal(h.calls.length, 2);
  assert.equal(h.calls[1].init.headers['Content-Type'], 'application/x-www-form-urlencoded');
  assert.equal(h.calls[1].init.body.get('fileName'), 'file42.tiff'); assert.equal(h.timers.size, 0);
});
await scenario(async () => {
  const h = harness(async (_, __, n) => n < 3 ? new Response('', { status: 415 }) : ok());
  await h.result; assert.equal(h.calls.length, 3); assert.equal(h.calls[2].init.headers['Content-Type'], 'application/json');
  assert.equal(JSON.parse(h.calls[2].init.body).projectId, '42'); assert.equal(h.timers.size, 0);
});
await scenario(async () => {
  const h = harness(async () => new Response('{}', { headers: { 'content-length': '262145' } }));
  await assert.rejects(h.result, /too large/); assert.equal(h.calls.length, 1); assert.equal(h.timers.size, 0);
});
await scenario(async () => {
  let cancelled = false, pulls = 0;
  const h = harness(async () => new Response(new ReadableStream({
    pull(controller) { pulls++; controller.enqueue(new Uint8Array(262145)); },
    cancel() { cancelled = true; }
  })));
  await assert.rejects(h.result, /too large/); assert(cancelled); assert(pulls <= 3);
  assert.equal(h.calls.length, 1); assert.equal(h.timers.size, 0);
});
await scenario(async () => {
  let cancelled = false;
  const h = harness(async () => new Response(new ReadableStream({
    pull(controller) { controller.enqueue(new Uint8Array(0)); },
    cancel() { cancelled = true; }
  })));
  await assert.rejects(h.result, /too large/); assert(cancelled); assert.equal(h.calls.length, 1); assert.equal(h.timers.size, 0);
});
await scenario(async () => {
  const h = harness(async (_, __, n) => n === 1 ? new Response('<html>invalid JSON</html>') : ok());
  await h.result; assert.equal(h.calls.length, 2); assert.equal(h.timers.size, 0);
});
await scenario(async () => {
  const h = harness((_, init) => new Promise((_, reject) => init.signal.addEventListener('abort', () => reject(init.signal.reason), { once: true })), true);
  await assert.rejects(h.result); assert.equal(h.calls.length, 1); assert(h.calls[0].init.signal.aborted); assert.equal(h.timers.size, 0);
});
await scenario(async () => {
  const h = harness(async (_, init) => new Response(new ReadableStream({
    start(controller) { init.signal.addEventListener('abort', () => controller.error(init.signal.reason), { once: true }); }
  })), true);
  await assert.rejects(h.result); assert.equal(h.calls.length, 1); assert.equal(h.timers.size, 0);
});
await scenario(async () => {
  const h = harness(async () => new Response(null));
  await assert.rejects(h.result, /no signed URL/); assert.equal(h.calls.length, 3); assert.equal(h.timers.size, 0);
});
await scenario(async () => {
  const h = harness(async () => new Response(JSON.stringify({ status: false, data: { url: 'https://s3.amazonaws.com/file.tiff' } })));
  await assert.rejects(h.result, /no signed URL/); assert.equal(h.calls.length, 3); assert.equal(h.timers.size, 0);
});


await scenario(async () => {const h=harness(async()=>ok(),false,'https://my.kickthemap.com');assert.equal(await h.result,'https://s3.amazonaws.com/file.tiff');assert.equal(h.calls.length,1);assert.equal(h.timers.size,0);});

await scenario(async () => {for(const status of [1,' true ']){const h=harness(async()=>new Response(JSON.stringify({status,data:{url:'https://s3.amazonaws.com/file.tiff'}})));assert.equal(await h.result,'https://s3.amazonaws.com/file.tiff');assert.equal(h.calls.length,1);assert.equal(h.timers.size,0);}});
console.log(`Signed URL JS: ${count} response/form/stream-budget/abort/cleanup scenarios passed`);
