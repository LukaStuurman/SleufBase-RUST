// Actual native fetch/XHR hooks in a synthetic browser context. No browser/account I/O.
import fs from 'node:fs';import vm from 'node:vm';import assert from 'node:assert/strict';
const source=fs.readFileSync(new URL('../rust/src/kickthemap_capture.js',import.meta.url),'utf8');
const flush=async()=>{for(let i=0;i<8;i++)await new Promise(r=>setImmediate(r));};
function harness(response=()=>new Response(JSON.stringify({status:true,data:{url:'https://s3.amazonaws.com/f.tiff'}})),jobId='42'){
 let sends=0;
 class XHR{constructor(){this.events={};this.status=200;this.responseType='';this.responseText=JSON.stringify({status:true,data:{url:'https://s3.amazonaws.com/f.tiff'}});}open(){}addEventListener(k,v){this.events[k]=v;}send(){queueMicrotask(()=>this.events.load?.());}}
 const context={location:{origin:'https://www.my.kickthemap.com',href:'https://www.my.kickthemap.com/'},XMLHttpRequest:XHR,fetch:(...args)=>{sends++;return Promise.resolve(response(...args));},Request,Response,ReadableStream,FormData,URLSearchParams,URL,TextDecoder,setTimeout:()=>0};context.window=context;
 vm.runInNewContext(`(${source})(${JSON.stringify({nonce:'nonce-a',jobId})})`,context);
 return {context,get sends(){return sends;},get capture(){return context.__sleufbaseNativeDownloadCapture?.captured;}};
}
{
 const h=harness();const original=await h.context.fetch('/jobs/get-file-url?projectId=42&fileName=f.tiff');assert.equal(original.status,200);assert.equal(h.sends,1);await original.text();await flush();
 assert.equal(h.capture.job_id,'42');assert.equal(h.capture.request.method,'GET');assert.equal(h.capture.request.request_mode,'form');assert.equal(h.capture.request.response_url_path,'data.url');assert(!JSON.stringify(h.capture).includes('s3.amazonaws.com'));
 h.context.__sleufbaseNativeDownloadCapture.cleanup();assert.equal(h.capture,undefined);
}
{
 const h=harness();const form=new FormData();form.set('projectId','42');form.set('fileName','f.tiff');await h.context.fetch('/jobs/get-file-url',{method:'POST',body:form});await flush();assert.equal(h.capture.request.request_mode,'multipart');
}
{
 const h=harness();await h.context.fetch('/jobs/get-file-url',{method:'POST',body:JSON.stringify({projectId:43})});await flush();assert.equal(h.capture,null);
 await h.context.fetch('https://other.example/jobs/get-file-url?projectId=42');await flush();assert.equal(h.capture,null);
 await h.context.fetch('/jobs/get-file-url',{method:'POST',body:JSON.stringify({projectId:42,password:'never capture'})});await flush();assert.equal(h.capture,null);
}
{
 const h=harness();const xhr=new h.context.XMLHttpRequest();xhr.open('GET','/jobs/get-file-url?projectId=42');xhr.send();await flush();assert.equal(h.capture.request.method,'GET');assert.equal(h.capture.request.response_url_path,'data.url');
}
{
 const h=harness();const xhr=new h.context.XMLHttpRequest();xhr.open('POST','/jobs/get-file-url');xhr.send(new URLSearchParams({projectId:'42'}));await flush();assert.equal(h.capture.request.request_mode,'form');
}
{
 let resolveRead;const stream=new ReadableStream({start(controller){resolveRead=()=>controller.close();}});const h=harness(()=>new Response(stream));
 const result=await Promise.race([h.context.fetch('/jobs/get-file-url?projectId=42'),new Promise((_,reject)=>setTimeout(()=>reject(Error('hook blocked fetch response')),100))]);assert.equal(result.status,200);resolveRead();await flush();assert.equal(h.capture,null);
}
{
 let pulls=0;const h=harness(()=>new Response(new ReadableStream({pull(controller){pulls++;controller.enqueue(new Uint8Array(524289));if(pulls>4)controller.close();}})));
 const response=await h.context.fetch('/jobs/get-file-url?projectId=42');await flush();assert.equal(h.capture,null);assert(pulls<=4,'capture must stop reading after budget');await response.body.cancel();
}
{
 const h=harness();const request=new Request('https://www.my.kickthemap.com/jobs/get-file-url',{method:'POST',body:JSON.stringify({projectId:42,padding:'x'.repeat(262145)})});await h.context.fetch(request);await flush();assert.equal(h.capture,null);
}
{
 const h=harness();const form=new FormData();form.set('projectId','42');form.set('fileName','f.tiff');const request=new Request('https://www.my.kickthemap.com/jobs/get-file-url',{method:'POST',body:form});await h.context.fetch(request);await flush();assert.equal(h.capture.request.request_mode,'multipart');
}
console.log('Native capture JS: 9 fetch/XHR/GET/body-budget/nonblocking/identity/cleanup scenarios passed');
await import('./test_kickthemap_signed_url.mjs');
