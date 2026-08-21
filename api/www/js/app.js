import { API } from './api.js';
import { setLanguage } from './i18n.js';
import './components/toast.js';
import { initDynamicEngines } from './dynamic_engines.js';

let selectedOtaFile = null;

window.switchDisplayTab = function(tabId) {
  // Update buttons
  document.querySelectorAll('#page-display .tab-btn').forEach(btn => {
    btn.classList.remove('active');
  });
  event.currentTarget.classList.add('active');

  // Update panes
  document.querySelectorAll('#page-display .tab-pane').forEach(pane => {
    pane.classList.remove('active');
  });
  document.getElementById('tab-' + tabId).classList.add('active');
};

window.switchSystemTab = function(tabId) {
  // Update buttons
  document.querySelectorAll('#page-system .tab-btn').forEach(btn => {
    btn.classList.remove('active');
  });
  event.currentTarget.classList.add('active');

  // Update panes
  document.querySelectorAll('#page-system .tab-pane').forEach(pane => {
    pane.classList.remove('active');
  });
  document.getElementById('sys-tab-' + tabId).classList.add('active');
};

document.addEventListener('DOMContentLoaded', () => {
  initNavigation();
  initDashboard();
  initOta();
  loadVersion();
  initCustomMarquee();
  initNetworkSettings();
  initSettings();
  initDynamicEngines();
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
    const defaultLang = navigator.language.split('-')[0];
    const initialLang = ['en', 'fr', 'es'].includes(defaultLang) ? defaultLang : 'en';
    langSelect.value = initialLang;
    setLanguage(initialLang);
    langSelect.addEventListener('change', (e) => setLanguage(e.target.value));
  }
}

