// Execute the production storage bridge with synthetic responses, never an account.
import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';
const source=fs.readFileSync(new URL('../rust/src/kickthemap_storage_config.js',import.meta.url),'utf8');
function harness(reply,origin='https://www.my.kickthemap.com'){
 const calls=[],timers=new Set();const context={location:{origin},document:{querySelector:()=>({content:'fixture-csrf'})},AbortController,TextDecoder,
 fetch:(url,init)=>{calls.push({url,init});return Promise.resolve(reply(url,init));},setTimeout:(fn,ms)=>{const t=setTimeout(fn,ms);timers.add(t);return t;},clearTimeout:t=>{timers.delete(t);clearTimeout(t);}};
 return{calls,timers,result:vm.runInNewContext(`${source}()`,context)};
}
for(const origin of ['https://www.my.kickthemap.com','https://my.kickthemap.com']){
 const h=harness(()=>new Response('{"adata":"synthetic","thg":"fixture-only"}'),origin);const payload=await h.result;assert.equal(JSON.parse(payload).adata,'synthetic');assert.equal(h.calls[0].url,'/main/adata');assert.equal(h.calls[0].init.method,'POST');assert.equal(h.calls[0].init.credentials,'same-origin');assert.equal(h.calls[0].init.headers['X-CSRF-TOKEN'],'fixture-csrf');assert.equal(h.timers.size,0);
}
for(const origin of ['https://other.example','http://my.kickthemap.com']){const h=harness(()=>new Response('{}'),origin);await assert.rejects(h.result,/origin/);assert.equal(h.calls.length,0);assert.equal(h.timers.size,0);}
{
 const h=harness(()=>new Response('',{status:403}));await assert.rejects(h.result,/HTTP 403/);assert.equal(h.calls.length,1);assert.equal(h.timers.size,0);
}
{
 let cancelled=false;const h=harness(()=>new Response(new ReadableStream({cancel(){cancelled=true;}}),{headers:{'content-length':'65537'}}));await assert.rejects(h.result,/too large/);assert(cancelled);assert.equal(h.timers.size,0);
}
{
 let cancelled=false;const h=harness(()=>new Response(new ReadableStream({pull(c){c.enqueue(new Uint8Array(65537));},cancel(){cancelled=true;}})));await assert.rejects(h.result,/too large/);assert(cancelled);assert.equal(h.timers.size,0);
}
console.log('Storage bridge: trusted aliases, authenticated POST, forbidden origin, HTTP error, byte limits and cleanup passed');
