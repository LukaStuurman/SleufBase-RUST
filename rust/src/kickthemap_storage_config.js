(async function () {
  if (!['https://www.my.kickthemap.com', 'https://my.kickthemap.com'].includes(location.origin)) throw new Error('Unexpected KickTheMap page origin');
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 14000);
  let reader, response;
  try {
    const csrf = document.querySelector('meta[name="csrf-token"]')?.content || '';
    response = await fetch('/main/adata', {method:'POST', credentials:'same-origin', headers:{'Accept':'application/json','X-CSRF-TOKEN':csrf},signal:controller.signal});
    if (!response.ok) throw new Error('Storage configuration HTTP ' + response.status);
    if (Number(response.headers.get('content-length') || 0) > 65536) throw new Error('Storage configuration too large');
    reader=response.body?.getReader();if(!reader) throw new Error('Missing storage configuration');
    let bytes=0;const chunks=[];const decoder=new TextDecoder();
    while(true){controller.signal.throwIfAborted();const {value,done}=await reader.read();if(done)break;bytes+=value.byteLength;if(bytes>65536||chunks.length>=1024)throw new Error('Storage configuration too large');chunks.push(decoder.decode(value,{stream:true}));}
    controller.signal.throwIfAborted();chunks.push(decoder.decode());return chunks.join('');
  } finally {clearTimeout(timer);if(reader){void reader.cancel().catch(()=>{});reader.releaseLock();}else{void response?.body?.cancel().catch(()=>{});}}
})
