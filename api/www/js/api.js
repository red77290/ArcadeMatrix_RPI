export const API = {
  async get(url) {
    const res = await fetch(url);
    if (!res.ok) throw new Error(`HTTP Error ${res.status}`);
    return await res.json();
  },

  async post(url, data) {
    const res = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(data),
    });
    if (!res.ok) throw new Error(`HTTP Error ${res.status}`);
    return await res.json();
  },

  async postAuth(url, data) {
    const headers = { 'Content-Type': 'application/json' };
    const token = localStorage.getItem('api_token');
    if (token) headers['X-API-Token'] = token;
    
    const res = await fetch(url, {
      method: 'POST',
      headers,
      body: data ? JSON.stringify(data) : undefined,
    });
    if (!res.ok) throw new Error(`HTTP Error ${res.status}`);
    return await res.json();
  },

  async uploadMarquee(file) {
    const formData = new FormData();
    formData.append('image', file);
    const res = await fetch('/api/marquee', {
      method: 'POST',
      body: formData
    });
    if (!res.ok) throw new Error(`HTTP Error ${res.status}`);
    return await res.json();
  },

  uploadFirmware(file, onProgress) {
    return new Promise((resolve, reject) => {
      const xhr = new XMLHttpRequest();
      const formData = new FormData();
      formData.append('firmware', file);

      xhr.upload.onprogress = (e) => {
        if (e.lengthComputable) {
          const percent = Math.round((e.loaded / e.total) * 100);
          onProgress(percent);
        }
      };

      xhr.onload = () => {
        if (xhr.status === 200) {
          try {
            resolve(JSON.parse(xhr.responseText));
          } catch {
            resolve({ status: 'success' });
          }
        } else {
          try {
            reject(JSON.parse(xhr.responseText));
          } catch {
            reject({ message: `HTTP Error ${xhr.status}` });
          }
        }
      };

      xhr.onerror = () => reject({ message: 'Network connection error' });
      xhr.open('POST', '/api/update');
      xhr.send(formData);
    });
  },

  async waitForReconnect(timeoutMs = 60000, intervalMs = 2000) {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
      try {
        const res = await fetch('/api/version', { signal: AbortSignal.timeout(1500) });
        if (res.ok) return await res.json();
      } catch {}
      await new Promise(r => setTimeout(r, intervalMs));
    }
    throw new Error('Device did not respond after reboot');
  }
};
