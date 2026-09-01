import { API } from './api.js';
import { setLanguage, SUPPORTED_LANGUAGES } from './i18n.js';
import './components/toast.js';
import { initDynamicEngines, GLOBAL_TIMEZONES } from './dynamic_engines.js';

let selectedOtaFile = null;

window.switchTab = function(tabId) {
  if (window.event && window.event.currentTarget) {
    document.querySelectorAll('#page-display .tab-btn').forEach(btn => {
      btn.classList.remove('active');
    });
    window.event.currentTarget.classList.add('active');
  }

  // Update panes
  document.querySelectorAll('#page-display .tab-pane').forEach(pane => {
    pane.classList.remove('active');
  });
  const pane = document.getElementById('tab-' + tabId);
  if (pane) pane.classList.add('active');
};
window.switchDisplayTab = window.switchTab;

window.switchSystemTab = function(tabId) {
  // Update buttons
  if (window.event && window.event.currentTarget) {
    document.querySelectorAll('#page-system .tab-btn').forEach(btn => {
      btn.classList.remove('active');
    });
    window.event.currentTarget.classList.add('active');
  }

  // Update panes
  document.querySelectorAll('#page-system .tab-pane').forEach(pane => {
    pane.classList.remove('active');
  });
  const pane = document.getElementById('sys-tab-' + tabId);
  if (pane) pane.classList.add('active');
};

document.addEventListener('DOMContentLoaded', () => {
  initNavigation();
  initDashboard();
  initOta();
  loadVersion();
  initCustomMarquee();
  initFighterOverlay();
  initNetworkSettings();
  initSettings();
  initDynamicEngines();
});

export function switchPage(pageName) {
  document.querySelectorAll('.nav-item').forEach(i => i.classList.remove('active'));
  document.querySelectorAll('.page').forEach(p => p.classList.remove('active'));

  const navItem = document.querySelector(`.nav-item[data-page="${pageName}"]`);
  const pageEl = document.getElementById(`page-${pageName}`);
  if (navItem) navItem.classList.add('active');
  if (pageEl) pageEl.classList.add('active');
  localStorage.setItem('activePage', pageName);
}
window.switchPage = switchPage;

function initNavigation() {
  document.querySelectorAll('.nav-item').forEach(item => {
    item.addEventListener('click', () => {
      const page = item.getAttribute('data-page');
      switchPage(page);
    });
  });

  const savedPage = localStorage.getItem('activePage') || 'dashboard';
  switchPage(savedPage);

  const langSelect = document.getElementById('lang-selector');
  if (langSelect) {
    langSelect.innerHTML = '';
    SUPPORTED_LANGUAGES.forEach(l => {
      const opt = document.createElement('option');
      opt.value = l.code;
      opt.textContent = l.label;
      langSelect.appendChild(opt);
    });

    const savedLang = localStorage.getItem('lang');
    const defaultLang = navigator.language.split('-')[0];
    const initialLang = savedLang || (SUPPORTED_LANGUAGES.some(l => l.code === defaultLang) ? defaultLang : 'fr');
    langSelect.value = initialLang;
    setLanguage(initialLang);

    langSelect.addEventListener('change', async (e) => {
      const selected = e.target.value;
      setLanguage(selected);
      try {
        await API.post('/api/system', { lang: selected });
      } catch (err) {
        console.warn('Failed to sync lang to backend:', err);
      }
    });
  }
}

