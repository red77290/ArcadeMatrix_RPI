import { API } from './api.js';
import { t, setLanguage, SUPPORTED_LANGUAGES } from './i18n.js';
import './components/toast.js';
import { initDynamicEngines, GLOBAL_TIMEZONES } from './dynamic_engines.js';
import { initGifLibrary } from './gif_library.js';

let selectedOtaFile = null;

window.toggleSidebar = function() {
  const sidebar = document.getElementById('sidebar');
  const overlay = document.getElementById('sidebar-overlay');
  if (sidebar) sidebar.classList.toggle('open');
  if (overlay) overlay.classList.toggle('open');
};

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
    document.querySelectorAll('#page-system > .tabs > .tab-btn').forEach(btn => {
      btn.classList.remove('active');
    });
    window.event.currentTarget.classList.add('active');
  }

  // Update panes
  document.querySelectorAll('#page-system > .tab-pane').forEach(pane => {
    pane.classList.remove('active');
  });
  const pane = document.getElementById('sys-tab-' + tabId);
  if (pane) pane.classList.add('active');
};

window.switchMqttSubTab = function(sub) {
  const container = document.getElementById('sys-tab-mqtt');
  if (!container) return;
  const buttons = container.querySelectorAll('.tabs .tab-btn');
  const panes = container.querySelectorAll('.mqtt-subtab-pane');
  panes.forEach(p => p.style.display = 'none');
  buttons.forEach(b => b.classList.remove('active'));
  const targetPane = document.getElementById('mqtt-subtab-' + sub);
  if (targetPane) targetPane.style.display = 'block';
  buttons.forEach(b => {
    if (b.getAttribute('onclick') && b.getAttribute('onclick').includes("'" + sub + "'")) {
      b.classList.add('active');
    }
  });
};

