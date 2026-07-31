import { API } from './api.js';
import { setLanguage } from './i18n.js';
import './components/toast.js';

let selectedOtaFile = null;

document.addEventListener('DOMContentLoaded', () => {
  initNavigation();
  initDashboard();
  initOta();
  loadVersion();
  initCustomMarquee();
  initNetworkSettings();
});

function initNavigation() {
  document.querySelectorAll('.nav-item').forEach(item => {
    item.addEventListener('click', () => {
      document.querySelectorAll('.nav-item').forEach(i => i.classList.remove('active'));
      document.querySelectorAll('.page').forEach(p => p.classList.remove('active'));

      item.classList.add('active');
      const pageId = `page-${item.getAttribute('data-page')}`;
      const pageEl = document.getElementById(pageId);
      if (pageEl) pageEl.classList.add('active');
    });
  });

  const langSelect = document.getElementById('lang-selector');
  if (langSelect) {
    langSelect.addEventListener('change', (e) => setLanguage(e.target.value));
  }
}

async function loadVersion() {
  try {
    const data = await API.get('/api/version');
    const versionTag = document.getElementById('version-tag');
    const otaVer = document.getElementById('ota-current-version');
    const otaArch = document.getElementById('ota-current-arch');

    if (versionTag) versionTag.textContent = `v${data.version}`;
    if (otaVer) otaVer.textContent = `v${data.version}`;
    if (otaArch) otaArch.textContent = data.arch || 'aarch64';
  } catch (e) {
    console.error('Failed to load version:', e);
  }
}

function initDashboard() {
  const btnPower = document.getElementById('btn-power-toggle');
  if (btnPower) {
    btnPower.addEventListener('click', async () => {
      const isOff = btnPower.classList.contains('btn-power-off');
      const newState = isOff;
      try {
        await API.post('/api/system/power', { state: newState });
        btnPower.textContent = newState ? 'Turn Panel OFF' : 'Turn Panel ON';
        btnPower.className = `btn ${newState ? 'btn-power-on' : 'btn-power-off'}`;
        window.showToast(newState ? 'Panel powered ON' : 'Panel powered OFF', 'info');
      } catch (e) {
        window.showToast('Failed to toggle power', 'error');
      }
    });
  }

  // System Info Polling
  setInterval(async () => {
    try {
      const info = await API.get('/api/system_info');
      document.getElementById('metric-cpu').textContent = `${info.cpu_load.toFixed(1)} %`;
      document.getElementById('metric-temp').textContent = `${info.temperature_c.toFixed(1)} °C`;
      document.getElementById('metric-ram').textContent = `${info.ram_used_mb} MB`;
      document.getElementById('metric-disk').textContent = `${info.disk_free_gb} GB`;
    } catch {}
  }, 4000);
}

function initOta() {
  const dropZone = document.getElementById('ota-drop-zone');
  const fileInput = document.getElementById('ota-file-input');
  const btnUpdate = document.getElementById('btn-ota-update');
  const fileDetails = document.getElementById('ota-file-details');
  const fileNameEl = document.getElementById('ota-filename');
  const fileSizeEl = document.getElementById('ota-filesize');
  const progressContainer = document.getElementById('ota-progress-container');
  const progressFill = document.getElementById('ota-progress-fill');
  const progressText = document.getElementById('ota-progress-text');

  if (!dropZone || !fileInput || !btnUpdate) return;

  dropZone.addEventListener('click', () => fileInput.click());

  dropZone.addEventListener('dragover', (e) => {
    e.preventDefault();
    dropZone.classList.add('drag-over');
  });

  dropZone.addEventListener('dragleave', () => dropZone.classList.remove('drag-over'));

  dropZone.addEventListener('drop', (e) => {
    e.preventDefault();
    dropZone.classList.remove('drag-over');
    if (e.dataTransfer.files.length > 0) {
      handleFileSelected(e.dataTransfer.files[0]);
    }
  });

  fileInput.addEventListener('change', () => {
    if (fileInput.files.length > 0) {
      handleFileSelected(fileInput.files[0]);
    }
  });

  function handleFileSelected(file) {
    selectedOtaFile = file;
    fileNameEl.textContent = file.name;
    fileSizeEl.textContent = `${(file.size / (1024 * 1024)).toFixed(2)} MB`;
    fileDetails.style.display = 'block';
    btnUpdate.disabled = false;
  }

  btnUpdate.addEventListener('click', async () => {
    if (!selectedOtaFile) return;

    if (!confirm('⚠️ This will replace the running firmware binary and restart the device. Continue?')) {
      return;
    }

    btnUpdate.disabled = true;
    progressContainer.style.display = 'block';
    progressFill.style.width = '0%';
    progressText.textContent = '0%';

    try {
      window.showToast('Uploading firmware binary...', 'info');
      await API.uploadFirmware(selectedOtaFile, (percent) => {
        progressFill.style.width = `${percent}%`;
        progressText.textContent = `${percent}%`;
      });

      progressText.textContent = 'Restarting daemon...';
      window.showToast('Firmware replaced! Reconnecting to device...', 'info');

      const versionData = await API.waitForReconnect();
      window.showToast(`✅ Firmware updated to v${versionData.version}!`, 'success');
      loadVersion();
    } catch (err) {
      window.showToast(`❌ Update failed: ${err.message || err.status}`, 'error');
    } finally {
      btnUpdate.disabled = false;
      progressContainer.style.display = 'none';
    }
  });
}

