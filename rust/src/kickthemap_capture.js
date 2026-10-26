(config=>{
 const key='__sleufbaseNativeDownloadCapture';
 if(location.origin!=='https://www.my.kickthemap.com')throw Error('Wrong origin');
 if(window[key]?.cleanup)window[key].cleanup();
 const state={nonce:config.nonce,jobId:String(config.jobId),captured:null,expires:Date.now()+120000};
 const alive=()=>window[key]===state&&Date.now()<state.expires;
 const boundedText=async(response,limit)=>{
  if(Number(response.headers?.get('content-length')||0)>limit)return null;
  const reader=response.body?.getReader();if(!reader)return null;
  let size=0,text='';const decoder=new TextDecoder();
  try{while(true){const chunk=await reader.read();if(chunk.done)break;size+=chunk.value.byteLength;if(size>limit){await reader.cancel();return null;}text+=decoder.decode(chunk.value,{stream:true});}return text+decoder.decode();}
  catch(_){try{await reader.cancel();}catch(_){}return null;}
 };
 const fields=async body=>{
  const result={};let mode;let count=0;
  const put=(k,v)=>{if(typeof v!=='string'&&typeof v!=='number'&&typeof v!=='boolean')return true;if(++count>64||k.length>128||String(v).length>8192||/token|password|cookie|secret|authorization|credential/i.test(k))return false;result[k]=v;return true;};
  if(body instanceof FormData){mode='multipart';for(const[k,v]of body.entries())if(typeof v==='string'&&!put(k,v))return null;}
  else if(body instanceof URLSearchParams){mode='form';for(const[k,v]of body.entries())if(!put(k,v))return null;}
  else if(typeof body==='string'&&body.length<=262144){try{const value=JSON.parse(body);if(value&&typeof value==='object'&&!Array.isArray(value)){mode='json';for(const k in value)if(Object.hasOwn(value,k)&&!put(k,value[k]))return null;}}catch(_){}if(!mode){mode='form';for(const[k,v]of new URLSearchParams(body))if(!put(k,v))return null;}}
  if(!mode||!Object.values(result).some(v=>String(v)===state.jobId))return null;
  return {request_mode:mode,fields:result};
 };
 const findUrlPath=payload=>{let visited=0;const walk=(v,path=[],depth=0)=>{if(depth>12||++visited>2048)return null;if(typeof v==='string'&&/^https:\/\//i.test(v))return path.join('.');if(v&&typeof v==='object'){for(const k in v){if(!Object.hasOwn(v,k))continue;const found=walk(v[k],[...path,k],depth+1);if(found!==null)return found;if(visited>2048)break;}}return null;};return walk(payload);};
 const info=async(url,method,body)=>{
  try{url=new URL(url,location.href);if(url.origin!==location.origin)return null;
   method=String(method||'GET').toUpperCase();const parsed=await fields(method==='GET'?url.searchParams:body);return parsed?{endpoint:url.origin+url.pathname,method,...parsed}:null;
  }catch(_){return null;}
 };
 const publish=(request,payload)=>{
  if(!alive()||state.captured||!request||!payload)return;
  const path=findUrlPath(payload);if(path===null)return;
  const result={nonce:state.nonce,job_id:state.jobId,status:'captured',request:{...request,response_url_path:path}};
  if(JSON.stringify(result).length<=262144)state.captured=result;
 };
 const originalFetch=window.fetch;const originalOpen=XMLHttpRequest.prototype.open;const originalSend=XMLHttpRequest.prototype.send;
 const fetchHook=function(input,init){
  const details=(async()=>{try{const request=input instanceof Request?input:null;let body=init?.body;
   if(body===undefined&&request&&String(init?.method||request.method).toUpperCase()!=='GET'){
    const clone=request.clone();const text=await boundedText(clone,262144);if(text===null)return null;
    body=/multipart\/form-data/i.test(clone.headers.get('content-type')||'')?await new Response(text,{headers:{'Content-Type':clone.headers.get('content-type')}}).formData():text;
   }
   return await info(request?request.url:String(input),init?.method||request?.method||'GET',body);
  }catch(_){return null;}})();
  // The original fetch runs immediately and its response is returned without
  // waiting for request parsing, a body clone, or capture publication.
  return originalFetch.call(this,input,init).then(response=>{
   if(response.ok&&alive()){try{const clone=response.clone();void details.then(async request=>{if(!request||!alive())return;const text=await boundedText(clone,524288);if(text!==null){try{publish(request,JSON.parse(text));}catch(_){}}}).catch(()=>{});}catch(_){}}
   return response;
  });
 };
 const openHook=function(method,url,...rest){this.__sleufbaseNativeCapture={method,url};return originalOpen.call(this,method,url,...rest);};
 const sendHook=function(body){const xhr=this;const request=xhr.__sleufbaseNativeCapture;
  const pending=request?info(request.url,request.method,body):Promise.resolve(null);
  if(request)xhr.addEventListener('load',()=>{if(xhr.status<200||xhr.status>=300||!alive())return;pending.then(details=>{try{const text=xhr.responseType==='json'?JSON.stringify(xhr.response):xhr.responseText;if(text.length<=524288)publish(details,JSON.parse(text));}catch(_){}});},{once:true});
  return originalSend.call(this,body);
 };
 state.cleanup=()=>{if(window[key]!==state)return;if(window.fetch===fetchHook)window.fetch=originalFetch;if(XMLHttpRequest.prototype.open===openHook)XMLHttpRequest.prototype.open=originalOpen;if(XMLHttpRequest.prototype.send===sendHook)XMLHttpRequest.prototype.send=originalSend;state.captured=null;delete window[key];};
 window[key]=state;window.fetch=fetchHook;XMLHttpRequest.prototype.open=openHook;XMLHttpRequest.prototype.send=sendHook;
 setTimeout(state.cleanup,120000);return true;
})
