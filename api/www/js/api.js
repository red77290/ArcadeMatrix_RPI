function authHeaders(base = {}) {
  const headers = { ...base };
  const token = localStorage.getItem('api_token');
  if (token) headers['X-API-Token'] = token;
  return headers;
}

export const API = {
  async get(url) {
    const res = await fetch(url, { headers: authHeaders() });
    if (!res.ok) throw new Error(`HTTP Error ${res.status}`);
    return await res.json();
  },

  async post(url, data) {
    const res = await fetch(url, {
      method: 'POST',
      headers: authHeaders({ 'Content-Type': 'application/json' }),
      body: data ? JSON.stringify(data) : undefined,
    });
    if (!res.ok) throw new Error(`HTTP Error ${res.status}`);
    return await res.json();
  },

  async postAuth(url, data) {
    return this.post(url, data);
  },

  async delete(url) {
    const res = await fetch(url, { method: 'DELETE', headers: authHeaders() });
    if (!res.ok) throw new Error(`HTTP Error ${res.status}`);
    return await res.json();
  },

  async uploadMarquee(file) {
    const formData = new FormData();
    formData.append('image', file);
    const res = await fetch('/api/marquee', {
      method: 'POST',
      headers: authHeaders(),
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
      const token = localStorage.getItem('api_token');
      if (token) xhr.setRequestHeader('X-API-Token', token);
      xhr.send(formData);
    });
  },

  async checkOtaUpdate() {
    return this.get('/api/ota/check?_t=' + Date.now());
  },

  async triggerAutoOta(downloadUrl = null) {
    return this.post('/api/ota/auto-update', { download_url: downloadUrl });
  },

  async getVersion() {
    return this.get('/api/version?_t=' + Date.now()).catch(() => this.get('/api/stats?_t=' + Date.now())).catch(() => ({ version: '--' }));
  },

  async waitForReconnect(timeoutMs = 60000, intervalMs = 2000, expectedVersion = null) {
    const start = Date.now();
    let wentOffline = false;

    // Grace period for the daemon to receive the update command and initiate shutdown
    await new Promise(r => setTimeout(r, 2500));

    while (Date.now() - start < timeoutMs) {
      try {
        const res = await fetch('/api/version?_t=' + Date.now(), { cache: 'no-store', signal: AbortSignal.timeout(1500) });
        if (res.ok) {
          const data = await res.json();
          // If we expected a new version, make sure it didn't just return the old un-restarted daemon
          if (expectedVersion && data.version === expectedVersion && !wentOffline && (Date.now() - start < 8000)) {
            await new Promise(r => setTimeout(r, intervalMs));
            continue;
          }
          return data;
        }
      } catch {
        wentOffline = true;
      }
      await new Promise(r => setTimeout(r, intervalMs));
    }
    throw new Error('Device did not respond after reboot');
  }
};