async function loadVersion() {
  try {
    const data = await API.get('/api/version?_t=' + Date.now()).catch(() => API.get('/api/stats?_t=' + Date.now())).catch(() => ({version: '--', arch: 'aarch64'}));
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

  const btnRestartApp = document.getElementById('btn-restart-app');
  if (btnRestartApp) {
    btnRestartApp.addEventListener('click', async () => {
      if (confirm('Restart the ArcadeMatrix app? The display will go blank for a few seconds.')) {
        try {
          await API.post('/api/system/restart');
          window.showToast('Restarting app...', 'info');
        } catch (e) {
          window.showToast('Failed to restart app', 'error');
        }
      }
    });
  }

  // System Info Polling (10s interval)
  const pollMetrics = async () => {
    try {
      const info = await API.get('/api/stats').catch(() => API.get('/api/system_info')).catch(() => null);
      if (!info) return;

      const cpuEl = document.getElementById('metric-cpu');
      const tempEl = document.getElementById('metric-temp');
      const ramEl = document.getElementById('metric-ram');
      const diskEl = document.getElementById('metric-disk');

      if (cpuEl && info.cpu_load !== undefined) cpuEl.textContent = `${info.cpu_load.toFixed(1)} %`;
      if (tempEl && info.temperature_c !== undefined) tempEl.textContent = `${info.temperature_c.toFixed(1)} °C`;
      if (ramEl) {
        if (info.ram_used_mb !== undefined) ramEl.textContent = `${typeof info.ram_used_mb === 'number' ? info.ram_used_mb.toFixed(1) : info.ram_used_mb} MB`;
        else if (info.free_heap !== undefined) ramEl.textContent = `${(info.free_heap / 1024).toFixed(0)} KB Free`;
      }
      if (diskEl && info.disk_free_gb !== undefined) diskEl.textContent = `${typeof info.disk_free_gb === 'number' ? info.disk_free_gb.toFixed(1) : info.disk_free_gb} GB`;
    } catch {}
  };

  pollMetrics();
  setInterval(pollMetrics, 10000);
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

  // --- 1. Online Automatic OTA (GitHub Releases) ---
  const btnCheck = document.getElementById('btn-ota-check');
  const btnAutoInstall = document.getElementById('btn-ota-auto-install');
  const onlineCard = document.getElementById('ota-online-card');
  const onlineBadge = document.getElementById('ota-online-badge');
  const onlineTargetVer = document.getElementById('ota-online-target-ver');
  const onlineSize = document.getElementById('ota-online-size');
  const onlineNotes = document.getElementById('ota-online-notes');
  const autoProgress = document.getElementById('ota-auto-progress');
  const autoProgressText = document.getElementById('ota-auto-progress-text');

  let latestDownloadUrl = null;
  let targetVersionName = '';

  if (btnCheck) {
    btnCheck.addEventListener('click', async () => {
      btnCheck.disabled = true;
      btnCheck.textContent = '⏳ Checking GitHub...';
      window.showToast('Checking GitHub for latest release...', 'info');

      try {
        const data = await API.checkOtaUpdate();
        if (data.status === 'error') {
          window.showToast(`❌ ${data.message}`, 'error');
          return;
        }

        onlineCard.style.display = 'block';
        targetVersionName = data.latest_version;
        onlineTargetVer.textContent = `v${data.latest_version}`;

        if (data.asset_size) {
          onlineSize.textContent = `${(data.asset_size / (1024 * 1024)).toFixed(1)} MB (${data.current_arch})`;
        } else {
          onlineSize.textContent = data.current_arch;
        }

        if (data.release_notes) {
          onlineNotes.textContent = data.release_notes;
          onlineNotes.style.display = 'block';
        } else {
          onlineNotes.style.display = 'none';
        }

        if (data.update_available && data.download_url) {
          latestDownloadUrl = data.download_url;
          onlineBadge.className = 'badge badge-accent';
          onlineBadge.textContent = 'New Update Available';
          btnAutoInstall.style.display = 'inline-block';
          btnAutoInstall.textContent = `⬇️ Download & Install v${data.latest_version}`;
          window.showToast(`🎉 New update v${data.latest_version} available!`, 'success');
        } else {
          latestDownloadUrl = null;
          onlineBadge.className = 'badge';
          onlineBadge.textContent = 'Up to Date';
          btnAutoInstall.style.display = 'none';
          window.showToast(`✅ You are already on the latest version (v${data.current_version}).`, 'info');
        }
      } catch (err) {
        window.showToast(`❌ Failed to check updates: ${err.message || err.status}`, 'error');
      } finally {
        btnCheck.disabled = false;
        btnCheck.textContent = '🔍 Check for Updates';
      }
    });
  }

  if (btnAutoInstall) {
    btnAutoInstall.addEventListener('click', async () => {
      if (!confirm(`⚠️ Download and install ArcadeMatrix v${targetVersionName || ''} from GitHub? The service will automatically restart.`)) {
        return;
      }

      btnCheck.disabled = true;
      btnAutoInstall.disabled = true;
      autoProgress.style.display = 'block';
      autoProgressText.textContent = 'Downloading binary from GitHub & verifying...';
      window.showToast('Downloading firmware from GitHub...', 'info');

      try {
        const oldVersionData = await API.getVersion().catch(() => null);
        const oldVer = oldVersionData?.version || '';

        const resp = await API.triggerAutoOta(latestDownloadUrl);
        if (resp.status === 'error') {
          throw new Error(resp.message || 'Auto-update failed');
        }

        autoProgressText.textContent = 'Restarting daemon...';
        window.showToast('Firmware downloaded & replaced! Waiting for restart...', 'info');

        const versionData = await API.waitForReconnect(60000, 2000, oldVer);
        if (oldVer && versionData.version === oldVer) {
          window.showToast(`⚠️ Device reconnected, but version is still v${versionData.version}. (Check /tmp/arcadematrix_ota.log)`, 'warning');
        } else {
          window.showToast(`✅ Firmware successfully updated to v${versionData.version}!`, 'success');
        }
        loadVersion();
        if (onlineCard) onlineCard.style.display = 'none';
      } catch (err) {
        window.showToast(`❌ Automatic update failed: ${err.message || err.status}`, 'error');
      } finally {
        btnCheck.disabled = false;
        btnAutoInstall.disabled = false;
        autoProgress.style.display = 'none';
      }
    });
  }

  // --- 2. Manual Offline OTA File Upload ---
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
      const oldVersionData = await API.getVersion().catch(() => null);
      const oldVer = oldVersionData?.version || '';

      window.showToast('Uploading firmware binary...', 'info');
      await API.uploadFirmware(selectedOtaFile, (percent) => {
        progressFill.style.width = `${percent}%`;
        progressText.textContent = `${percent}%`;
      });

      progressText.textContent = 'Restarting daemon...';
      window.showToast('Firmware uploaded! Waiting for daemon to restart...', 'info');

      const versionData = await API.waitForReconnect(60000, 2000, oldVer);
      if (oldVer && versionData.version === oldVer) {
        window.showToast(`⚠️ Device reconnected, but version is still v${versionData.version}. (Check /tmp/arcadematrix_ota.log)`, 'warning');
      } else {
        window.showToast(`✅ Firmware successfully updated to v${versionData.version}!`, 'success');
      }
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

function initFighterOverlay() {
  const enabledEl = document.getElementById('fighter-enabled');
  const intervalEl = document.getElementById('fighter-interval');
  const saveBtn = document.getElementById('btn-save-fighter');
  if (!enabledEl || !intervalEl || !saveBtn) return;

  // Load current values from the canonical system config.
  (async () => {
    try {
      const cfg = await API.get('/api/system');
      const sys = cfg.system || {};
      if (typeof sys.idle_fighter_enabled === 'boolean') {
        enabledEl.checked = sys.idle_fighter_enabled;
      }
      if (typeof sys.idle_fighter_interval === 'number') {
        intervalEl.value = sys.idle_fighter_interval;
      }
    } catch (e) {
      console.error('Failed to load fighter settings', e);
    }
  })();

  saveBtn.addEventListener('click', async () => {
    try {
      await API.post('/api/system', {
        idle_fighter_enabled: enabledEl.checked,
        idle_fighter_interval: parseInt(intervalEl.value) || 1,
      });
      window.showToast('Fighter overlay settings saved!', 'success');
    } catch (e) {
      window.showToast('Failed to save fighter settings', 'error');
    }
  });
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
      const base = (window.__sysConfig && window.__sysConfig.mqtt) ? { ...window.__sysConfig.mqtt } : {};
      const mqtt = {
        ...base,
        enabled: document.getElementById('hw-mqtt-enable').value === '1',
        broker: document.getElementById('hw-mqtt-broker').value,
        port: parseInt(document.getElementById('hw-mqtt-port').value, 10) || 1883,
        user: document.getElementById('hw-mqtt-user').value,
        pass: document.getElementById('hw-mqtt-pass').value,
      };
      try {
        await API.post('/api/system', { mqtt });
        if (window.__sysConfig) window.__sysConfig.mqtt = mqtt;
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
  // Load current settings (canonical nested model straight from config.json).
  try {
    const cfg = await API.get('/api/system');
    window.__sysConfig = cfg; // keep the full config to merge on save
    const sys = cfg.system || {};
    const matrix = cfg.matrix || {};
    const mqtt = cfg.mqtt || {};

    const setVal = (id, v) => { const el = document.getElementById(id); if (el && v !== undefined && v !== null) el.value = v; };
    const setChk = (id, v) => { const el = document.getElementById(id); if (el && v !== undefined && v !== null) el.checked = !!v; };

    if (sys.lang) {
      const ls = document.getElementById('lang-selector');
      if (ls) ls.value = sys.lang;
      setLanguage(sys.lang);
    }

    // Dashboard brightness (0-100) is the live daytime brightness (day_brightness).
    const sliderBright = document.getElementById('slider-brightness');
    if (sliderBright) {
      const bright = sys.day_brightness ?? 100;
      sliderBright.value = bright;
      const valEl = document.getElementById('val-brightness');
      if (valEl) valEl.textContent = bright;
      sliderBright.addEventListener('input', (e) => {
        const v = document.getElementById('val-brightness');
        if (v) v.textContent = e.target.value;
      });
      sliderBright.addEventListener('change', async (e) => {
        await API.post('/api/system', { brightness_limit: parseInt(e.target.value) });
        window.showToast('Brightness updated', 'info');
      });
    }

    // Night Mode
    setChk('cfg-night-mode-enabled', sys.night_mode_enabled);
    setVal('cfg-turn-off-at', sys.turn_off_at || '23:00');
    setVal('cfg-wake-up-at', sys.wake_up_at || '07:00');
    const sliderNightBright = document.getElementById('cfg-night-brightness');
    if (sliderNightBright) {
      const nb = (sys.night_brightness !== undefined) ? sys.night_brightness : 10;
      sliderNightBright.value = nb;
      const v = document.getElementById('night-brightness-val');
      if (v) v.textContent = nb + '%';
      sliderNightBright.addEventListener('input', (e) => {
        const val = document.getElementById('night-brightness-val');
        if (val) val.textContent = e.target.value + '%';
      });
    }

    // Populate Timezone dropdown in General settings tab
    const selTz = document.getElementById('sys-timezone');
    if (selTz && selTz.options.length === 0) {
      GLOBAL_TIMEZONES.filter(tz => tz.value !== 'system').forEach(tz => {
        const opt = document.createElement('option');
        opt.value = tz.value;
        opt.textContent = tz.label;
        selTz.appendChild(opt);
      });
    }

    // General / Regional Preferences
    setVal('sys-timezone', sys.timezone || 'CET-1CEST,M3.5.0,M10.5.0/3');
    setVal('sys-lang', sys.lang || 'en');
    setVal('sys-format-24h', sys.format_24h !== undefined ? String(sys.format_24h) : 'true');
    setVal('sys-unit', sys.temp_unit || 'C');
    setChk('sys-idle-fighter-enabled', sys.idle_fighter_enabled ?? true);
    setVal('sys-idle-fighter-interval', sys.idle_fighter_interval || 10);

    // Raspberry Pi Matrix Hardware (pure RPi HUB75 configuration)
    setVal('hw-rows', matrix.height || 32);
    setVal('hw-cols', matrix.width || 64);
    setVal('hw-chain', matrix.chain_length || 1);
    setVal('hw-parallel', 1);
    setVal('hw-driver-chip', matrix.driver_chip || 'SHIFTREG');
    setVal('hw-mapping', matrix.mapping || 'regular');
    setVal('hw-rgb', matrix.rgb_sequence || 'RGB');
    setVal('hw-row-addr-type', matrix.row_address_mode ?? 0);
    setVal('hw-multiplexing', matrix.multiplexing ?? 0);
    setVal('hw-slowdown', matrix.slowdown ?? 2);
    setVal('hw-pwm-bits', matrix.pwm_bits ?? 11);
    setVal('hw-pwm-lsb', matrix.pwm_lsb_nanoseconds ?? 130);
    setVal('hw-limit-refresh', matrix.limit_refresh_rate_hz ?? 0);
    setChk('hw-disable-pulsing', matrix.disable_hardware_pulsing ?? false);

    // MQTT (nested mqtt.*)
    setVal('hw-mqtt-enable', mqtt.enabled ? '1' : '0');
    setVal('hw-mqtt-broker', mqtt.broker);
    setVal('hw-mqtt-port', mqtt.port);
    setVal('hw-mqtt-user', mqtt.user || '');
    setVal('hw-mqtt-pass', mqtt.pass || '');

    // API auth (top-level)
    setVal('hw-api-auth', cfg.api_auth_enabled ? 'true' : 'false');
    setVal('hw-api-token', cfg.api_token || '');
  } catch (e) {
    console.error('Failed to load settings', e);
  }

  // Save General System Preferences
  const btnSaveGeneral = document.getElementById('btn-save-general');
  if (btnSaveGeneral) {
    btnSaveGeneral.addEventListener('click', async () => {
      const timezone = document.getElementById('sys-timezone')?.value || 'CET-1CEST,M3.5.0,M10.5.0/3';
      const lang = document.getElementById('sys-lang')?.value || 'en';
      const format24h = document.getElementById('sys-format-24h')?.value === 'true';
      const tempUnit = document.getElementById('sys-unit')?.value || 'C';
      const idleFighterEnabled = document.getElementById('sys-idle-fighter-enabled')?.checked ?? true;
      const idleFighterInterval = parseInt(document.getElementById('sys-idle-fighter-interval')?.value) || 10;

      try {
        await API.post('/api/system', {
          timezone,
          lang,
          format_24h: format24h,
          temp_unit: tempUnit,
          idle_fighter_enabled: idleFighterEnabled,
          idle_fighter_interval: idleFighterInterval
        });
        window.showToast('System preferences saved!', 'success');
      } catch (e) {
        window.showToast('Failed to save system preferences', 'error');
      }
    });
  }

  // Save Night Mode settings
  const btnSaveNight = document.getElementById('btn-save-night');
  if (btnSaveNight) {
    btnSaveNight.addEventListener('click', async () => {
      const nightEnabled = document.getElementById('cfg-night-mode-enabled')?.checked || false;
      const turnOffAt = document.getElementById('cfg-turn-off-at')?.value || '23:00';
      const wakeUpAt = document.getElementById('cfg-wake-up-at')?.value || '07:00';
      const nightBright = parseInt(document.getElementById('cfg-night-brightness')?.value) || 0;

      try {
        await API.post('/api/system', {
          night_mode_enabled: nightEnabled,
          turn_off_at: turnOffAt,
          wake_up_at: wakeUpAt,
          night_brightness: nightBright
        });
        window.showToast('Night Mode settings saved!', 'success');
      } catch (e) {
        window.showToast('Failed to save Night Mode settings', 'error');
      }
    });
  }

  // Save Raspberry Pi Hardware settings
  const btnSaveHw = document.getElementById('btn-save-hw');
  if (btnSaveHw) {
    btnSaveHw.addEventListener('click', async () => {
      const base = (window.__sysConfig && window.__sysConfig.matrix) ? { ...window.__sysConfig.matrix } : {};
      const numOr = (id, d) => parseInt(document.getElementById(id)?.value) || d;
      const matrix = {
        ...base,
        height: numOr('hw-rows', 32),
        width: numOr('hw-cols', 64),
        chain_length: numOr('hw-chain', 1),
        driver_chip: document.getElementById('hw-driver-chip')?.value || 'SHIFTREG',
        mapping: document.getElementById('hw-mapping')?.value || 'regular',
        rgb_sequence: document.getElementById('hw-rgb')?.value || 'RGB',
        row_address_mode: numOr('hw-row-addr-type', 0),
        multiplexing: numOr('hw-multiplexing', 0),
        slowdown: numOr('hw-slowdown', 2),
        pwm_bits: numOr('hw-pwm-bits', 11),
        pwm_lsb_nanoseconds: numOr('hw-pwm-lsb', 130),
        limit_refresh_rate_hz: numOr('hw-limit-refresh', 0),
        disable_hardware_pulsing: document.getElementById('hw-disable-pulsing')?.checked || false,
      };

      try {
        await API.post('/api/system', { matrix });
        if (window.__sysConfig) window.__sysConfig.matrix = matrix;
        window.showToast('Hardware settings saved! The panel will restart to apply changes.', 'success');
      } catch (e) {
        window.showToast('Failed to save HW settings', 'error');
      }
    });
  }
}