async function loadVersion() {
  try {
    const data = await API.get('/api/stats').catch(() => ({version: '2.0.0'}));
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

  const btnReboot = document.getElementById('btn-reboot');
  if (btnReboot) {
    btnReboot.addEventListener('click', async () => {
      if (confirm('Are you sure you want to reboot the system?')) {
        try {
          await API.post('/api/system/reboot');
          window.showToast('Rebooting system...', 'info');
        } catch (e) {
          window.showToast('Failed to reboot system', 'error');
        }
      }
    });
  }

  const btnShutdown = document.getElementById('btn-shutdown');
  if (btnShutdown) {
    btnShutdown.addEventListener('click', async () => {
      if (confirm('Are you sure you want to shutdown the system?')) {
        try {
          await API.post('/api/system/shutdown');
          window.showToast('Shutting down system...', 'info');
        } catch (e) {
          window.showToast('Failed to shutdown system', 'error');
        }
      }
    });
  }

  // System Info Polling
  setInterval(async () => {
    try {
      const info = await API.get('/api/stats');
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
    const cbCustomAuth = document.getElementById('hw-mqtt-custom-auth');
    const authFields = document.getElementById('mqtt-custom-auth-fields');
    if (cbCustomAuth && authFields) {
      cbCustomAuth.addEventListener('change', () => {
        authFields.style.display = cbCustomAuth.checked ? 'grid' : 'none';
      });
    }

    btnInstallMqtt.addEventListener('click', async () => {
      const ip = document.getElementById('hw-mqtt-ip').value;
      if (!ip) {
        window.showToast('Please enter Console IP Address', 'error');
        return;
      }
      
      let user = null;
      let pass = null;
      if (cbCustomAuth && cbCustomAuth.checked) {
        user = document.getElementById('hw-mqtt-ssh-user').value || 'root';
        pass = document.getElementById('hw-mqtt-ssh-pass').value;
      }

      btnInstallMqtt.disabled = true;
      btnInstallMqtt.textContent = 'Installing...';
      try {
        await API.postAuth('/api/mqtt/install', { ip, user, pass });
        window.showToast('MQTT Sync Script Installed!', 'success');
      } catch (e) {
        window.showToast('Failed to install script', 'error');
      } finally {
        btnInstallMqtt.disabled = false;
        btnInstallMqtt.textContent = 'Install via SSH';
      }
    });

    const btnCheckLogs = document.getElementById('btn-check-logs');
    if (btnCheckLogs) {
      btnCheckLogs.addEventListener('click', async () => {
        const ip = document.getElementById('hw-mqtt-ip').value;
        if (!ip) {
          window.showToast('Please enter Console IP Address', 'error');
          return;
        }

        let user = null;
        let pass = null;
        if (cbCustomAuth && cbCustomAuth.checked) {
          user = document.getElementById('hw-mqtt-ssh-user').value || 'root';
          pass = document.getElementById('hw-mqtt-ssh-pass').value;
        }

        btnCheckLogs.disabled = true;
        btnCheckLogs.textContent = 'Checking...';
        const logOutput = document.getElementById('ssh-log-output');
        if (logOutput) {
          logOutput.style.display = 'block';
          logOutput.textContent = 'Fetching logs from ' + ip + '...';
        }
        
        try {
          const res = await API.postAuth('/api/mqtt/logs', { ip, user, pass });
          if (logOutput) {
            logOutput.textContent = res.logs || 'No logs returned.';
          }
          window.showToast('Logs retrieved!', 'success');
        } catch (e) {
          if (logOutput) {
            logOutput.textContent = 'Failed to fetch logs: ' + (e.message || e.status || '');
          }
          window.showToast('Failed to fetch logs', 'error');
        } finally {
          btnCheckLogs.disabled = false;
          btnCheckLogs.textContent = 'Check Logs';
        }
      });
    }
  }

  const btnSaveMqtt = document.getElementById('btn-save-mqtt');
  if (btnSaveMqtt) {
    btnSaveMqtt.addEventListener('click', async () => {
      const mqtt_enable = document.getElementById('hw-mqtt-enable').value === '1';
      const mqtt_broker = document.getElementById('hw-mqtt-broker').value;
      const mqtt_port = parseInt(document.getElementById('hw-mqtt-port').value, 10);
      const mqtt_user = document.getElementById('hw-mqtt-user').value;
      const mqtt_pass = document.getElementById('hw-mqtt-pass').value;
      try {
        await API.post('/api/system', { mqtt_enable, mqtt_broker, mqtt_port, mqtt_user, mqtt_pass });
        window.showToast('MQTT Settings Saved!', 'success');
      } catch (e) {
        window.showToast('Failed to save MQTT Settings', 'error');
      }
    });
  }

  const btnSaveApi = document.getElementById('btn-save-api');
  if (btnSaveApi) {
    btnSaveApi.addEventListener('click', async () => {
      const api_auth_enabled = document.getElementById('hw-api-auth').value === 'true';
      const api_token = document.getElementById('hw-api-token').value;
      try {
        await API.post('/api/system', { api_auth_enabled, api_token });
        window.showToast('API Auth Settings Saved!', 'success');
      } catch (e) {
        window.showToast('Failed to save API Auth Settings', 'error');
      }
    });
  }
}

async function initSettings() {
  // Load current settings
  try {
    const s = await API.get('/api/system').then(s => ({ brightness_limit: s.system.night_brightness || 50, matrix_brightness: 50, ...s.system, ...s.mqtt, ...s.wifi }));
    
    // Dashboard
    const sliderBright = document.getElementById('slider-brightness');
    if (sliderBright) {
      sliderBright.value = s.brightness_limit;
      document.getElementById('val-brightness').textContent = s.brightness_limit;
      sliderBright.addEventListener('input', (e) => {
        document.getElementById('val-brightness').textContent = e.target.value;
      });
      sliderBright.addEventListener('change', async (e) => {
        await API.post('/api/system', { brightness_limit: parseInt(e.target.value) });
        window.showToast('Brightness updated', 'info');
      });
    }

    const sliderNightBright = document.getElementById('cfg-night-brightness');
    if (sliderNightBright) {
      sliderNightBright.addEventListener('input', (e) => {
        document.getElementById('night-brightness-val').textContent = e.target.value + '%';
      });
    }

    // Hardware
    document.getElementById('hw-rows').value = s.matrix_rows;
    document.getElementById('hw-cols').value = s.matrix_cols;
    document.getElementById('hw-chain').value = s.matrix_chain;
    document.getElementById('hw-parallel').value = s.matrix_parallel;
    if (document.getElementById('hw-driver-chip')) document.getElementById('hw-driver-chip').value = s.matrix_driver_chip || 'SHIFTREG';
    if (document.getElementById('hw-row-addr-type')) document.getElementById('hw-row-addr-type').value = s.matrix_row_addr_type || 0;
    if (document.getElementById('hw-multiplexing')) document.getElementById('hw-multiplexing').value = s.matrix_multiplexing || 0;
    document.getElementById('hw-mapping').value = s.matrix_mapping || 'regular';
    document.getElementById('hw-rgb').value = s.matrix_rgb_sequence || 'RGB';
    document.getElementById('hw-slowdown').value = s.matrix_slowdown;
    document.getElementById('hw-pwm-bits').value = s.matrix_pwm_bits;
    document.getElementById('hw-pwm-lsb').value = s.matrix_pwm_lsb_nanoseconds;
    document.getElementById('hw-disable-pulsing').checked = s.matrix_disable_hardware_pulsing;
    document.getElementById('hw-limit-refresh').value = s.matrix_limit_refresh_rate_hz;
    
    // MQTT
    document.getElementById('hw-mqtt-enable').value = s.mqtt_enabled ? '1' : '0';
    document.getElementById('hw-mqtt-broker').value = s.mqtt_broker;
    document.getElementById('hw-mqtt-port').value = s.mqtt_port;
    document.getElementById('hw-mqtt-user').value = s.mqtt_user || '';
    document.getElementById('hw-mqtt-pass').value = s.mqtt_pass || '';

    // API
    document.getElementById('hw-api-auth').value = s.api_auth_enabled ? 'true' : 'false';
    document.getElementById('hw-api-token').value = s.api_token || '';


  } catch (e) {
    console.error('Failed to load settings', e);
  }


  // Save HW settings
  const btnSaveHw = document.getElementById('btn-save-hw');
  if (btnSaveHw) {
    btnSaveHw.addEventListener('click', async () => {
      const rows = parseInt(document.getElementById('hw-rows').value) || 32;
      const cols = parseInt(document.getElementById('hw-cols').value) || 64;
      const chain = parseInt(document.getElementById('hw-chain').value) || 1;
      const parallel = parseInt(document.getElementById('hw-parallel').value) || 1;
      const driverChip = document.getElementById('hw-driver-chip')?.value || 'SHIFTREG';
      const rowAddrType = parseInt(document.getElementById('hw-row-addr-type')?.value) || 0;
      const multiplexing = parseInt(document.getElementById('hw-multiplexing')?.value) || 0;
      const mapping = document.getElementById('hw-mapping').value || 'regular';
      const rgb = document.getElementById('hw-rgb').value || 'RGB';
      const slowdown = parseInt(document.getElementById('hw-slowdown').value) || 2;
      const pwmBits = parseInt(document.getElementById('hw-pwm-bits').value) || 11;
      const pwmLsb = parseInt(document.getElementById('hw-pwm-lsb').value) || 130;
      const disablePulsing = document.getElementById('hw-disable-pulsing').checked;
      const limitRefresh = parseInt(document.getElementById('hw-limit-refresh').value) || 120;
      
      try {
        await API.post('/api/system', { 
          matrix_rows: rows,
          matrix_cols: cols,
          matrix_chain: chain,
          matrix_parallel: parallel,
          matrix_driver_chip: driverChip,
          matrix_row_addr_type: rowAddrType,
          matrix_multiplexing: multiplexing,
          matrix_mapping: mapping,
          matrix_rgb_sequence: rgb,
          matrix_slowdown: slowdown,
          matrix_pwm_bits: pwmBits,
          matrix_pwm_lsb_nanoseconds: pwmLsb,
          matrix_disable_hardware_pulsing: disablePulsing,
          matrix_limit_refresh_rate_hz: limitRefresh
        });
        window.showToast('Hardware settings saved! Please Reboot for changes to take effect.', 'success');
        
      } catch (e) {
        window.showToast('Failed to save HW settings', 'error');
      }
    });
  }

  
        window.showToast('Message sent!', 'success');
      } catch (e) {
        window.showToast('Failed to send message', 'error');
      }
    });
  }
}


