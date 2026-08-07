import { API } from './api.js';
import { setLanguage } from './i18n.js';
import './components/toast.js';

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
  initPlaylists();
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

  const btnRestartApp = document.getElementById('btn-restart-app');
  if (btnRestartApp) {
    btnRestartApp.addEventListener('click', async () => {
      if (confirm('Are you sure you want to soft restart the application? (Hardware config will be reapplied)')) {
        try {
          await API.post('/api/system/restart_app');
          window.showToast('Restarting application...', 'info');
          setTimeout(() => location.reload(), 2000);
        } catch (e) {
          window.showToast('Failed to restart application', 'error');
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
      const mqtt_user = document.getElementById('hw-mqtt-user').value;
      const mqtt_pass = document.getElementById('hw-mqtt-pass').value;
      try {
        await API.post('/api/settings', { mqtt_enable, mqtt_broker, mqtt_port, mqtt_user, mqtt_pass });
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
        await API.post('/api/settings', { api_auth_enabled, api_token });
        window.showToast('API Auth Settings Saved!', 'success');
      } catch (e) {
        window.showToast('Failed to save API Auth Settings', 'error');
      }
    });
  }
}

async function initSettings() {
  // Load fonts list
  try {
    const fonts = await API.get('/api/fonts');
    const clockFontSel = document.getElementById('cfg-clock-font');
    const dateFontSel = document.getElementById('cfg-date-font');
    if (clockFontSel && dateFontSel) {
      fonts.forEach(f => {
        clockFontSel.add(new Option(f, f));
        dateFontSel.add(new Option(f, f));
      });
    }
  } catch (e) {
    console.warn('Failed to load fonts list', e);
  }

  // Load current settings
  try {
    const s = await API.get('/api/settings');
    
    // Dashboard
    const sliderBright = document.getElementById('slider-brightness');
    if (sliderBright) {
      sliderBright.value = s.brightness_limit;
      document.getElementById('val-brightness').textContent = s.brightness_limit;
      sliderBright.addEventListener('input', (e) => {
        document.getElementById('val-brightness').textContent = e.target.value;
      });
      sliderBright.addEventListener('change', async (e) => {
        await API.post('/api/settings', { brightness_limit: parseInt(e.target.value) });
        window.showToast('Brightness updated', 'info');
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

    // Clock
    document.getElementById('cfg-clock-theme').value = s.clock_theme;
    document.getElementById('cfg-clock-font').value = s.clock_font;
    document.getElementById('cfg-clock-format').value = s.time_format;
    document.getElementById('cfg-clock-size').value = s.time_size || 2;
    document.getElementById('cfg-ntp-server').value = s.ntp_server || 'pool.ntp.org';
    document.getElementById('cfg-timezone').value = s.timezone || 'Europe/Paris';
    document.getElementById('cfg-clock-color1').value = s.clock_color_1.toLowerCase();
    document.getElementById('cfg-clock-color2').value = s.clock_color_2.toLowerCase();
    document.getElementById('cfg-clock-x').value = s.clock_offset_x;
    document.getElementById('cfg-clock-y').value = s.clock_offset_y;

    // Date
    document.getElementById('cfg-date-theme').value = s.date_theme;
    document.getElementById('cfg-date-font').value = s.date_font;
    document.getElementById('cfg-date-format').value = s.date_format;
    document.getElementById('cfg-date-size').value = s.date_size || 1;
    document.getElementById('cfg-date-color1').value = s.date_color_1.toLowerCase();
    document.getElementById('cfg-date-color2').value = s.date_color_2.toLowerCase();
    document.getElementById('cfg-date-x').value = s.date_offset_x;
    document.getElementById('cfg-date-y').value = s.date_offset_y;

    // Weather
    document.getElementById('cfg-weather-api').value = s.weather_api_key;
    document.getElementById('cfg-weather-city').value = s.weather_city;
    document.getElementById('cfg-weather-lang').value = s.weather_lang;
    document.getElementById('cfg-weather-x').value = s.weather_offset_x || 0;
    document.getElementById('cfg-weather-y').value = s.weather_offset_y || 0;

    // Crypto & Stock
    if (document.getElementById('cfg-crypto-symbols')) document.getElementById('cfg-crypto-symbols').value = s.crypto_symbols || 'BTC,ETH,SOL,DOGE';
    if (document.getElementById('cfg-dur-crypto')) document.getElementById('cfg-dur-crypto').value = s.crypto_duration_sec || 5;
    if (document.getElementById('cfg-cache-crypto')) document.getElementById('cfg-cache-crypto').value = s.crypto_cache_ttl_min || 1;
    if (document.getElementById('cfg-stock-symbols')) document.getElementById('cfg-stock-symbols').value = s.stock_symbols || 'AAPL,NVDA,TSLA,MSFT';
    if (document.getElementById('cfg-dur-stock')) document.getElementById('cfg-dur-stock').value = s.stock_duration_sec || 5;
    if (document.getElementById('cfg-cache-stock')) document.getElementById('cfg-cache-stock').value = s.stock_cache_ttl_min || 1;

    // Rotation
    document.getElementById('cfg-rotation').value = s.rotation;
    const rotationArr = (s.rotation || '').split(',').map(x => x.trim());
    document.querySelectorAll('.rotation-toggles input[type="checkbox"]').forEach(cb => {
      cb.checked = rotationArr.includes(cb.value);
    });
    document.getElementById('cfg-dur-clock').value = s.clock_duration_sec;
    document.getElementById('cfg-dur-date').value = s.date_duration_sec;
    document.getElementById('cfg-dur-weather').value = s.weather_duration_sec;
    document.getElementById('cfg-dur-gifs').value = s.gifs_count || 5;
    document.getElementById('cfg-sprite-enable').value = s.fighter_enabled === 'true' ? 'true' : 'false';
    document.getElementById('cfg-fighter-int').value = s.fighter_interval_sec || 5;

    // Night Mode
    document.getElementById('cfg-night-enable').value = s.night_mode_enabled ? 'true' : 'false';
    document.getElementById('cfg-night-off').value = s.turn_off_at;
    document.getElementById('cfg-night-on').value = s.wake_up_at;

  } catch (e) {
    console.error('Failed to load settings', e);
  }

  // Save Display Settings
  const btnSaveDisplayList = document.querySelectorAll('.btn-save-display');
  btnSaveDisplayList.forEach(btnSaveDisplay => {
    btnSaveDisplay.addEventListener('click', async () => {
      btnSaveDisplay.disabled = true;
      btnSaveDisplay.textContent = 'Saving...';
      try {
          const selectedRotations = Array.from(document.querySelectorAll('.rotation-toggles input[type="checkbox"]:checked'))
            .map(cb => cb.value).join(',');

          const payload = {
            clock_theme: parseInt(document.getElementById('cfg-clock-theme').value),
            clock_font: document.getElementById('cfg-clock-font').value,
            time_format: document.getElementById('cfg-clock-format').value,
            time_size: parseInt(document.getElementById('cfg-clock-size').value) || 2,
            ntp_server: document.getElementById('cfg-ntp-server').value,
            timezone: document.getElementById('cfg-timezone').value,
            clock_color_1: document.getElementById('cfg-clock-color1').value,
            clock_color_2: document.getElementById('cfg-clock-color2').value,
            clock_offset_x: parseInt(document.getElementById('cfg-clock-x').value) || 0,
            clock_offset_y: parseInt(document.getElementById('cfg-clock-y').value) || 0,

            date_theme: parseInt(document.getElementById('cfg-date-theme').value),
            date_font: document.getElementById('cfg-date-font').value,
            date_format: document.getElementById('cfg-date-format').value,
            date_size: parseInt(document.getElementById('cfg-date-size').value) || 1,
            date_color_1: document.getElementById('cfg-date-color1').value,
            date_color_2: document.getElementById('cfg-date-color2').value,
            date_offset_x: parseInt(document.getElementById('cfg-date-x').value) || 0,
            date_offset_y: parseInt(document.getElementById('cfg-date-y').value) || 0,

            weather_api_key: document.getElementById('cfg-weather-api').value,
            weather_city: document.getElementById('cfg-weather-city').value,
            weather_lang: document.getElementById('cfg-weather-lang').value,
            weather_offset_x: parseInt(document.getElementById('cfg-weather-x').value) || 0,
            weather_offset_y: parseInt(document.getElementById('cfg-weather-y').value) || 0,

            crypto_symbols: document.getElementById('cfg-crypto-symbols') ? document.getElementById('cfg-crypto-symbols').value : 'BTC,ETH,SOL,DOGE',
            crypto_duration_sec: parseInt(document.getElementById('cfg-dur-crypto')?.value) || 5,
            crypto_cache_ttl_min: parseInt(document.getElementById('cfg-cache-crypto')?.value) || 1,
            stock_symbols: document.getElementById('cfg-stock-symbols') ? document.getElementById('cfg-stock-symbols').value : 'AAPL,NVDA,TSLA,MSFT',
            stock_duration_sec: parseInt(document.getElementById('cfg-dur-stock')?.value) || 5,
            stock_cache_ttl_min: parseInt(document.getElementById('cfg-cache-stock')?.value) || 1,

            rotation: selectedRotations,
            clock_duration_sec: parseInt(document.getElementById('cfg-dur-clock').value) || 10,
          date_duration_sec: parseInt(document.getElementById('cfg-dur-date').value) || 10,
          weather_duration_sec: parseInt(document.getElementById('cfg-dur-weather').value) || 10,
          gifs_count: parseInt(document.getElementById('cfg-dur-gifs').value) || 5,
          fighter_enabled: document.getElementById('cfg-sprite-enable').value === 'true',
          fighter_interval_sec: parseInt(document.getElementById('cfg-fighter-int').value) || 5,

          night_mode_enabled: document.getElementById('cfg-night-enable').value === 'true',
          turn_off_at: document.getElementById('cfg-night-off').value,
          wake_up_at: document.getElementById('cfg-night-on').value,
        };
        await API.post('/api/settings', payload);
        window.showToast('Display settings saved!', 'success');
      } catch (e) {
        window.showToast('Failed to save settings', 'error');
      } finally {
        btnSaveDisplay.disabled = false;
        btnSaveDisplay.textContent = 'Save Display Settings';
      }
    });
  });

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
        await API.post('/api/settings', { 
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
        window.showToast('Hardware settings saved! Restarting app...', 'success');
        setTimeout(async () => {
          await API.post('/api/system/restart_app');
        }, 1500);
      } catch (e) {
        window.showToast('Failed to save HW settings', 'error');
      }
    });
  }

  // Send Custom Message
  const btnSendMsg = document.getElementById('btn-send-message');
  if (btnSendMsg) {
    btnSendMsg.addEventListener('click', async () => {
      const text = document.getElementById('msg-input').value;
      const color = document.getElementById('msg-color')?.value || '#ffffff';
      const size = parseInt(document.getElementById('msg-size')?.value) || 1;
      const direction = document.getElementById('msg-direction')?.value || 'left';
      const speed = parseInt(document.getElementById('msg-speed')?.value) || 30;
      const timeout = parseInt(document.getElementById('msg-timeout')?.value) || 10;
      
      if (!text) return;
      try {
        await API.post('/api/message', {
          text: text,
          color: color,
          size: size,
          direction: direction,
          speed: speed,
          timeoutSeconds: timeout
        });
        window.showToast('Message sent!', 'success');
      } catch (e) {
        window.showToast('Failed to send message', 'error');
      }
    });
  }
}

async function initPlaylists() {
  const container = document.querySelector('.playlist-list');
  if (!container) return;
  
  container.innerHTML = `<div style="padding:2rem;text-align:center;color:var(--text-color);">Loading GIFs...<br><small style="opacity:0.7">Reading SD Card, this may take a few seconds...</small></div>`;
  
  try {
    const res = await API.get('/api/playlists');
    const allPlaylists = res; 
    
    let selectedPaths = [];
    try {
      const selRes = await API.get('/api/playlists/selected');
      selectedPaths = selRes.playlists || [];
    } catch (e) {
      console.warn('Could not fetch selected playlists', e);
    }
    
    let html = '';
    for (const [name, info] of Object.entries(allPlaylists)) {
      const isChecked = selectedPaths.includes(info.path) ? 'checked' : '';
      const count = info.count || (info.files ? info.files.length : 0);
      
      html += `
        <div class="playlist-item">
          <div class="info">
            <h4>${name}</h4>
            <span>${count} GIFs found</span>
          </div>
          <div class="action">
            <label style="display:flex; align-items:center; cursor:pointer;">
              <input type="checkbox" class="playlist-cb" value="${info.path}" ${isChecked} style="width:20px;height:20px;margin-right:10px;accent-color:var(--primary);">
              Include
            </label>
          </div>
        </div>
      `;
    }
    
    if (Object.keys(allPlaylists).length === 0) {
      html = `<div style="padding:1rem;text-align:center;">No GIF folders found. Check your data/gifs directory.</div>`;
    } else {
      html += `<button id="btn-save-playlists" class="btn btn-primary" style="margin-top:1rem; width:100%;">Save GIF Playlists</button>`;
    }
    
    container.innerHTML = html;
    
    const btnSave = document.getElementById('btn-save-playlists');
    if (btnSave) {
      btnSave.addEventListener('click', async () => {
        btnSave.disabled = true;
        btnSave.textContent = 'Saving...';
        
        const checkboxes = document.querySelectorAll('.playlist-cb:checked');
        const pathsToSave = Array.from(checkboxes).map(cb => cb.value);
        
        try {
          await API.post('/api/playlists/save', { playlists: pathsToSave });
          window.showToast('Playlists saved successfully!', 'success');
        } catch (e) {
          window.showToast('Failed to save playlists', 'error');
        } finally {
          btnSave.disabled = false;
          btnSave.textContent = 'Save GIF Playlists';
        }
      });
    }
    
  } catch (e) {
    container.innerHTML = `<div style="padding:2rem;text-align:center;color:#ef4444;">Failed to load playlists.</div>`;
    console.error(e);
  }
}


