(async function (fields) {
  if (!['https://www.my.kickthemap.com', 'https://my.kickthemap.com'].includes(location.origin)) throw new Error('Unexpected KickTheMap page origin');
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 14000);
  const limit = 256 * 1024;
  const success = value => value === true || (typeof value === 'number' && value !== 0) || (typeof value === 'string' && value.trim().toLowerCase() === 'true');
  class LimitError extends Error {}
  async function read(response) {
    if (Number(response.headers.get('content-length') || 0) > limit) {
      void response.body?.cancel().catch(() => {});
      throw new LimitError('Download-link response too large');
    }
    const reader = response.body?.getReader();
    if (!reader) throw new Error('Missing download-link response');
    const decoder = new TextDecoder();
    const chunks = [];
    let bytes = 0;
    try {
      while (true) {
        controller.signal.throwIfAborted();
        const { value, done } = await reader.read();
        if (done) break;
        bytes += value.byteLength;
        if (bytes > limit || chunks.length >= 2048) throw new LimitError('Download-link response too large');
        chunks.push(decoder.decode(value, { stream: true }));
      }
      controller.signal.throwIfAborted();
      chunks.push(decoder.decode());
      return JSON.parse(chunks.join(''));
    } finally {
      void reader.cancel().catch(() => {});
      reader.releaseLock();
    }
  }
  try {
    const csrf = document.querySelector('meta[name="csrf-token"]')?.content || '';
    for (const kind of ['multipart', 'form', 'json']) {
      controller.signal.throwIfAborted();
      const headers = { Accept: 'application/json', 'X-CSRF-TOKEN': csrf, 'X-Requested-With': 'XMLHttpRequest' };
      let body;
      if (kind === 'multipart') {
        body = new FormData();
        for (const [key, value] of Object.entries(fields)) body.append(key, value);
      } else if (kind === 'form') {
        body = new URLSearchParams(fields);
        headers['Content-Type'] = 'application/x-www-form-urlencoded';
      } else {
        body = JSON.stringify(fields);
        headers['Content-Type'] = 'application/json';
      }
      try {
        const response = await fetch('/jobs/get-file-url', {
          method: 'POST', credentials: 'same-origin', headers, body, signal: controller.signal
        });
        if (!response.ok) { void response.body?.cancel().catch(() => {}); continue; }
        const result = await read(response);
        const data = result?.data;
        const url = typeof data === 'string' ? data : (data?.url || data?.downloadUrl || data?.download_url || result?.url);
        if (success(result?.status) && typeof url === 'string' && url.length > 0) return url;
      } catch (error) {
        if (controller.signal.aborted || error instanceof LimitError) throw error;
      }
    }
    throw new Error('KickTheMap returned no signed URL');
  } finally {
    clearTimeout(timer);
  }
})