document.addEventListener('DOMContentLoaded', () => {
  initNavigation();
  initDashboard();
  initOta();
  loadVersion();
  initMessagePage();
  initFighterOverlay();
  initGifLibrary();
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

  const sidebar = document.getElementById('sidebar');
  const overlay = document.getElementById('sidebar-overlay');
  if (sidebar) sidebar.classList.remove('open');
  if (overlay) overlay.classList.remove('open');
}
window.switchPage = switchPage;
window.switchToPage = switchPage;

function initNavigation() {
  const overlay = document.getElementById('sidebar-overlay');
  if (overlay) {
    overlay.addEventListener('click', () => {
      const sidebar = document.getElementById('sidebar');
      if (sidebar) sidebar.classList.remove('open');
      overlay.classList.remove('open');
    });
  }

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
    const initialLang = savedLang || (SUPPORTED_LANGUAGES.some(l => l.code === defaultLang) ? defaultLang : 'en');
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

function initMessagePage() {
  const btnSend = document.getElementById('btn-send-message');
  if (!btnSend) return;

  btnSend.addEventListener('click', async () => {
    const textEl = document.getElementById('msg-input');
    const text = textEl ? textEl.value.trim() : '';
    if (!text) {
      window.showToast(t('msg_empty_error', 'Please enter a message to display'), 'error');
      return;
    }

    const color = document.getElementById('msg-color')?.value || '#ffffff';
    const size = parseInt(document.getElementById('msg-size')?.value, 10) || 1;
    const direction = document.getElementById('msg-direction')?.value || 'rtl';
    const speed = parseInt(document.getElementById('msg-speed')?.value, 10) || 30;
    const timeout = parseInt(document.getElementById('msg-timeout')?.value, 10) || 10;

    btnSend.disabled = true;
    try {
      await API.post('/api/message', {
        text,
        color,
        size,
        direction,
        speed,
        timeout,
      });
      window.showToast(t('msg_sent_toast', 'Message sent to matrix!'), 'success');
    } catch (e) {
      window.showToast(t('msg_error_toast', 'Failed to send message to matrix'), 'error');
    } finally {
      btnSend.disabled = false;
    }
  });
}

function initFighterOverlay() {
  const enabledEl = document.getElementById('fighter-enabled');
  const intervalEl = document.getElementById('fighter-interval');
  const speedEl = document.getElementById('fighter-speed');
  const valSpeedEl = document.getElementById('val-fighter-speed');
  const saveBtn = document.getElementById('btn-save-fighter');

  if (!enabledEl || !intervalEl || !saveBtn) return;

  if (speedEl && valSpeedEl && !speedEl._boundInput) {
    speedEl._boundInput = true;
    speedEl.addEventListener('input', (e) => {
      valSpeedEl.textContent = e.target.value;
    });
  }

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
      if (typeof sys.idle_fighter_speed === 'number' && speedEl) {
        speedEl.value = sys.idle_fighter_speed;
        if (valSpeedEl) valSpeedEl.textContent = String(sys.idle_fighter_speed);
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
        idle_fighter_speed: parseInt(speedEl?.value) || 100,
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

    const targetOsSelect = document.getElementById('hw-mqtt-target-os');
    if (targetOsSelect) {
      targetOsSelect.addEventListener('change', () => {
        const osVal = targetOsSelect.value;
        const userInput = document.getElementById('hw-mqtt-ssh-user');
        const passInput = document.getElementById('hw-mqtt-ssh-pass');
        if (osVal === 'retropie') {
          if (userInput) userInput.placeholder = 'pi';
          if (passInput) passInput.placeholder = 'raspberry';
        } else if (osVal === 'batocera') {
          if (userInput) userInput.placeholder = 'root';
          if (passInput) passInput.placeholder = 'linux';
        } else {
          if (userInput) userInput.placeholder = 'root';
          if (passInput) passInput.placeholder = 'recalboxroot';
        }
      });
    }

    btnInstallMqtt.addEventListener('click', async () => {
      const ip = document.getElementById('hw-mqtt-ip').value;
      if (!ip) {
        window.showToast('Please enter Console IP Address', 'error');
        return;
      }
      
      const os = targetOsSelect ? targetOsSelect.value : 'auto';
      let user = null;
      let pass = null;
      if (cbCustomAuth && cbCustomAuth.checked) {
        user = document.getElementById('hw-mqtt-ssh-user').value || (os === 'retropie' ? 'pi' : 'root');
        pass = document.getElementById('hw-mqtt-ssh-pass').value;
      }

      btnInstallMqtt.disabled = true;
      btnInstallMqtt.textContent = 'Installing...';
      try {
        await API.postAuth('/api/mqtt/install', { ip, os, user, pass });
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

        const os = targetOsSelect ? targetOsSelect.value : 'auto';
        let user = null;
        let pass = null;
        if (cbCustomAuth && cbCustomAuth.checked) {
          user = document.getElementById('hw-mqtt-ssh-user').value || (os === 'retropie' ? 'pi' : 'root');
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
          const res = await API.postAuth('/api/mqtt/logs', { ip, os, user, pass });
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

  const updateFrontendSyncShowcaseVisibility = () => {
    const igEnEl = document.getElementById('hw-mqtt-ingame-showcase');
    const cycleEl = document.getElementById('hw-mqtt-cycle-mode');
    const cardSrcEl = document.getElementById('hw-mqtt-card-source');
    const igBox = document.getElementById('group-fsync-ingame-options');
    const intervalBox = document.getElementById('group-fsync-marquee-interval');
    const customCardBox = document.getElementById('group-fsync-custom-card');

    const igEnabled = !igEnEl || igEnEl.value === 'true';
    if (igBox) igBox.style.display = igEnabled ? 'block' : 'none';
    if (intervalBox) {
      intervalBox.style.display = (!cycleEl || cycleEl.value === 'alternating') ? 'block' : 'none';
    }
    if (customCardBox) {
      customCardBox.style.display = (cardSrcEl && cardSrcEl.value === 'custom') ? 'block' : 'none';
    }
  };
  window.updateFrontendSyncShowcaseVisibility = updateFrontendSyncShowcaseVisibility;

  ['hw-mqtt-ingame-showcase', 'hw-mqtt-cycle-mode', 'hw-mqtt-card-source'].forEach((id) => {
    const el = document.getElementById(id);
    if (el) el.addEventListener('change', updateFrontendSyncShowcaseVisibility);
  });

  const btnEvictShowcaseCache = document.getElementById('btn-evict-showcase-cache');
  if (btnEvictShowcaseCache) {
    btnEvictShowcaseCache.addEventListener('click', async () => {
      btnEvictShowcaseCache.disabled = true;
      try {
        const res = await API.postAuth('/api/showcase/cache/evict', {});
        const cnt = res && typeof res.evicted_entries === 'number' ? res.evicted_entries : 0;
        window.showToast(
          `${t('fsync_cache_evicted', 'Showcase conversion cache evicted')} (${cnt})`,
          'success'
        );
      } catch (e) {
        window.showToast(
          t('fsync_cache_evict_failed', 'Failed to evict Showcase cache (verify Showcase is reachable)'),
          'error'
        );
      } finally {
        btnEvictShowcaseCache.disabled = false;
      }
    });
  }

  const btnSaveMqtt = document.getElementById('btn-save-mqtt');
  if (btnSaveMqtt) {
    btnSaveMqtt.addEventListener('click', async () => {
      const base = (window.__sysConfig && window.__sysConfig.mqtt) ? { ...window.__sysConfig.mqtt } : {};
      const mqtt = {
        ...base,
        enabled: document.getElementById('hw-mqtt-enable').value === '1',
        allow_overlay: document.getElementById('hw-mqtt-overlay').value === '1',
        broker: document.getElementById('hw-mqtt-broker').value,
        port: parseInt(document.getElementById('hw-mqtt-port').value, 10) || 1883,
        user: document.getElementById('hw-mqtt-user').value,
        pass: document.getElementById('hw-mqtt-pass').value,
        use_showcase_broker: document.getElementById('hw-mqtt-use-showcase-broker')?.value !== 'false',
        ingame_showcase_enabled: document.getElementById('hw-mqtt-ingame-showcase')?.value !== 'false',
        ingame_cycle_mode: document.getElementById('hw-mqtt-cycle-mode')?.value || 'alternating',
        ingame_marquee_interval_sec: Math.max(5, parseInt(document.getElementById('hw-mqtt-marquee-interval')?.value, 10) || 45),
        card_style_source: document.getElementById('hw-mqtt-card-source')?.value || 'showcase_profile',
        include_card_intro: document.getElementById('hw-mqtt-card-intro')?.value !== 'false',
        card_layout: document.getElementById('hw-mqtt-card-layout')?.value || 'classic_split',
        card_theme: document.getElementById('hw-mqtt-card-theme')?.value || 'tinted',
        include_poster: document.getElementById('hw-mqtt-include-poster')?.value !== 'false',
        include_synopsis: document.getElementById('hw-mqtt-include-synopsis')?.value !== 'false',
        text_scroll_speed: document.getElementById('hw-mqtt-scroll-speed')?.value || 'normal',
        jacket_panorama: document.getElementById('hw-mqtt-jacket-pano')?.value !== 'false',
        marquee_panorama: document.getElementById('hw-mqtt-marquee-pano')?.value === 'true',
        snapshot_panorama_count: Math.max(0, Math.min(5, parseInt(document.getElementById('hw-mqtt-snap-pano')?.value, 10) || 0)),
        include_video: document.getElementById('hw-mqtt-include-video')?.value !== 'false',
        video_max_duration_s: Math.max(2, Math.min(120, parseInt(document.getElementById('hw-mqtt-video-duration')?.value, 10) || 12)),
        video_fps: parseInt(document.getElementById('hw-mqtt-video-fps')?.value, 10) || 15,
        video_mode: document.getElementById('hw-mqtt-video-mode')?.value || 'auto_action',
        color_boost: document.getElementById('hw-mqtt-color-boost')?.value || 'auto',
      };
      try {
        const res = await API.post('/api/system', { mqtt });
        if (window.__sysConfig) window.__sysConfig.mqtt = mqtt;
        if (res && res.status === 'rebooting') {
          window.showToast('MQTT Settings Saved! Restarting...', 'info');
        } else {
          window.showToast('MQTT Settings Saved!', 'success');
        }
      } catch (e) {
        window.showToast('Failed to save MQTT Settings', 'error');
      }
    });
  }

  const btnSaveDataMqtt = document.getElementById('btn-save-data-mqtt');
  if (btnSaveDataMqtt) {
    btnSaveDataMqtt.addEventListener('click', async () => {
      const data_mqtt = {
        broker: document.getElementById('hw-data-mqtt-broker').value.trim(),
        port: parseInt(document.getElementById('hw-data-mqtt-port').value, 10) || 1883,
        user: document.getElementById('hw-data-mqtt-user').value,
        pass: document.getElementById('hw-data-mqtt-pass').value,
      };
      try {
        await API.post('/api/system', { data_mqtt });
        if (window.__sysConfig) window.__sysConfig.data_mqtt = data_mqtt;
        window.showToast('MQTT Data Saved!', 'success');
      } catch (e) {
        window.showToast('Failed to save MQTT Data', 'error');
      }
    });
  }

  const updateShowcaseBrokerVisibility = () => {
    const useBrokerEl = document.getElementById('hw-showcase-use-broker');
    const sourceEl = document.getElementById('hw-showcase-broker-source');
    const extGroup = document.getElementById('group-showcase-external-broker');
    const customGroup = document.getElementById('group-showcase-custom-url');
    const useEmbedded = !useBrokerEl || useBrokerEl.value === 'true';
    if (extGroup) extGroup.style.display = useEmbedded ? 'none' : 'grid';
    if (customGroup) {
      customGroup.style.display = (!useEmbedded && sourceEl && sourceEl.value === 'custom') ? 'block' : 'none';
    }
  };
  window.updateShowcaseBrokerVisibility = updateShowcaseBrokerVisibility;

  const selUseBroker = document.getElementById('hw-showcase-use-broker');
  const selBrokerSource = document.getElementById('hw-showcase-broker-source');
  if (selUseBroker) selUseBroker.addEventListener('change', updateShowcaseBrokerVisibility);
  if (selBrokerSource) selBrokerSource.addEventListener('change', updateShowcaseBrokerVisibility);

  const checkShowcaseStatus = async (forceProbe = false) => {
    const badge = document.getElementById('showcase-comm-badge');
    const enEl = document.getElementById('hw-showcase-enabled');
    const urlEl = document.getElementById('hw-showcase-server-url');
    if (!badge) return;
    const enabled = enEl ? enEl.value === 'true' : !!(window.__sysConfig?.showcase?.enabled);
    const url = (urlEl ? urlEl.value : (window.__sysConfig?.showcase?.server_url || 'http://dmdmediashowcase.local:8890')).trim() || 'http://dmdmediashowcase.local:8890';
    if (!enabled && !forceProbe) {
      badge.textContent = t('showcase_status_disabled', '⚪ Disabled');
      badge.style.background = 'rgba(255,255,255,0.08)';
      badge.style.color = '#b0bec5';
      badge.style.borderColor = 'rgba(255,255,255,0.15)';
      return;
    }
    badge.textContent = t('showcase_status_checking', '⏳ Checking...');
    badge.style.background = 'rgba(255, 193, 7, 0.15)';
    badge.style.color = '#ffd54f';
    badge.style.borderColor = 'rgba(255, 193, 7, 0.35)';
    try {
      const qs = `?url=${encodeURIComponent(url)}&enabled=${enabled ? 'true' : 'false'}${forceProbe ? '&force=true' : ''}`;
      const res = await API.get(`/api/showcase/status${qs}`);
      if (res && res.online) {
        if (res.resolved_url) {
          window.__showcaseResolvedUrl = res.resolved_url;
        }
        const ver = res.version ? `v${res.version}` : '';
        const resolvedHost = (res.resolved_url && res.resolved_url !== url)
          ? res.resolved_url.replace(/^https?:\/\//i, '')
          : '';
        const meta = [ver, resolvedHost].filter(Boolean).join(' • ');
        badge.textContent = `${t('showcase_status_ok', '🟢 Communication OK')}${meta ? ` (${meta})` : ''}`;
        badge.style.background = 'rgba(0, 230, 118, 0.15)';
        badge.style.color = '#00e676';
        badge.style.borderColor = 'rgba(0, 230, 118, 0.4)';
      } else {
        badge.textContent = t('showcase_status_offline', '🔴 Unreachable');
        badge.style.background = 'rgba(255, 82, 82, 0.15)';
        badge.style.color = '#ff5252';
        badge.style.borderColor = 'rgba(255, 82, 82, 0.4)';
      }
    } catch (_) {
      badge.textContent = t('showcase_status_offline', '🔴 Unreachable');
      badge.style.background = 'rgba(255, 82, 82, 0.15)';
      badge.style.color = '#ff5252';
      badge.style.borderColor = 'rgba(255, 82, 82, 0.4)';
    }
  };
  window.checkShowcaseStatus = checkShowcaseStatus;

  const selShowcaseEnabled = document.getElementById('hw-showcase-enabled');
  if (selShowcaseEnabled) {
    selShowcaseEnabled.addEventListener('change', () => {
      checkShowcaseStatus(selShowcaseEnabled.value === 'true');
    });
  }
  const btnTestShowcase = document.getElementById('btn-test-showcase-conn');
  if (btnTestShowcase) {
    btnTestShowcase.addEventListener('click', () => checkShowcaseStatus(true));
  }

  const btnSaveShowcase = document.getElementById('btn-save-showcase');
  if (btnSaveShowcase) {
    btnSaveShowcase.addEventListener('click', async () => {
      const enabled = document.getElementById('hw-showcase-enabled')?.value === 'true';
      const server_url = (document.getElementById('hw-showcase-server-url').value || '').trim() || 'http://dmdmediashowcase.local:8890';
      const showcase = {
        enabled,
        server_url,
        transport_mode: document.getElementById('hw-showcase-transport-mode').value,
        use_showcase_broker: document.getElementById('hw-showcase-use-broker').value === 'true',
        mqtt_broker_source: document.getElementById('hw-showcase-broker-source').value,
        mqtt_custom_url: (document.getElementById('hw-showcase-custom-url').value || '').trim(),
      };
      try {
        await API.post('/api/system', { showcase });
        if (window.__sysConfig) window.__sysConfig.showcase = showcase;
        window.showToast(t('showcase_saved', 'Showcase settings saved successfully!'), 'success');
        await checkShowcaseStatus(false);
      } catch (e) {
        window.showToast(t('showcase_save_failed', 'Failed to save Showcase settings'), 'error');
      }
    });
  }

  const btnSaveApi = document.getElementById('btn-save-api');
  if (btnSaveApi) {
    btnSaveApi.addEventListener('click', async () => {
      const api_auth_enabled = document.getElementById('hw-api-auth').value === 'true';
      const api_token = document.getElementById('hw-api-token').value.trim();
      try {
        if (api_token && api_token !== '••••••••') {
          localStorage.setItem('api_token', api_token);
        }
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
    setChk('sys-idle-fighter-enabled', sys.idle_fighter_enabled ?? false);
    setVal('sys-idle-fighter-interval', sys.idle_fighter_interval || 10);
    setVal('sys-idle-fighter-speed', sys.idle_fighter_speed || 100);

    const sysFighterSpeedEl = document.getElementById('sys-idle-fighter-speed');
    const valSysFighterSpeed = document.getElementById('val-sys-fighter-speed');
    if (valSysFighterSpeed) valSysFighterSpeed.textContent = String(sys.idle_fighter_speed || 100);
    if (sysFighterSpeedEl && valSysFighterSpeed && !sysFighterSpeedEl._boundInput) {
      sysFighterSpeedEl._boundInput = true;
      sysFighterSpeedEl.addEventListener('input', (e) => {
        valSysFighterSpeed.textContent = e.target.value;
      });
    }

    const sysLangEl = document.getElementById('sys-lang');
    if (sysLangEl && !sysLangEl._boundChange) {
      sysLangEl._boundChange = true;
      sysLangEl.addEventListener('change', (e) => {
        setLanguage(e.target.value);
      });
    }

    // Screen Orientation & Transitions (General preferences)
    setVal('sys-screen-rotation', matrix.rotation ?? 0);
    setVal('sys-rotation-transition', matrix.transition_effect || 'vortex');
    setVal('sys-slot-transition', matrix.slot_transition || 'none');
    setVal('sys-slot-transition-duration', matrix.slot_transition_duration_ms || 500);
    setVal('sys-transition-duration', matrix.transition_duration_ms || 400);

    // Raspberry Pi Matrix Hardware (pure RPi HUB75 configuration)
    setVal('hw-rows', matrix.height || 32);
    setVal('hw-cols', matrix.width || 64);
    setVal('hw-chain', matrix.chain_length || 1);
    setVal('hw-parallel', matrix.parallel ?? 1);
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
    setVal('hw-render-pipeline', matrix.render_pipeline || 'canvas_single');

    // MQTT (nested mqtt.*)
    setVal('hw-mqtt-enable', mqtt.enabled ? '1' : '0');
    setVal('hw-mqtt-overlay', mqtt.allow_overlay ? '1' : '0');
    setVal('hw-mqtt-broker', mqtt.broker);
    setVal('hw-mqtt-port', mqtt.port);
    setVal('hw-mqtt-user', mqtt.user || '');
    setVal('hw-mqtt-pass', mqtt.pass || '');
    setVal('hw-mqtt-use-showcase-broker', mqtt.use_showcase_broker !== false ? 'true' : 'false');
    setVal('hw-mqtt-ingame-showcase', mqtt.ingame_showcase_enabled !== false ? 'true' : 'false');
    setVal('hw-mqtt-cycle-mode', mqtt.ingame_cycle_mode || 'alternating');
    setVal('hw-mqtt-marquee-interval', mqtt.ingame_marquee_interval_sec ?? 45);
    setVal('hw-mqtt-card-source', mqtt.card_style_source || 'showcase_profile');
    setVal('hw-mqtt-card-intro', mqtt.include_card_intro !== false ? 'true' : 'false');
    setVal('hw-mqtt-card-layout', mqtt.card_layout || 'classic_split');
    setVal('hw-mqtt-card-theme', mqtt.card_theme || 'tinted');
    setVal('hw-mqtt-include-poster', mqtt.include_poster !== false ? 'true' : 'false');
    setVal('hw-mqtt-include-synopsis', mqtt.include_synopsis !== false ? 'true' : 'false');
    setVal('hw-mqtt-scroll-speed', mqtt.text_scroll_speed || 'normal');
    setVal('hw-mqtt-jacket-pano', mqtt.jacket_panorama !== false ? 'true' : 'false');
    setVal('hw-mqtt-marquee-pano', mqtt.marquee_panorama ? 'true' : 'false');
    setVal('hw-mqtt-snap-pano', mqtt.snapshot_panorama_count ?? 0);
    setVal('hw-mqtt-include-video', mqtt.include_video !== false ? 'true' : 'false');
    setVal('hw-mqtt-video-duration', mqtt.video_max_duration_s ?? 12);
    setVal('hw-mqtt-video-fps', String(mqtt.video_fps ?? 15));
    setVal('hw-mqtt-video-mode', mqtt.video_mode || 'auto_action');
    setVal('hw-mqtt-color-boost', mqtt.color_boost || 'auto');
    if (typeof window.updateFrontendSyncShowcaseVisibility === 'function') {
      window.updateFrontendSyncShowcaseVisibility();
    }

    // Data MQTT (nested data_mqtt.*)
    const dataMqtt = cfg.data_mqtt || {};
    setVal('hw-data-mqtt-broker', dataMqtt.broker || '');
    setVal('hw-data-mqtt-port', dataMqtt.port ?? 1883);
    setVal('hw-data-mqtt-user', dataMqtt.user || '');
    setVal('hw-data-mqtt-pass', dataMqtt.pass || '');

    // DMD Media Showcase (nested showcase.*)
    const showcase = cfg.showcase || {};
    setVal('hw-showcase-enabled', showcase.enabled ? 'true' : 'false');
    setVal('hw-showcase-server-url', showcase.server_url || 'http://dmdmediashowcase.local:8890');
    setVal('hw-showcase-transport-mode', showcase.transport_mode || 'mqtt_udp');
    setVal('hw-showcase-use-broker', showcase.use_showcase_broker !== undefined ? (showcase.use_showcase_broker ? 'true' : 'false') : 'true');
    setVal('hw-showcase-broker-source', showcase.mqtt_broker_source || 'global');
    setVal('hw-showcase-custom-url', showcase.mqtt_custom_url || '');
    if (typeof window.updateShowcaseBrokerVisibility === 'function') {
      window.updateShowcaseBrokerVisibility();
    }
    if (typeof window.checkShowcaseStatus === 'function') {
      window.checkShowcaseStatus(false);
    }

    // API auth (top-level)
    setVal('hw-api-auth', cfg.api_auth_enabled ? 'true' : 'false');
    setVal('hw-api-token', cfg.api_token || '');
    if (cfg.api_token && cfg.api_token !== '••••••••' && !localStorage.getItem('api_token')) {
      localStorage.setItem('api_token', cfg.api_token);
    }
  } catch (e) {
    console.error('Failed to load settings', e);
  }

  // Save General System Preferences (including real-time Screen Orientation & Transitions)
  const btnSaveGeneral = document.getElementById('btn-save-general');
  if (btnSaveGeneral && !btnSaveGeneral._boundClick) {
    btnSaveGeneral._boundClick = true;
    btnSaveGeneral.addEventListener('click', async () => {
      const timezone = document.getElementById('sys-timezone')?.value || 'CET-1CEST,M3.5.0,M10.5.0/3';
      const lang = document.getElementById('sys-lang')?.value || 'en';
      const format24h = document.getElementById('sys-format-24h')?.value === 'true';
      const tempUnit = document.getElementById('sys-unit')?.value || 'C';
      const idleFighterEnabled = document.getElementById('sys-idle-fighter-enabled')?.checked ?? false;
      const idleFighterInterval = parseInt(document.getElementById('sys-idle-fighter-interval')?.value) || 10;
      const idleFighterSpeed = parseInt(document.getElementById('sys-idle-fighter-speed')?.value) || 100;
      const rotation = parseInt(document.getElementById('sys-screen-rotation')?.value) || 0;
      const transitionEffect = document.getElementById('sys-rotation-transition')?.value || 'vortex';
      const transitionDuration = parseInt(document.getElementById('sys-transition-duration')?.value) || 400;
      const slotTransition = document.getElementById('sys-slot-transition')?.value || 'none';
      const slotTransitionDuration = parseInt(document.getElementById('sys-slot-transition-duration')?.value) || 500;

      try {
        await API.post('/api/system', {
          timezone,
          lang,
          format_24h: format24h,
          temp_unit: tempUnit,
          idle_fighter_enabled: idleFighterEnabled,
          idle_fighter_interval: idleFighterInterval,
          idle_fighter_speed: idleFighterSpeed,
          rotation,
          transition_effect: transitionEffect,
          transition_duration_ms: transitionDuration,
          slot_transition: slotTransition,
          slot_transition_duration_ms: slotTransitionDuration
        });
        if (window.__sysConfig && window.__sysConfig.matrix) {
          window.__sysConfig.matrix.rotation = rotation;
          window.__sysConfig.matrix.transition_effect = transitionEffect;
          window.__sysConfig.matrix.transition_duration_ms = transitionDuration;
          window.__sysConfig.matrix.slot_transition = slotTransition;
          window.__sysConfig.matrix.slot_transition_duration_ms = slotTransitionDuration;
        }
        window.invalidateCachedEndpoint?.('/api/themes');
        setLanguage(lang);
        window.showToast(t('system_prefs_saved', 'System preferences and display rotation saved in real-time!'), 'success');
      } catch (e) {
        window.showToast(t('system_prefs_error', 'Failed to save system preferences'), 'error');
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

  // Save Raspberry Pi Hardware settings (requires panel daemon restart for hardware bus/chip re-init)
  const btnSaveHw = document.getElementById('btn-save-hw');
  if (btnSaveHw && !btnSaveHw._boundClick) {
    btnSaveHw._boundClick = true;
    btnSaveHw.addEventListener('click', async () => {
      const base = (window.__sysConfig && window.__sysConfig.matrix) ? { ...window.__sysConfig.matrix } : {};
      const numOr = (id, d) => {
        const v = parseInt(document.getElementById(id)?.value, 10);
        return Number.isNaN(v) ? d : v;
      };
      const matrix = {
        ...base,
        height: numOr('hw-rows', 32),
        width: numOr('hw-cols', 64),
        chain_length: numOr('hw-chain', 1),
        parallel: numOr('hw-parallel', 1),
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
        render_pipeline: document.getElementById('hw-render-pipeline')?.value || 'canvas_single',
      };

      try {
        await API.post('/api/system', { matrix });
        if (window.__sysConfig) window.__sysConfig.matrix = matrix;
        window.invalidateCachedEndpoint?.('/api/themes');
        window.showToast('Hardware settings saved! The panel will restart to apply physical bus changes.', 'success');
      } catch (e) {
        window.showToast('Failed to save HW settings', 'error');
      }
    });
  }
}