function initCustomMarquee() {
  const dropZone = document.getElementById('marquee-drop-zone');
  const fileInput = document.getElementById('marquee-file-input');
  if (!dropZone || !fileInput) return;

  dropZone.addEventListener('click', () => fileInput.click());
  dropZone.addEventListener('dragover', (e) => {
    e.preventDefault();
    dropZone.classList.add('drag-over');
  });
  dropZone.addEventListener('dragleave', () => dropZone.classList.remove('drag-over'));
  dropZone.addEventListener('drop', (e) => {
    e.preventDefault();
    dropZone.classList.remove('drag-over');
    if (e.dataTransfer.files.length > 0) {
      uploadMarqueeImage(e.dataTransfer.files[0]);
    }
  });
  fileInput.addEventListener('change', () => {
    if (fileInput.files.length > 0) {
      uploadMarqueeImage(fileInput.files[0]);
    }
  });

  async function uploadMarqueeImage(file) {
    window.showToast('Uploading marquee image...', 'info');
    try {
      await API.uploadMarquee(file);
      window.showToast('Marquee image displayed!', 'success');
    } catch (e) {
      window.showToast('Failed to display marquee image', 'error');
    }
  }
}

function initNetworkSettings() {
  const btnSaveWifi = document.getElementById('btn-save-wifi');
  if (btnSaveWifi) {
    btnSaveWifi.addEventListener('click', async () => {
      const ssid = document.getElementById('hw-wifi-ssid').value;
      const pass = document.getElementById('hw-wifi-pass').value;
      if (!ssid || !pass) {
        window.showToast('Please enter both SSID and Password', 'error');
        return;
      }
      btnSaveWifi.disabled = true;
      btnSaveWifi.textContent = 'Connecting...';
      try {
        await API.postAuth('/api/wifi', { ssid, password: pass });
        window.showToast('Connected to Wi-Fi!', 'success');
      } catch (e) {
        window.showToast('Failed to connect to Wi-Fi', 'error');
      } finally {
        btnSaveWifi.disabled = false;
        btnSaveWifi.textContent = 'Connect Wi-Fi';
      }
    });
  }

  const btnInstallMqtt = document.getElementById('btn-install-mqtt');
  if (btnInstallMqtt) {
    btnInstallMqtt.addEventListener('click', async () => {
      const ip = document.getElementById('hw-mqtt-ip').value;
      if (!ip) {
        window.showToast('Please enter Console IP Address', 'error');
        return;
      }
      btnInstallMqtt.disabled = true;
      btnInstallMqtt.textContent = 'Installing...';
      try {
        await API.postAuth('/api/mqtt/install', { ip });
        window.showToast('MQTT Sync Script Installed!', 'success');
      } catch (e) {
        window.showToast('Failed to install script', 'error');
      } finally {
        btnInstallMqtt.disabled = false;
        btnInstallMqtt.textContent = 'Install via SSH';
      }
    });
  }

  const btnSaveMqtt = document.getElementById('btn-save-mqtt');
  if (btnSaveMqtt) {
    btnSaveMqtt.addEventListener('click', async () => {
      const mqtt_enable = document.getElementById('hw-mqtt-enable').value === '1';
      const mqtt_broker = document.getElementById('hw-mqtt-broker').value;
      const mqtt_port = parseInt(document.getElementById('hw-mqtt-port').value, 10);
      try {
        await API.post('/api/settings', { mqtt_enable, mqtt_broker, mqtt_port });
        window.showToast('MQTT Settings Saved!', 'success');
      } catch (e) {
        window.showToast('Failed to save MQTT Settings', 'error');
      }
    });
  }
}

