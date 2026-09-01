import { API } from './api.js';
import { translations, t } from './i18n.js';

export const GLOBAL_TIMEZONES = [
  { value: "system", label: "System Default / Fuseau Système" },
  { value: "Europe/Paris", label: "Europe/Paris (UTC+1/+2)" },
  { value: "Europe/London", label: "Europe/London (UTC+0/+1)" },
  { value: "Europe/Dublin", label: "Europe/Dublin (UTC+0/+1)" },
  { value: "Europe/Lisbon", label: "Europe/Lisbon (UTC+0/+1)" },
  { value: "Europe/Berlin", label: "Europe/Berlin (UTC+1/+2)" },
  { value: "Europe/Madrid", label: "Europe/Madrid (UTC+1/+2)" },
  { value: "Europe/Rome", label: "Europe/Rome (UTC+1/+2)" },
  { value: "Europe/Brussels", label: "Europe/Brussels (UTC+1/+2)" },
  { value: "Europe/Amsterdam", label: "Europe/Amsterdam (UTC+1/+2)" },
  { value: "Europe/Zurich", label: "Europe/Zurich (UTC+1/+2)" },
  { value: "Europe/Vienna", label: "Europe/Vienna (UTC+1/+2)" },
  { value: "Europe/Warsaw", label: "Europe/Warsaw (UTC+1/+2)" },
  { value: "Europe/Prague", label: "Europe/Prague (UTC+1/+2)" },
  { value: "Europe/Stockholm", label: "Europe/Stockholm (UTC+1/+2)" },
  { value: "Europe/Oslo", label: "Europe/Oslo (UTC+1/+2)" },
  { value: "Europe/Copenhagen", label: "Europe/Copenhagen (UTC+1/+2)" },
  { value: "Europe/Athens", label: "Europe/Athens (UTC+2/+3)" },
  { value: "Europe/Helsinki", label: "Europe/Helsinki (UTC+2/+3)" },
  { value: "Europe/Bucharest", label: "Europe/Bucharest (UTC+2/+3)" },
  { value: "Europe/Kyiv", label: "Europe/Kyiv (UTC+2/+3)" },
  { value: "Europe/Moscow", label: "Europe/Moscow (UTC+3)" },
  { value: "Europe/Istanbul", label: "Europe/Istanbul (UTC+3)" },
  { value: "Atlantic/Reykjavik", label: "Atlantic/Reykjavik (UTC+0)" },
  { value: "Atlantic/Azores", label: "Atlantic/Azores (UTC-1/+0)" },
  { value: "America/New_York", label: "America/New_York (EST/EDT, UTC-5/-4)" },
  { value: "America/Detroit", label: "America/Detroit (EST/EDT, UTC-5/-4)" },
  { value: "America/Indiana/Indianapolis", label: "America/Indiana/Indianapolis (EST/EDT, UTC-5/-4)" },
  { value: "America/Montreal", label: "America/Montreal (EST/EDT, UTC-5/-4)" },
  { value: "America/Toronto", label: "America/Toronto (EST/EDT, UTC-5/-4)" },
  { value: "America/Chicago", label: "America/Chicago (CST/CDT, UTC-6/-5)" },
  { value: "America/Mexico_City", label: "America/Mexico_City (CST, UTC-6)" },
  { value: "America/Denver", label: "America/Denver (MST/MDT, UTC-7/-6)" },
  { value: "America/Boise", label: "America/Boise (MST/MDT, UTC-7/-6)" },
  { value: "America/Phoenix", label: "America/Phoenix (MST, UTC-7, no DST)" },
  { value: "America/Los_Angeles", label: "America/Los_Angeles (PST/PDT, UTC-8/-7)" },
  { value: "America/Vancouver", label: "America/Vancouver (PST/PDT, UTC-8/-7)" },
  { value: "America/Anchorage", label: "America/Anchorage (AKST/AKDT, UTC-9/-8)" },
  { value: "America/Halifax", label: "America/Halifax (AST/ADT, UTC-4/-3)" },
  { value: "America/St_Johns", label: "America/St_Johns (NST/NDT, UTC-3:30/-2:30)" },
  { value: "Pacific/Honolulu", label: "Pacific/Honolulu (HST, UTC-10)" },
  { value: "America/Sao_Paulo", label: "America/Sao_Paulo (BRT, UTC-3)" },
  { value: "America/Buenos_Aires", label: "America/Buenos_Aires (ART, UTC-3)" },
  { value: "America/Santiago", label: "America/Santiago (CLT/CLST, UTC-4/-3)" },
  { value: "America/Bogota", label: "America/Bogota (COT, UTC-5)" },
  { value: "America/Lima", label: "America/Lima (PET, UTC-5)" },
  { value: "Africa/Casablanca", label: "Africa/Casablanca (WEST, UTC+1)" },
  { value: "Africa/Cairo", label: "Africa/Cairo (EET/EEST, UTC+2/+3)" },
  { value: "Africa/Johannesburg", label: "Africa/Johannesburg (SAST, UTC+2)" },
  { value: "Africa/Nairobi", label: "Africa/Nairobi (EAT, UTC+3)" },
  { value: "Africa/Lagos", label: "Africa/Lagos (WAT, UTC+1)" },
  { value: "Asia/Jerusalem", label: "Asia/Jerusalem (IST/IDT, UTC+2/+3)" },
  { value: "Asia/Riyadh", label: "Asia/Riyadh (AST, UTC+3)" },
  { value: "Asia/Dubai", label: "Asia/Dubai (GST, UTC+4)" },
  { value: "Asia/Tehran", label: "Asia/Tehran (IRST, UTC+3:30)" },
  { value: "Asia/Karachi", label: "Asia/Karachi (PKT, UTC+5)" },
  { value: "Asia/Kolkata", label: "Asia/Kolkata (IST, UTC+5:30)" },
  { value: "Asia/Dhaka", label: "Asia/Dhaka (BST, UTC+6)" },
  { value: "Asia/Bangkok", label: "Asia/Bangkok (ICT, UTC+7)" },
  { value: "Asia/Jakarta", label: "Asia/Jakarta (WIB, UTC+7)" },
  { value: "Asia/Singapore", label: "Asia/Singapore (SGT, UTC+8)" },
  { value: "Asia/Hong_Kong", label: "Asia/Hong_Kong (HKT, UTC+8)" },
  { value: "Asia/Shanghai", label: "Asia/Shanghai (CST, UTC+8)" },
  { value: "Asia/Taipei", label: "Asia/Taipei (CST, UTC+8)" },
  { value: "Asia/Manila", label: "Asia/Manila (PST, UTC+8)" },
  { value: "Asia/Tokyo", label: "Asia/Tokyo (JST, UTC+9)" },
  { value: "Asia/Seoul", label: "Asia/Seoul (KST, UTC+9)" },
  { value: "Australia/Sydney", label: "Australia/Sydney (AEST/AEDT, UTC+10/+11)" },
  { value: "Australia/Melbourne", label: "Australia/Melbourne (AEST/AEDT, UTC+10/+11)" },
  { value: "Australia/Brisbane", label: "Australia/Brisbane (AEST, UTC+10)" },
  { value: "Australia/Adelaide", label: "Australia/Adelaide (ACST/ACDT, UTC+9:30/+10:30)" },
  { value: "Australia/Perth", label: "Australia/Perth (AWST, UTC+8)" },
  { value: "Pacific/Guam", label: "Pacific/Guam (ChST, UTC+10)" },
  { value: "Pacific/Auckland", label: "Pacific/Auckland (NZST/NZDT, UTC+12/+13)" },
  { value: "Pacific/Fiji", label: "Pacific/Fiji (FJT, UTC+12)" },
  { value: "UTC", label: "UTC (Coordinated Universal Time)" }
];

export const THEME_ID_MAP = {
  "0": "Nintendo", "1": "Capcom", "2": "Taito", "3": "Sega",
  "4": "Cave", "5": "Konami", "6": "SNK", "7": "Technos",
  "8": "IGS", "9": "Hudson", "10": "Banpresto", "11": "Namco",
  "12": "Street Fighter (Ryu)", "13": "Super Mario", "14": "Metal Slug (Marco)",
  "15": "Mega Man", "16": "Space Invaders", "17": "Bubble Bobble (Bub)",
  "18": "Cyberpunk", "19": "Flip Clock", "20": "Custom Gradient",
  "21": "Matrix Rain", "22": "Pong Clock", "23": "Tetris Clock",
  "24": "Word Clock", "25": "Binary Clock", "26": "Pac-Man Clock",
  "27": "Versus Clock", "28": "Slot Machine Clock", "29": "Tetris Game Boy",
  "tetris": "Tetris Clock", "matrix": "Matrix Rain", "matrix_rain": "Matrix Rain",
  "versus": "Capcom Versus", "arcade": "Arcade", "pacman": "Pac-Man",
  "flip": "Flip Clock", "word": "Word Clock", "cyberpunk": "Cyberpunk",
  "pong": "Pong", "binary": "Binary", "slot_machine": "Slot Machine"
};

export function getEngineMeta(descOrMeta) {
  const meta = (descOrMeta && descOrMeta.metadata) ? descOrMeta.metadata : (descOrMeta || {});
  const id = String(meta.id || '').toLowerCase();
  const name = meta.name || (id ? (id.charAt(0).toUpperCase() + id.slice(1)) : 'Display Engine');
  const cat = String(meta.category || '').toLowerCase();
  
  let icon = meta.icon || '';
  if (!icon) {
    if (id === 'dashboard' || cat === 'dashboard') icon = '📊';
    else if (id.includes('clock') || cat.includes('time')) icon = '⏰';
    else if (id.includes('weather') || cat.includes('weather')) icon = '🌤️';
    else if (id.includes('crypto') || cat.includes('crypto')) icon = '🪙';
    else if (id.includes('stock') || cat.includes('stock') || cat.includes('finance')) icon = '📈';
    else if (id.includes('gif') || cat.includes('animation') || cat.includes('media_anim')) icon = '🎞️';
    else if (id.includes('sys') || cat.includes('system') || cat.includes('info')) icon = '🖥️';
    else if (id.includes('date') || cat.includes('calendar')) icon = '📅';
    else if (id.includes('spotify')) icon = '🟢';
    else if (id.includes('cast') || id.includes('music') || cat.includes('audio')) icon = '🎵';
    else if (id.includes('marquee')) icon = '📜';
    else if (id.includes('msg') || id.includes('message') || cat.includes('text')) icon = '💬';
    else if (cat.includes('game') || cat.includes('retro')) icon = '🕹️';
    else if (cat.includes('sensor')) icon = '🌡️';
    else icon = '🧩';
  }
  
  const desc = meta.description || `${name} display engine plugin for ArcadeMatrix.`;
  return { id, name, category: cat, icon, desc, version: meta.version || '1.0.0' };
}

// Global in-memory cache to prevent multiple round-trips
const endpointCache = new Map();

async function fetchCachedEndpoint(url) {
  if (!url) return [];
  if (endpointCache.has(url)) {
    return endpointCache.get(url);
  }
  const promise = API.get(url).catch(() => []);
  endpointCache.set(url, promise);
  return promise;
}

// Robust helper to extract schema fields on both ESP32 and RPi
export function getSchemaFields(engineDesc) {
  if (!engineDesc || !engineDesc.schema) return [];
  if (Array.isArray(engineDesc.schema.fields)) return engineDesc.schema.fields;
  if (Array.isArray(engineDesc.schema)) return engineDesc.schema;
  return [];
}

// Robust helper to parse options from array of objects, array of strings, or comma-separated string
export function parseOptions(rawOptions) {
  if (!rawOptions) return [];
  if (Array.isArray(rawOptions)) {
    return rawOptions.map(opt => {
      if (typeof opt === 'object' && opt !== null) {
        const val = opt.value !== undefined ? opt.value : (opt.id !== undefined ? opt.id : (opt.name || ''));
        const lbl = opt.label !== undefined ? opt.label : (opt.name !== undefined ? opt.name : val);
        return { value: String(val), label: String(lbl) };
      }
      return { value: String(opt), label: String(opt) };
    });
  }
  if (typeof rawOptions === 'string' && rawOptions.trim().length > 0) {
    return rawOptions.split(',').map(s => s.trim()).filter(s => s.length > 0).map(val => ({ value: val, label: val }));
  }
  return [];
}

// Type detection helpers
export function isBooleanField(f) {
  const t = f.field_type !== undefined ? f.field_type : f.type;
  return t === 0 || t === '0' || t === 'Boolean' || t === 'BOOLEAN' || t === 'boolean';
}

export function isNumberField(f) {
  const t = f.field_type !== undefined ? f.field_type : f.type;
  return t === 1 || t === 2 || t === 6 || t === '1' || t === '2' || t === '6' ||
         t === 'Integer' || t === 'INTEGER' || t === 'integer' ||
         t === 'Float' || t === 'FLOAT' || t === 'float' ||
         t === 'Duration' || t === 'DURATION' || t === 'duration';
}

export function isColorField(f) {
  const t = f.field_type !== undefined ? f.field_type : f.type;
  return t === 5 || t === '5' || t === 'Color' || t === 'COLOR' || t === 'color' || 
         (f.id && f.id.toLowerCase().includes('color'));
}

export function isOptionsField(f) {
  const t = f.field_type !== undefined ? f.field_type : f.type;
  return t === 4 || t === 7 || t === '4' || t === '7' ||
         t === 'Options' || t === 'OPTIONS' || t === 'options' ||
         t === 'Enum' || t === 'ENUM' || t === 'enum' ||
         t === 'List' || t === 'LIST' || t === 'list';
}

// Format options with localization
export function formatOptLabel(fieldId, val, raw) {
  const lang = localStorage.getItem('lang') || 'en';
  const dict = (typeof translations !== 'undefined' && translations && translations[lang]) ? translations[lang] : ((typeof translations !== 'undefined' && translations) ? translations.en : {});
  const v = String(val).trim();
  const lower = v.toLowerCase();

  // 1. Direction labels
  if (fieldId === 'direction' || ['rtl', 'ltr', 'ttb', 'btt', 'static', 'left', 'right', 'up', 'down', 'none'].includes(lower)) {
    if (lower === 'rtl' || lower === 'left') return dict.dir_rtl || "Right to Left (RTL)";
    if (lower === 'ltr' || lower === 'right') return dict.dir_ltr || "Left to Right (LTR)";
    if (lower === 'ttb' || lower === 'down') return dict.dir_ttb || "Top to Bottom (TTB)";
    if (lower === 'btt' || lower === 'up') return dict.dir_btt || "Bottom to Top (BTT)";
    if (lower === 'static' || lower === 'none') return dict.dir_static || "Static (No Scroll)";
  }
  // 2. Units
  if (fieldId === 'units' || fieldId === 'unit') {
    if (lower === 'c' || lower === 'metric') return "Celsius (°C)";
    if (lower === 'f' || lower === 'imperial') return "Fahrenheit (°F)";
  }
  // 3. Languages
  if (fieldId === 'lang') {
    if (lower === 'fr') return "Français (French)";
    if (lower === 'en') return "English";
    if (lower === 'es') return "Español (Spanish)";
    if (lower === 'de') return "Deutsch (German)";
    if (lower === 'it') return "Italiano (Italian)";
  }
  // 4. Timeframes
  if (fieldId === 'chart_timeframe') {
    if (lower === 'hourly') return "Hourly (1h)";
    if (lower === 'daily') return "Daily (24h)";
    if (lower === 'weekly') return "Weekly (7d)";
    if (lower === 'monthly') return "Monthly (30d)";
  }
  // 5. Currencies
  if (fieldId === 'currency') {
    if (lower === 'usd') return "US Dollar ($ USD)";
    if (lower === 'eur') return "Euro (€ EUR)";
    if (lower === 'gbp') return "British Pound (£ GBP)";
    if (lower === 'jpy') return "Japanese Yen (¥ JPY)";
  }
  // 6. Visualizer styles
  if (fieldId === 'style') {
    if (lower === 'spectrum') return "Spectrum Bars";
    if (lower === 'waveform') return "Waveform Oscilloscope";
    if (lower === 'radial') return "Radial Circular";
    if (lower === 'neon_fire') return "Neon Fire";
  }
  // 7. Date & Clock formats
  if (fieldId === 'date_format') {
    if (v === '%d/%m/%Y') return "DD/MM/YYYY (24/08/2026)";
    if (v === '%Y-%m-%d') return "YYYY-MM-DD (2026-08-24)";
    if (v === '%d %b %Y') return "DD Mon YYYY (24 Aug 2026)";
    if (v === '%A %d %B') return "Weekday DD Month (Monday 24 August)";
  }
  if (fieldId === 'clock_format' || fieldId === 'format') {
    if (v === '%H:%M:%S') return dict.opt_time_24s || "24h with Seconds (HH:MM:SS)";
    if (v === '%H:%M') return dict.opt_time_24 || "24h (HH:MM)";
    if (v === '%I:%M:%S %p') return dict.opt_time_12s_ampm || "12h with Seconds (HH:MM:SS AM/PM)";
    if (v === '%I:%M %p') return dict.opt_time_12_ampm || "12h (HH:MM AM/PM)";
    if (v === '%I:%M:%S') return dict.opt_time_12s || "12h with Seconds (HH:MM:SS)";
    if (v === '%I:%M') return dict.opt_time_12 || "12h (HH:MM)";
  }
  // 8. Theme names
  if (fieldId === 'clock_theme' || fieldId === 'theme') {
    const tName = THEME_ID_MAP[v] || raw || v;
    const themeKey = `theme_${tName.toLowerCase().replace(/[^a-z0-9]/g, '_')}`;
    return dict[themeKey] || tName;
  }
  return raw || val;
}

// --- Dynamic Screen Naming & Resolution (100% Schema-Driven) ---
export function resolveScreenInfo(instance, engineDesc) {
  const cfg = (instance && instance.config) ? instance.config : {};
  const meta = getEngineMeta(engineDesc || { id: instance?.engine_id });
  const icon = meta.icon;
  const engName = meta.name;
  
  if (cfg.screen_title && String(cfg.screen_title).trim().length > 0) {
    const custom = String(cfg.screen_title).trim();
    return { icon, title: custom, subtitle: engName, full: `${icon} ${custom}`, isCustom: true };
  }
  
  // Dynamically look for the most descriptive field in instance.config
  const fields = getSchemaFields(engineDesc);
  let bestValueLabel = '';
  
  if (fields && fields.length > 0) {
    const keyFieldNames = [
      'theme', 'clock_theme', 'style', 'mode', 'clock_mode',
      'city', 'location', 'folder', 'playlist', 'coins', 'symbols', 'symbol',
      'text', 'message', 'format', 'date_format'
    ];
    
    let chosenField = null;
    for (const key of keyFieldNames) {
      const match = fields.find(f => f.id === key || f.id.toLowerCase().includes(key));
      if (match && cfg[match.id] !== undefined && String(cfg[match.id]).trim().length > 0) {
        chosenField = match;
        break;
      }
    }
    
    if (!chosenField) {
      chosenField = fields.find(f => isOptionsField(f) && cfg[f.id] !== undefined && String(cfg[f.id]).trim().length > 0);
    }
    
    if (chosenField) {
      const rawVal = String(cfg[chosenField.id]).trim();
      if (chosenField.options) {
        const opts = parseOptions(chosenField.options);
        const matchOpt = opts.find(o => String(o.value) === rawVal);
        bestValueLabel = matchOpt ? matchOpt.label : formatOptLabel(chosenField.id, rawVal);
      } else {
        bestValueLabel = formatOptLabel(chosenField.id, rawVal);
      }
      if (bestValueLabel.length > 20) {
        bestValueLabel = bestValueLabel.substring(0, 17) + '...';
      }
    }
  }
  
  if (bestValueLabel) {
    return { icon, title: `${engName} (${bestValueLabel})`, subtitle: bestValueLabel, full: `${icon} ${engName} (${bestValueLabel})` };
  }
  
  const instId = instance?.instance_id || 'default';
  return { icon, title: `${engName} (${instId})`, subtitle: instId, full: `${icon} ${engName} (${instId})` };
}

window.switchToScreenTab = function(instanceId) {
  const cleanId = instanceId.replace(/[^a-zA-Z0-9]/g, '-');
  const tabId = `tab-dyn-${cleanId}`;
  
  if (typeof window.switchToPage === 'function') {
    window.switchToPage('display');
  }
  
  document.querySelectorAll('#page-display .tab-btn').forEach(btn => {
    btn.classList.remove('active');
    if (btn.getAttribute('data-tab-target') === tabId) {
      btn.classList.add('active');
      btn.scrollIntoView({ behavior: 'smooth', block: 'nearest', inline: 'center' });
    }
  });
  
  document.querySelectorAll('#page-display .tab-pane').forEach(pane => pane.classList.remove('active'));
  const targetPane = document.getElementById(tabId);
  if (targetPane) {
    targetPane.classList.add('active');
    targetPane.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }
};

// Modal for 1-Click Interactive Screen Creation (100% Schema-Driven)
export function showAddScreenModal(selectedEngineId, descriptors, instancesList, defaultAddToRotation = true) {
  const existingModal = document.getElementById('add-screen-modal-overlay');
  if (existingModal) existingModal.remove();

  const enginesMap = {};
  descriptors.forEach(d => { enginesMap[d.metadata.id] = d; });

  let currentEngineId = selectedEngineId || (descriptors.length > 0 ? descriptors[0].metadata.id : 'clock');
  let currentDesc = enginesMap[currentEngineId] || descriptors[0];

  const overlay = document.createElement('div');
  overlay.id = 'add-screen-modal-overlay';
  overlay.className = 'modal-overlay';

  function generateSuggestedId(engId, variant) {
    const cleanVariant = variant ? String(variant).toLowerCase().replace(/[^a-z0-9]/g, '_').replace(/^_+|_+$/g, '') : '';
    const base = cleanVariant ? `${engId}_${cleanVariant}` : `${engId}_1`;
    let candidate = base;
    let counter = 1;
    while (instancesList.some(i => i.instance_id === candidate)) {
      counter++;
      candidate = `${base}_${counter}`;
    }
    return candidate;
  }

  async function renderModalContent() {
    currentDesc = enginesMap[currentEngineId] || descriptors[0];
    const engId = currentEngineId.toLowerCase();
    const meta = getEngineMeta(currentDesc);
    const icon = meta.icon;
    const name = meta.name;
    const fields = getSchemaFields(currentDesc);

    let variantHtml = '';
    let initialVariant = '';

    if (engId === 'clock') {
      initialVariant = 'tetris';
      variantHtml = `
        <div class="form-group" style="margin-top: 1rem;">
          <label style="font-size: 0.85rem; font-weight: 600;" data-i18n="modal_choose_theme">${t('modal_choose_theme', 'Theme / Style / Variant:')}</label>
          <select id="modal-field-variant" class="input" style="font-weight: 600;">
            <option value="23" selected>🎮 Tetris Clock</option>
            <option value="21">💻 Matrix Rain Clock</option>
            <option value="27">🥊 Capcom / Versus Clock</option>
            <option value="26">🟡 Pac-Man Clock</option>
            <option value="19">⏱️ Flip Clock</option>
            <option value="24">🔤 Word Clock</option>
            <option value="18">🌆 Cyberpunk Clock</option>
            <option value="22">🏓 Pong Clock</option>
            <option value="25">🔢 Binary Clock</option>
            <option value="28">🎰 Slot Machine Clock</option>
          </select>
        </div>
      `;
    } else if (engId === 'weather') {
      initialVariant = 'paris';
      variantHtml = `
        <div class="form-group" style="margin-top: 1rem;">
          <label style="font-size: 0.85rem; font-weight: 600;" data-i18n="modal_city_label">${t('modal_city_label', 'City / Location (e.g. Paris, FR or Tokyo, JP):')}</label>
          <input type="text" id="modal-field-variant" class="input" placeholder="Paris, FR" value="Paris, FR">
          <div style="display: flex; gap: 0.35rem; flex-wrap: wrap; margin-top: 0.35rem; align-items: center;">
            <span style="font-size: 0.72rem; color: var(--text-muted); margin-right: 0.2rem;">⚡ ${t('quick_presets_label', 'Suggestions :')}</span>
            ${['Paris, FR', 'New York, US', 'Tokyo, JP', 'London, GB', 'Berlin, DE', 'Madrid, ES', 'Rome, IT', 'Los Angeles, US', 'Sydney, AU', 'Dubai, AE'].map(c => `
              <button type="button" class="btn btn-sm modal-quick-chip" data-val="${c}" style="font-size: 0.72rem; padding: 0.15rem 0.45rem; background: rgba(255,255,255,0.06); border-radius: 4px; border: 1px solid rgba(255,255,255,0.1); cursor: pointer;">${c.split(',')[0]}</button>
            `).join('')}
          </div>
        </div>
      `;
    } else if (engId === 'gifs' || engId === 'gif') {
      initialVariant = 'all';
      variantHtml = `
        <div class="form-group" style="margin-top: 1rem;">
          <label style="font-size: 0.85rem; font-weight: 600;" data-i18n="modal_gif_orientation_label">${t('modal_gif_orientation_label', 'GIF Orientation Library:')}</label>
          <div style="display: flex; gap: 0.5rem; margin-top: 0.25rem; margin-bottom: 0.5rem;">
            <button type="button" class="btn btn-secondary btn-sm" id="gif-tab-yoko" style="flex:1; border: 1px solid var(--accent); font-weight: 700;">🖥️ ${t('tab_yoko', 'Horizontal (Landscape / YOKO)')}</button>
            <button type="button" class="btn btn-secondary btn-sm" id="gif-tab-tate" style="flex:1; font-weight: 700; opacity: 0.7;">📱 ${t('tab_tate', 'Vertical (Portrait / TATE)')}</button>
          </div>
          <div style="font-size: 0.78rem; color: var(--text-muted); margin-bottom: 0.5rem;">💡 ${t('gif_modal_help', 'Selected folders or \'all\' will automatically switch according to the active rotation mode.')}</div>
          <label style="font-size: 0.85rem; font-weight: 600;" data-i18n="modal_gif_folder_label">${t('modal_gif_folder_label', 'Dossier / Playlist de GIFs :')}</label>
          <select id="modal-select-gif-folder" class="input" style="font-weight: 600; margin-bottom: 0.5rem;">
            <option value="all" selected>🌟 ${t('all_folders_opt', 'All folders (Auto Horizontal / Vertical)')}</option>
          </select>
          <input type="text" id="modal-field-variant" class="input" placeholder="all, /gifs/arcade, /gifs_tate/shmup" value="all">
        </div>
      `;
    } else if (engId === 'crypto') {
      initialVariant = 'btc';
      variantHtml = `
        <div class="form-group" style="margin-top: 1rem;">
          <label style="font-size: 0.85rem; font-weight: 600;" data-i18n="modal_crypto_label">${t('modal_crypto_label', 'Crypto Symbols (comma-separated):')}</label>
          <input type="text" id="modal-field-variant" class="input" placeholder="BTC,ETH,SOL" value="BTC,ETH">
          <div style="display: flex; gap: 0.35rem; flex-wrap: wrap; margin-top: 0.35rem; align-items: center;">
            <span style="font-size: 0.72rem; color: var(--text-muted); margin-right: 0.2rem;">⚡ ${t('quick_presets_label', 'Suggestions :')}</span>
            ${['BTC', 'ETH', 'SOL', 'DOGE', 'XRP', 'BNB', 'ADA', 'AVAX'].map(c => `
              <button type="button" class="btn btn-sm modal-quick-chip" data-val="${c}" style="font-size: 0.72rem; padding: 0.15rem 0.45rem; background: rgba(255,255,255,0.06); border-radius: 4px; border: 1px solid rgba(255,255,255,0.1); cursor: pointer;">${c}</button>
            `).join('')}
          </div>
        </div>
      `;
    } else if (engId === 'stock') {
      initialVariant = 'aapl';
      variantHtml = `
        <div class="form-group" style="margin-top: 1rem;">
          <label style="font-size: 0.85rem; font-weight: 600;" data-i18n="modal_stock_label">${t('modal_stock_label', 'Stock Tickers (comma-separated):')}</label>
          <input type="text" id="modal-field-variant" class="input" placeholder="AAPL,TSLA,NVDA" value="AAPL,TSLA">
          <div style="display: flex; gap: 0.35rem; flex-wrap: wrap; margin-top: 0.35rem; align-items: center;">
            <span style="font-size: 0.72rem; color: var(--text-muted); margin-right: 0.2rem;">⚡ ${t('quick_presets_label', 'Suggestions :')}</span>
            ${['AAPL', 'TSLA', 'NVDA', 'MSFT', 'AMZN', 'GOOGL', 'META'].map(s => `
              <button type="button" class="btn btn-sm modal-quick-chip" data-val="${s}" style="font-size: 0.72rem; padding: 0.15rem 0.45rem; background: rgba(255,255,255,0.06); border-radius: 4px; border: 1px solid rgba(255,255,255,0.1); cursor: pointer;">${s}</button>
            `).join('')}
          </div>
        </div>
      `;
    } else if (engId === 'dashboard') {
      initialVariant = '0';
      variantHtml = `
        <div class="form-group" style="margin-top: 1rem;">
          <label style="font-size: 0.85rem; font-weight: 600;" data-i18n="modal_choose_theme">${t('modal_choose_theme', 'Theme / Style / Variant:')}</label>
          <select id="modal-field-variant" class="input" style="font-weight: 600;">
            <option value="0" selected>🌆 Cyberpunk Neon</option>
            <option value="1">📟 Amber HUD / Tactical</option>
            <option value="2">❄️ Minimalist Luxury</option>
            <option value="3">💻 Matrix Phosphor</option>
          </select>
        </div>
      `;
    } else {
      // Dynamic fallback for any schema
      const primaryField = (fields && fields.length > 0) ? (
        fields.find(f => isOptionsField(f) || (f.options && f.options.length > 0) || f.options_endpoint) ||
        fields.find(f => !isBooleanField(f) && !isNumberField(f) && !isColorField(f))
      ) : null;

      if (primaryField) {
        const fLabel = primaryField.label || primaryField.id;
        if (isOptionsField(primaryField) || (primaryField.options && primaryField.options.length > 0)) {
          const opts = parseOptions(primaryField.options);
          initialVariant = primaryField.default_value || (opts.length > 0 ? opts[0].value : '');
          const optsHtml = opts.map(o => `<option value="${o.value}" ${String(o.value) === String(initialVariant) ? 'selected' : ''}>${formatOptLabel(primaryField.id, o.value, o.label)}</option>`).join('');
          variantHtml = `
            <div class="form-group" style="margin-top: 1rem;">
              <label style="font-size: 0.85rem; font-weight: 600;">${fLabel}:</label>
              <select id="modal-field-variant" class="input" style="font-weight: 600;">${optsHtml}</select>
            </div>
          `;
        } else {
          initialVariant = primaryField.default_value || '';
          variantHtml = `
            <div class="form-group" style="margin-top: 1rem;">
              <label style="font-size: 0.85rem; font-weight: 600;">${fLabel}:</label>
              <input type="text" id="modal-field-variant" class="input" placeholder="${primaryField.description || primaryField.default_value || ''}" value="${initialVariant}">
            </div>
          `;
        }
      }
    }

    const defaultId = generateSuggestedId(engId, initialVariant);

    overlay.innerHTML = `
      <div class="modal-box glass">
        <div class="modal-header">
          <div class="modal-title">
            <span>➕</span> <span data-i18n="modal_add_title">${t('modal_add_title', 'Add New Display Screen')}</span>
          </div>
          <button class="modal-close" id="modal-btn-close">✕</button>
        </div>
        
        <div class="modal-body">
          <div class="form-group">
            <label style="font-size: 0.85rem; font-weight: 600; color: var(--accent-primary);" data-i18n="choose_engine">${t('choose_engine', 'Choose Engine:')}</label>
            <select id="modal-select-engine" class="input" style="font-weight: 600; font-size: 1rem; border-color: var(--accent-primary);">
              ${descriptors.map(d => {
                const dMeta = getEngineMeta(d);
                const isSel = d.metadata.id === currentEngineId ? 'selected' : '';
                return `<option value="${d.metadata.id}" ${isSel}>${dMeta.icon} ${dMeta.name}</option>`;
              }).join('')}
            </select>
          </div>

          ${variantHtml}

          <div class="form-group" style="margin-top: 1rem;">
            <label style="font-size: 0.85rem; font-weight: 600;" data-i18n="screen_name_label">${t('screen_name_label', 'Screen Title / Label:')}</label>
            <input type="text" id="modal-input-title" class="input" placeholder="e.g. Living Room Clock" value="${name}">
          </div>

          <div class="form-group" style="margin-top: 0.75rem;">
            <label style="font-size: 0.8rem; font-weight: 600; color: var(--text-muted);" data-i18n="instance_name_id">${t('instance_name_id', 'Unique Instance ID:')}</label>
            <input type="text" id="modal-input-id" class="input" style="font-size: 0.85rem; opacity: 0.85;" value="${defaultId}">
          </div>

          <div style="background: rgba(0,0,0,0.25); border-radius: var(--radius-md); padding: 1rem; margin-top: 1.25rem; border: 1px solid var(--glass-border);">
            <label class="switch" style="display: flex; align-items: center; gap: 0.5rem; cursor: pointer;">
              <input type="checkbox" id="modal-check-rotation" ${defaultAddToRotation ? 'checked' : ''}>
              <span style="font-weight: 600; font-size: 0.9rem;" data-i18n="modal_add_rotation_check">${t('modal_add_rotation_check', 'Insert directly into rotation loop')}</span>
            </label>
            <div id="modal-rotation-options" style="margin-top: 0.75rem; display: ${defaultAddToRotation ? 'flex' : 'none'}; gap: 1rem; align-items: center; flex-wrap: wrap;">
              <div style="display: flex; align-items: center; gap: 0.5rem;">
                <span style="font-size: 0.85rem; color: var(--text-secondary);" data-i18n="modal_duration_label">${t('modal_duration_label', 'Display Duration (seconds / count):')}</span>
                <input type="number" id="modal-input-duration" class="input" style="width: 70px; padding: 0.35rem 0.5rem; text-align: center;" value="30" min="1">
                <span style="font-size: 0.85rem; color: var(--text-muted);">${(engId === 'gifs' || engId === 'gif') ? 'GIFs' : 'sec'}</span>
              </div>
              ${(currentDesc.capabilities && currentDesc.capabilities.allows_overlay !== false) ? `
                <label style="display: flex; align-items: center; gap: 0.35rem; font-size: 0.85rem; cursor: pointer;">
                  <input type="checkbox" id="modal-check-fighter" ${engId === 'clock' ? 'checked' : ''}>
                  <span data-i18n="fighter_overlay_label">${t('fighter_overlay_label', '🥊 Street Fighter Overlay')}</span>
                </label>
              ` : ''}
            </div>
          </div>
        </div>

        <div class="modal-footer">
          <button class="btn" id="modal-btn-cancel" style="background: rgba(255,255,255,0.08); color:#fff;" data-i18n="cancel_btn">${t('cancel_btn', 'Cancel')}</button>
          <button class="btn btn-primary" id="modal-btn-submit" style="font-weight: 700; box-shadow: 0 0 15px var(--accent-glow);" data-i18n="modal_create_btn">
            ${t('modal_create_btn', '🚀 Create & Add Screen')}
          </button>
        </div>
      </div>
    `;

    // Wire events
    overlay.querySelector('#modal-btn-close').onclick = () => overlay.remove();
    overlay.querySelector('#modal-btn-cancel').onclick = () => overlay.remove();

    const engSelect = overlay.querySelector('#modal-select-engine');
    engSelect.onchange = () => {
      currentEngineId = engSelect.value;
      renderModalContent();
    };

    const variantEl = overlay.querySelector('#modal-field-variant');
    const idInput = overlay.querySelector('#modal-input-id');
    const titleInput = overlay.querySelector('#modal-input-title');

    // Wire modal quick-select chips
    overlay.querySelectorAll('.modal-quick-chip').forEach(chip => {
      chip.onclick = () => {
        const val = chip.getAttribute('data-val');
        if (variantEl) {
          if (engId === 'crypto' || engId === 'stock') {
            const currentTokens = variantEl.value.split(',').map(s => s.trim()).filter(s => s.length > 0);
            const idx = currentTokens.findIndex(t => t.toUpperCase() === val.toUpperCase());
            if (idx >= 0) currentTokens.splice(idx, 1);
            else currentTokens.push(val);
            variantEl.value = currentTokens.join(',');
          } else {
            variantEl.value = val;
          }
          variantEl.dispatchEvent(new Event('change'));
        }
      };
    });

    if (variantEl && idInput) {
      variantEl.oninput = variantEl.onchange = () => {
        const val = variantEl.value;
        idInput.value = generateSuggestedId(engId, val);
        let optText = val;
        if (variantEl.tagName === 'SELECT' && variantEl.selectedIndex >= 0) {
          optText = variantEl.options[variantEl.selectedIndex].text.replace(/^[^\w\s]+/, '').trim();
        }
        titleInput.value = optText ? `${name} (${optText})` : name;
      };
    }

    // Specialized GIF Playlist Loader and TATE / YOKO tab handling in Modal
    const tabYoko = overlay.querySelector('#gif-tab-yoko');
    const tabTate = overlay.querySelector('#gif-tab-tate');
    const gifSelect = overlay.querySelector('#modal-select-gif-folder');
    if (gifSelect && variantEl) {
      let cachedPlaylists = [];
      fetchCachedEndpoint('/api/playlists').then(data => {
        cachedPlaylists = parseOptions(data);
        populateGifSelect('yoko');
      }).catch(() => {});

      function populateGifSelect(orientation) {
        gifSelect.innerHTML = `<option value="all" selected>🌟 ${t('all_folders_opt', 'All folders (Auto Horizontal / Vertical)')}</option>`;
        const filtered = cachedPlaylists.filter(p => p.orientation === orientation || (orientation === 'yoko' ? !p.value.includes('tate') : p.value.includes('tate')));
        filtered.forEach(p => {
          const opt = document.createElement('option');
          opt.value = p.value;
          const cnt = (p.count !== undefined && p.count > 0) ? ` (${p.count})` : '';
          opt.textContent = `${p.label}${cnt}`;
          gifSelect.appendChild(opt);
        });
      }

      gifSelect.onchange = () => {
        variantEl.value = gifSelect.value;
        if (idInput) idInput.value = generateSuggestedId('gifs', gifSelect.value);
        if (titleInput) titleInput.value = `${name} (${gifSelect.options[gifSelect.selectedIndex].text.replace(/^[^\w\s]+/, '').trim()})`;
      };

      if (tabYoko && tabTate) {
        tabYoko.onclick = () => {
          tabYoko.style.border = '1px solid var(--accent)';
          tabYoko.style.opacity = '1';
          tabTate.style.border = 'none';
          tabTate.style.opacity = '0.7';
          populateGifSelect('yoko');
          variantEl.value = 'all';
          variantEl.placeholder = "all, /gifs/arcade, /gifs/nintendo";
          if (idInput) idInput.value = generateSuggestedId('gifs', 'all');
        };
        tabTate.onclick = () => {
          tabTate.style.border = '1px solid var(--accent)';
          tabTate.style.opacity = '1';
          tabYoko.style.border = 'none';
          tabYoko.style.opacity = '0.7';
          populateGifSelect('tate');
          variantEl.value = 'all';
          variantEl.placeholder = "all, /gifs_tate/arcade, /gifs_tate/shmup";
          if (idInput) idInput.value = generateSuggestedId('gifs', 'all');
        };
      }
    }

    const rotCheck = overlay.querySelector('#modal-check-rotation');
    const rotOpts = overlay.querySelector('#modal-rotation-options');
    rotCheck.onchange = () => {
      rotOpts.style.display = rotCheck.checked ? 'flex' : 'none';
    };

    const submitBtn = overlay.querySelector('#modal-btn-submit');
    submitBtn.onclick = async () => {
      const instId = idInput.value.trim() || generateSuggestedId(engId, '');
      const screenTitle = titleInput.value.trim();
      const initialConfig = {};
      if (screenTitle) initialConfig.screen_title = screenTitle;

      // Populate default schema values
      if (fields && fields.length > 0) {
        fields.forEach(f => {
          if (f.default_value !== undefined && f.default_value !== null) {
            initialConfig[f.id] = f.default_value;
          }
        });
      }

      if (variantEl) {
        if (engId === 'clock') {
          initialConfig.theme = parseInt(variantEl.value) || 23;
        } else if (engId === 'weather') {
          initialConfig.city = variantEl.value || 'Paris, FR';
        } else if (engId === 'gifs' || engId === 'gif') {
          initialConfig.folder = variantEl.value || 'all';
        } else if (engId === 'crypto') {
          initialConfig.symbols = variantEl.value || 'BTC,ETH';
        } else if (engId === 'stock') {
          initialConfig.symbols = variantEl.value || 'AAPL,TSLA';
        } else if (engId === 'dashboard') {
          initialConfig.theme = parseInt(variantEl.value) || 0;
        }
      }

      submitBtn.disabled = true;
      submitBtn.innerText = t('creating_btn', 'Creating...');

      try {
        await API.post('/api/instances', {
          instance_id: instId,
          engine_id: currentEngineId,
          config: initialConfig
        });

        if (rotCheck.checked) {
          const curRot = await API.get('/api/rotation').catch(() => []);
          const newRot = Array.isArray(curRot) ? curRot.slice() : [];
          const dur = parseInt(overlay.querySelector('#modal-input-duration')?.value) || 30;
          const fighter = overlay.querySelector('#modal-check-fighter')?.checked ?? false;
          
          newRot.push({
            instance_id: instId,
            duration_sec: dur,
            overlays: { fighter },
            fighter_overlay: fighter
          });
          await API.post('/api/rotation', newRot);
        }

        overlay.remove();
        window.showToast(t('preview_success', 'Screen created successfully!'), 'success');
        await renderDynamicDisplay(instId);
      } catch (e) {
        console.error('Failed to create screen:', e);
        window.showToast('Failed to create screen: ' + (e.message || ''), 'error');
        submitBtn.disabled = false;
        submitBtn.innerText = t('modal_create_btn', '🚀 Create & Add Screen');
      }
    };
  }

  renderModalContent();
  document.body.appendChild(overlay);
}

// 1. Render Rotation Playlist Panel
export function renderRotationPanel(container, insertionPoint, rotationList, instancesList, enginesMap, descriptors) {
  let entries = Array.isArray(rotationList) ? rotationList.slice() : [];

  const nameFor = (instId) => {
    const inst = instancesList.find(i => i.instance_id === instId);
    if (!inst) return instId;
    const eng = enginesMap[inst.engine_id];
    const info = resolveScreenInfo(inst, eng);
    return info.full;
  };

  const isCountBased = (instId) => {
    const inst = instancesList.find(i => i.instance_id === instId);
    const eng = inst ? enginesMap[inst.engine_id] : null;
    return Boolean((eng && eng.capabilities && eng.capabilities.selfPaced) || (inst && inst.engine_id === 'gifs'));
  };

  const allowsOverlay = (instId) => {
    const inst = instancesList.find(i => i.instance_id === instId);
    const eng = inst ? enginesMap[inst.engine_id] : null;
    return Boolean(!eng || !eng.capabilities || eng.capabilities.allows_overlay !== false);
  };

  const card = document.createElement('div');
  card.id = 'rotation-playlist-card';
  card.className = 'card glass shadow';
  card.style = 'margin: 1rem 0 2rem 0; box-sizing: border-box; max-width: 100%;';

  card.innerHTML = `
    <div class="card-header-badge" style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.5rem; flex-wrap: wrap; gap: 0.5rem;">
      <h3 style="margin: 0; font-size: 1.25rem;" data-i18n="rotation_loop_title">🔁 ${t('rotation_loop_title', 'Screen Rotation Loop (Matrix Playlist)')}</h3>
      <span class="badge badge-accent" data-i18n="rotation_loop_badge">${t('rotation_loop_badge', 'Auto-Cycling')}</span>
    </div>
    <p class="text-muted" style="margin: 0 0 1.25rem 0; font-size: 0.88rem;" data-i18n="rotation_loop_desc">
      ${t('rotation_loop_desc', 'This sequence controls which screens cycle on your LED Matrix, how long each screen stays active (seconds / GIF count), their display order, and whether the Street Fighter animation overlay is enabled.')}
    </p>
  `;

  const list = document.createElement('div');
  list.className = 'rotation-list';
  card.appendChild(list);

  function renderRows() {
    list.innerHTML = '';
    entries.forEach((entry, idx) => {
      const row = document.createElement('div');
      row.className = 'rotation-row';
      row.id = `rotation-row-${entry.instance_id}`;

      const header = document.createElement('div');
      header.className = 'rotation-row-header';

      const badge = document.createElement('span');
      badge.className = 'rotation-order-badge';
      badge.innerText = `#${idx + 1}`;
      header.appendChild(badge);

      const label = document.createElement('span');
      label.className = 'rotation-title';
      label.id = `rot-title-${entry.instance_id}`;
      label.innerText = nameFor(entry.instance_id);
      header.appendChild(label);

      row.appendChild(header);

      const settingsBar = document.createElement('div');
      settingsBar.className = 'rotation-settings-bar';

      const countBased = isCountBased(entry.instance_id);

      const durGroup = document.createElement('div');
      durGroup.className = 'rotation-dur-group';
      durGroup.title = countBased ? t('dur_tooltip_gifs', 'Number of GIFs to play before rotating') : t('dur_tooltip_sec', 'Duration in seconds');

      const durLbl = document.createElement('span');
      durLbl.style = "font-size: 0.8rem; color: var(--text-secondary);";
      durLbl.innerText = countBased ? 'Count:' : 'Duration:';
      durGroup.appendChild(durLbl);

      const dur = document.createElement('input');
      dur.type = 'number';
      dur.className = 'input rotation-dur-input';
      dur.min = '1';
      dur.value = entry.duration_sec;
      dur.onchange = () => { entry.duration_sec = parseInt(dur.value) || 1; };
      durGroup.appendChild(dur);

      const unit = document.createElement('span');
      unit.style = "font-size: 0.8rem; color: var(--text-muted);";
      unit.innerText = countBased ? 'GIFs' : 'sec';
      durGroup.appendChild(unit);
      settingsBar.appendChild(durGroup);

      const fightLbl = document.createElement('label');
      fightLbl.className = 'rotation-fighter-lbl';
      fightLbl.title = t('fighter_tooltip', 'Show Street Fighter battle animation overlay on this screen');
      const fightCb = document.createElement('input');
      fightCb.type = 'checkbox';
      const isFighter = (entry.overlays && typeof entry.overlays.fighter === 'boolean')
        ? entry.overlays.fighter
        : (entry.fighter_overlay === true);
      fightCb.checked = isFighter;
      fightCb.onchange = () => {
        if (!entry.overlays) entry.overlays = {};
        entry.overlays.fighter = fightCb.checked;
        entry.fighter_overlay = fightCb.checked;
      };
      fightLbl.appendChild(fightCb);
      const fightTxt = document.createElement('span');
      fightTxt.innerText = `🥊 ${t('fighter_title', 'Overlay')}`;
      fightLbl.appendChild(fightTxt);
      settingsBar.appendChild(fightLbl);

      // Direct settings button to jump to the instance tab
      const editBtn = document.createElement('button');
      editBtn.type = 'button';
      editBtn.className = 'btn btn-icon';
      editBtn.style = "background: rgba(99, 102, 241, 0.2); color: #818cf8; border-color: rgba(99, 102, 241, 0.4); font-weight: 600;";
      editBtn.title = t('jump_to_settings', 'Jump to this screen settings');
      editBtn.innerText = t('jump_to_settings', '⚙️ Settings');
      editBtn.onclick = () => window.switchToScreenTab(entry.instance_id);
      settingsBar.appendChild(editBtn);

      const actions = document.createElement('div');
      actions.className = 'rotation-actions';

      const up = document.createElement('button');
      up.type = 'button';
      up.className = 'btn btn-icon';
      up.title = t('move_up_title', 'Move Up');
      up.innerText = '▲';
      up.disabled = idx === 0;
      up.onclick = () => { entries.splice(idx - 1, 0, entries.splice(idx, 1)[0]); renderRows(); };
      actions.appendChild(up);

      const down = document.createElement('button');
      down.type = 'button';
      down.className = 'btn btn-icon';
      down.title = t('move_down_title', 'Move Down');
      down.innerText = '▼';
      down.disabled = idx === entries.length - 1;
      down.onclick = () => { entries.splice(idx + 1, 0, entries.splice(idx, 1)[0]); renderRows(); };
      actions.appendChild(down);

      const del = document.createElement('button');
      del.type = 'button';
      del.className = 'btn btn-icon btn-danger';
      del.title = t('remove_from_loop_title', 'Remove from rotation loop');
      del.innerText = '🗑️';
      del.onclick = () => { entries.splice(idx, 1); renderRows(); };
      actions.appendChild(del);

      settingsBar.appendChild(actions);
      row.appendChild(settingsBar);
      list.appendChild(row);
    });

    if (entries.length === 0) {
      const empty = document.createElement('p');
      empty.className = 'text-muted';
      empty.style.padding = '0.75rem 0';
      empty.setAttribute('data-i18n', 'rotation_empty');
      empty.innerText = t('rotation_empty', 'No screens in rotation. Pick an engine above or below to add one.');
      list.appendChild(empty);
    }
  }

  // Quick Add Row with Optgroups
  const addCard = document.createElement('div');
  addCard.style = "margin-top: 1rem; padding-top: 1rem; border-top: 1px solid var(--glass-border);";

  const addRow = document.createElement('div');
  addRow.style = "display: flex; gap: 0.75rem; align-items: center; flex-wrap: wrap;";

  const sel = document.createElement('select');
  sel.className = 'input';
  sel.style = "flex: 1; min-width: 260px; font-weight: 500;";

  // Group 1: Configured Screens
  if (instancesList.length > 0) {
    const grpConfigured = document.createElement('optgroup');
    grpConfigured.label = t('optgroup_configured', '📁 Configured Screens');
    instancesList.forEach(inst => {
      const eng = enginesMap[inst.engine_id];
      const info = resolveScreenInfo(inst, eng);
      const opt = document.createElement('option');
      opt.value = `inst:${inst.instance_id}`;
      opt.innerText = info.full;
      grpConfigured.appendChild(opt);
    });
    sel.appendChild(grpConfigured);
  }

  // Group 2: Engine Library
  const grpLibrary = document.createElement('optgroup');
  grpLibrary.label = t('optgroup_library', '🌟 Engine Library (Auto-Create & Add)');
  descriptors.forEach(d => {
    const icon = (d.metadata && d.metadata.icon) ? d.metadata.icon : '🧩';
    const name = (d.metadata && d.metadata.name) ? d.metadata.name : d.metadata.id;
    const opt = document.createElement('option');
    opt.value = `new:${d.metadata.id}`;
    opt.innerText = `${icon} ${name} (${t('modal_add_title', 'New Screen')})`;
    grpLibrary.appendChild(opt);
  });
  sel.appendChild(grpLibrary);

  const addBtn = document.createElement('button');
  addBtn.type = 'button';
  addBtn.className = 'btn btn-primary';
  addBtn.style = "padding: 0.75rem 1.25rem; font-weight: 600; white-space: nowrap;";
  addBtn.innerText = `➕ ${t('add_to_rotation_btn', 'Add to Loop')}`;
  addBtn.onclick = () => {
    const val = sel.value;
    if (!val) return;
    if (val.startsWith('inst:')) {
      const instId = val.replace('inst:', '');
      const inst = instancesList.find(i => i.instance_id === instId);
      const allowOverlay = allowsOverlay(instId);
      entries.push({
        instance_id: instId,
        duration_sec: 30,
        overlays: { fighter: allowOverlay },
        fighter_overlay: allowOverlay
      });
      renderRows();
    } else if (val.startsWith('new:')) {
      const engId = val.replace('new:', '');
      showAddScreenModal(engId, descriptors, instancesList, true);
    }
  };

  const saveBtn = document.createElement('button');
  saveBtn.type = 'button';
  saveBtn.className = 'btn btn-primary';
  saveBtn.style = "margin-top: 1.25rem; width: 100%; padding: 0.85rem; font-size: 1rem; font-weight: 700; box-shadow: 0 0 15px var(--accent-glow);";
  saveBtn.setAttribute('data-i18n', 'rotation_save_btn');
  saveBtn.innerText = `💾 ${t('rotation_save_btn', 'Save Rotation Playlist')}`;
  saveBtn.onclick = async () => {
    const payload = entries.map(e => {
      const active = (e.overlays && typeof e.overlays.fighter === 'boolean')
        ? e.overlays.fighter
        : (e.fighter_overlay === true);
      return {
        instance_id: e.instance_id,
        duration_sec: parseInt(e.duration_sec) || 1,
        overlays: { fighter: active },
        fighter_overlay: active,
      };
    });
    try {
      await API.post('/api/rotation', payload);
      window.showToast('Rotation playlist saved successfully!', 'success');
    } catch (e) {
      window.showToast('Failed to save rotation playlist', 'warning');
    }
  };

  addRow.appendChild(sel);
  addRow.appendChild(addBtn);
  addCard.appendChild(addRow);
  card.appendChild(addCard);
  card.appendChild(saveBtn);

  renderRows();
  container.insertBefore(card, insertionPoint);
}

// 2. Render Visual Engine Catalog (Library) on Dashboard Hub
export function renderEngineCatalog(descriptors, instancesList, enginesMap) {
  const dashboardContainer = document.getElementById('dashboard-catalog-container');
  if (!dashboardContainer) return;
  dashboardContainer.innerHTML = '';

  const catalogWrapper = document.createElement('div');
  catalogWrapper.id = 'engine-catalog-section';

  const catalogHeader = document.createElement('div');
  catalogHeader.style = "margin: 2.5rem 0 1rem 0; display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 0.5rem;";
  catalogHeader.innerHTML = `
    <div>
      <h3 style="margin: 0; font-size: 1.25rem;" data-i18n="engine_catalog_title">📚 ${t('engine_catalog_title', 'Plugin & Engine Library')}</h3>
      <p class="text-muted" style="margin: 0.25rem 0 0 0; font-size: 0.85rem;" data-i18n="engine_catalog_desc">${t('engine_catalog_desc', 'All display engines available on your device. Click Add to Loop to launch a new screen on your matrix.')}</p>
    </div>
    <span class="badge badge-accent">${descriptors.length} ${t('engines_available', 'Engines Available')}</span>
  `;
  catalogWrapper.appendChild(catalogHeader);

  const grid = document.createElement('div');
  grid.className = 'catalog-grid';

  descriptors.forEach(desc => {
    const eng = desc.metadata || {};
    const req = desc.requirements || {};
    const cap = desc.capabilities || {};
    const meta = getEngineMeta(desc);
    const icon = meta.icon;
    const name = meta.name;
    const descText = meta.desc;

    const card = document.createElement('div');
    card.className = 'engine-catalog-card';

    let tagsHtml = '';
    if (req.needsPsram) tagsHtml += `<span class="engine-tag psram">${t('tag_psram', '💾 PSRAM')}</span>`;
    if (req.needsAudio) tagsHtml += `<span class="engine-tag sensor">${t('tag_audio', '🎙️ Micro I2S')}</span>`;
    if (req.needsTempSensor) tagsHtml += `<span class="engine-tag sensor">${t('tag_temp', '🌡️ Capteur Temp')}</span>`;
    if (cap.realtime) tagsHtml += `<span class="engine-tag realtime">${t('tag_realtime', '⚡ Temps Réel')}</span>`;

    const existingCount = instancesList.filter(i => i.engine_id === eng.id).length;
    const existingBadge = existingCount > 0 ? `<span class="badge" style="background: rgba(16,185,129,0.2); color:#34d399; font-size: 0.75rem;">${existingCount} ${t('screens_active', 'active screen(s)')}</span>` : '';

    card.innerHTML = `
      <div>
        <div class="engine-card-header">
          <div class="engine-card-icon">${icon}</div>
          <div>
            <div class="engine-card-title">${name}</div>
            <div style="font-size: 0.75rem; color: var(--text-muted); font-family: monospace;">${eng.id}</div>
          </div>
        </div>
        <div class="engine-card-desc">${descText}</div>
        <div class="engine-card-tags">
          ${tagsHtml}
          ${existingBadge}
        </div>
      </div>
      <div class="engine-card-actions">
        <button class="btn btn-primary btn-add-loop" style="font-weight:600;">➕ ${t('add_to_rotation_btn', 'Add to Loop')}</button>
        <button class="btn btn-config" style="background: rgba(255,255,255,0.08); color: #fff;">⚙️ ${t('configure_btn', 'Configure')}</button>
      </div>
    `;

    card.querySelector('.btn-add-loop').onclick = () => {
      showAddScreenModal(eng.id, descriptors, instancesList, true);
    };

    card.querySelector('.btn-config').onclick = () => {
      const existing = instancesList.find(i => i.engine_id === eng.id);
      if (existing) {
        window.switchToScreenTab(existing.instance_id);
      } else {
        showAddScreenModal(eng.id, descriptors, instancesList, false);
      }
    };

    grid.appendChild(card);
  });

  catalogWrapper.appendChild(grid);
  dashboardContainer.appendChild(catalogWrapper);
}

// Helper: Build Rich Multi-Select Group with YOKO / TATE Tabs for Folder / GIF fields
function buildMultiSelectGroup(opts, targetFieldId, instId, initVal) {
  const container = document.createElement('div');
  container.style = 'display: flex; flex-direction: column; gap: 0.5rem; width: 100%;';

  const isFolderField = (targetFieldId === 'folder' || targetFieldId === 'playlists' || targetFieldId === 'gif_folder' || targetFieldId === 'folders' || opts.some(o => String(o.value).startsWith('/gifs')));
  const initialTokens = String(initVal || 'all').split(',').map(s => s.trim().toUpperCase()).filter(s => s.length > 0);

  const textInput = document.createElement('input');
  textInput.type = 'text';
  textInput.className = 'input';
  textInput.id = `cfg-dyn-${instId}-${targetFieldId}`;
  textInput.value = initVal || '';
  textInput.placeholder = isFolderField ? 'all, /gifs/arcade, /gifs_tate/shmup...' : 'all, item1, item2...';

  function syncTagsFromInput() {
    const activeTokens = textInput.value.split(',').map(s => s.trim().toUpperCase()).filter(s => s.length > 0);
    const isAll = activeTokens.includes('ALL');
    const cbs = container.querySelectorAll(`input[type="checkbox"].multi-cb-${instId}-${targetFieldId}`);
    cbs.forEach(cb => {
      const cbVal = String(cb.value).trim().toUpperCase();
      cb.checked = isAll || activeTokens.includes(cbVal) || activeTokens.some(t => cbVal.endsWith('/' + t) || t.endsWith('/' + cbVal));
    });
  }

  function syncInputFromTags() {
    const currentTokens = textInput.value.split(',').map(s => s.trim()).filter(s => s.length > 0 && s.toUpperCase() !== 'ALL');
    const knownTagVals = opts.map(o => String(o.value).trim().toUpperCase());
    const customTokens = currentTokens.filter(tok => !knownTagVals.includes(tok.toUpperCase()));
    
    const checkedTagVals = Array.from(container.querySelectorAll(`input[type="checkbox"].multi-cb-${instId}-${targetFieldId}:checked`)).map(cb => cb.value);
    const allTokens = [...checkedTagVals, ...customTokens];
    textInput.value = (allTokens.length === 0) ? 'all' : allTokens.join(',');
    textInput.dispatchEvent(new Event('input', { bubbles: true }));
  }

  if (isFolderField) {
    const yokoOpts = opts.filter(o => o.orientation === 'yoko' || (!o.orientation && !o.value.includes('tate') && !o.label.includes('tate')));
    const tateOpts = opts.filter(o => o.orientation === 'tate' || o.value.includes('tate') || o.label.includes('tate'));

    // Helper info card
    const infoCard = document.createElement('div');
    infoCard.style = 'background: rgba(0, 229, 255, 0.08); border-left: 3px solid var(--accent-primary, #00e5ff); padding: 0.6rem 0.8rem; border-radius: 4px; font-size: 0.82rem; line-height: 1.35; color: var(--text-secondary);';
    infoCard.innerHTML = `<strong>💡 ${t('gif_auto_switch_title', 'Auto-Switching Detection :')}</strong> ${t('gif_auto_switch_desc', 'ArcadeMatrix automatically switches: in Horizontal (Landscape / YOKO) mode, only horizontal GIF folders are played. In Vertical (Portrait / TATE) mode, only vertical GIF folders are played.')}`;
    container.appendChild(infoCard);

    // Tab switcher for orientations
    const tabNav = document.createElement('div');
    tabNav.style = 'display: flex; gap: 0.5rem; margin-top: 0.25rem;';
    tabNav.innerHTML = `
      <button type="button" class="btn btn-secondary btn-sm" id="btn-tab-yoko-${instId}" style="flex:1; border: 1px solid var(--accent-primary, #00e5ff); font-weight: 700;">🖥️ ${t('tab_yoko', 'Horizontal (Landscape / YOKO)')} <span class="badge" style="margin-left: 4px; opacity: 0.8;">${yokoOpts.length}</span></button>
      <button type="button" class="btn btn-secondary btn-sm" id="btn-tab-tate-${instId}" style="flex:1; font-weight: 700; opacity: 0.7;">📱 ${t('tab_tate', 'Vertical (Portrait / TATE)')} <span class="badge" style="margin-left: 4px; opacity: 0.8;">${tateOpts.length}</span></button>
    `;
    container.appendChild(tabNav);

    // Container for YOKO
    const yokoContainer = document.createElement('div');
    yokoContainer.id = `pane-yoko-${instId}`;
    yokoContainer.style = 'display: flex; flex-direction: column; gap: 0.4rem;';

    const yokoActions = document.createElement('div');
    yokoActions.style = 'display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.2rem;';
    yokoActions.innerHTML = `
      <span style="font-size: 0.8rem; font-weight: 600; color: var(--text-muted);">🖥️ /gifs/ (${yokoOpts.length} ${t('folders_count', 'folders')})</span>
      <div style="display: flex; gap: 0.35rem;">
        <button type="button" class="btn btn-sm" style="font-size: 0.72rem; padding: 0.2rem 0.5rem;" id="btn-sel-all-yoko-${instId}">✅ ${t('select_all_btn', 'Select All')}</button>
        <button type="button" class="btn btn-sm" style="font-size: 0.72rem; padding: 0.2rem 0.5rem;" id="btn-clr-yoko-${instId}">✖️ ${t('clear_btn', 'Clear')}</button>
      </div>
    `;
    yokoContainer.appendChild(yokoActions);

    const yokoTagBox = document.createElement('div');
    yokoTagBox.className = 'rotation-toggles';
    yokoTagBox.style = 'display: flex; gap: 0.4rem; flex-wrap: wrap; max-height: 180px; overflow-y: auto; padding: 0.25rem; background: rgba(0,0,0,0.15); border-radius: var(--radius-sm); border: 1px solid var(--glass-border);';

    if (yokoOpts.length === 0) {
      yokoTagBox.innerHTML = `<span style="font-size: 0.8rem; color: var(--text-muted); padding: 0.5rem;">${t('gif_no_yoko', 'No horizontal GIF folders found in /gifs')}</span>`;
    } else {
      yokoOpts.forEach(opt => {
        const isChecked = initialTokens.includes('ALL') || initialTokens.includes(String(opt.value).trim().toUpperCase()) || initialTokens.includes(String(opt.label).trim().toUpperCase());
        const lbl = document.createElement('label');
        lbl.className = 'toggle-btn';
        const countStr = (opt.count !== undefined && opt.count > 0) ? ` <span style="opacity:0.65; font-size:0.75rem;">(${opt.count})</span>` : '';
        lbl.innerHTML = `<input type="checkbox" value="${opt.value}" class="multi-cb-${instId}-${targetFieldId}" style="display:none;" ${isChecked ? 'checked' : ''}> <span class="toggle-label">${opt.label}${countStr}</span>`;
        lbl.querySelector('input[type="checkbox"]').addEventListener('change', syncInputFromTags);
        yokoTagBox.appendChild(lbl);
      });
    }
    yokoContainer.appendChild(yokoTagBox);
    container.appendChild(yokoContainer);

    // Container for TATE
    const tateContainer = document.createElement('div');
    tateContainer.id = `pane-tate-${instId}`;
    tateContainer.style = 'display: none; flex-direction: column; gap: 0.4rem;';

    const tateActions = document.createElement('div');
    tateActions.style = 'display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.2rem;';
    tateActions.innerHTML = `
      <span style="font-size: 0.8rem; font-weight: 600; color: var(--text-muted);">📱 /gifs_tate/ (${tateOpts.length} ${t('folders_count', 'folders')})</span>
      <div style="display: flex; gap: 0.35rem;">
        <button type="button" class="btn btn-sm" style="font-size: 0.72rem; padding: 0.2rem 0.5rem;" id="btn-sel-all-tate-${instId}">✅ ${t('select_all_btn', 'Select All')}</button>
        <button type="button" class="btn btn-sm" style="font-size: 0.72rem; padding: 0.2rem 0.5rem;" id="btn-clr-tate-${instId}">✖️ ${t('clear_btn', 'Clear')}</button>
      </div>
    `;
    tateContainer.appendChild(tateActions);

    const tateTagBox = document.createElement('div');
    tateTagBox.className = 'rotation-toggles';
    tateTagBox.style = 'display: flex; gap: 0.4rem; flex-wrap: wrap; max-height: 180px; overflow-y: auto; padding: 0.25rem; background: rgba(0,0,0,0.15); border-radius: var(--radius-sm); border: 1px solid var(--glass-border);';

    if (tateOpts.length === 0) {
      tateTagBox.innerHTML = `<span style="font-size: 0.8rem; color: var(--text-muted); padding: 0.5rem;">${t('gif_no_tate', 'No vertical GIF folders found in /gifs_tate')}</span>`;
    } else {
      tateOpts.forEach(opt => {
        const isChecked = initialTokens.includes('ALL') || initialTokens.includes(String(opt.value).trim().toUpperCase()) || initialTokens.includes(String(opt.label).trim().toUpperCase());
        const lbl = document.createElement('label');
        lbl.className = 'toggle-btn';
        const countStr = (opt.count !== undefined && opt.count > 0) ? ` <span style="opacity:0.65; font-size:0.75rem;">(${opt.count})</span>` : '';
        lbl.innerHTML = `<input type="checkbox" value="${opt.value}" class="multi-cb-${instId}-${targetFieldId}" style="display:none;" ${isChecked ? 'checked' : ''}> <span class="toggle-label">${opt.label}${countStr}</span>`;
        lbl.querySelector('input[type="checkbox"]').addEventListener('change', syncInputFromTags);
        tateTagBox.appendChild(lbl);
      });
    }
    tateContainer.appendChild(tateTagBox);
    container.appendChild(tateContainer);

    // Tab switcher events
    const btnTabYoko = tabNav.querySelector(`#btn-tab-yoko-${instId}`);
    const btnTabTate = tabNav.querySelector(`#btn-tab-tate-${instId}`);
    btnTabYoko.onclick = () => {
      btnTabYoko.style.border = '1px solid var(--accent-primary, #00e5ff)';
      btnTabYoko.style.opacity = '1';
      btnTabTate.style.border = 'none';
      btnTabTate.style.opacity = '0.7';
      yokoContainer.style.display = 'flex';
      tateContainer.style.display = 'none';
    };
    btnTabTate.onclick = () => {
      btnTabTate.style.border = '1px solid var(--accent-primary, #00e5ff)';
      btnTabTate.style.opacity = '1';
      btnTabYoko.style.border = 'none';
      btnTabYoko.style.opacity = '0.7';
      tateContainer.style.display = 'flex';
      yokoContainer.style.display = 'none';
    };

    // Actions
    const btnSelYoko = yokoActions.querySelector(`#btn-sel-all-yoko-${instId}`);
    const btnClrYoko = yokoActions.querySelector(`#btn-clr-yoko-${instId}`);
    if (btnSelYoko) btnSelYoko.onclick = () => {
      yokoTagBox.querySelectorAll('input[type="checkbox"]').forEach(c => c.checked = true);
      syncInputFromTags();
    };
    if (btnClrYoko) btnClrYoko.onclick = () => {
      yokoTagBox.querySelectorAll('input[type="checkbox"]').forEach(c => c.checked = false);
      syncInputFromTags();
    };

    const btnSelTate = tateActions.querySelector(`#btn-sel-all-tate-${instId}`);
    const btnClrTate = tateActions.querySelector(`#btn-clr-tate-${instId}`);
    if (btnSelTate) btnSelTate.onclick = () => {
      tateTagBox.querySelectorAll('input[type="checkbox"]').forEach(c => c.checked = true);
      syncInputFromTags();
    };
    if (btnClrTate) btnClrTate.onclick = () => {
      tateTagBox.querySelectorAll('input[type="checkbox"]').forEach(c => c.checked = false);
      syncInputFromTags();
    };
  } else {
    // Standard multi-select group
    const tagContainer = document.createElement('div');
    tagContainer.className = 'rotation-toggles';
    tagContainer.style = 'display: flex; gap: 0.4rem; flex-wrap: wrap; margin-bottom: 0.2rem;';

    opts.forEach(opt => {
      const isChecked = initialTokens.includes(String(opt.value).trim().toUpperCase());
      const lbl = document.createElement('label');
      lbl.className = 'toggle-btn';
      lbl.innerHTML = `<input type="checkbox" value="${opt.value}" class="multi-cb-${instId}-${targetFieldId}" style="display:none;" ${isChecked ? 'checked' : ''}> <span class="toggle-label">${formatOptLabel(targetFieldId, opt.value, opt.label)}</span>`;
      lbl.querySelector('input[type="checkbox"]').addEventListener('change', syncInputFromTags);
      tagContainer.appendChild(lbl);
    });
    container.appendChild(tagContainer);
  }

  textInput.addEventListener('input', syncTagsFromInput);
  container.appendChild(textInput);
  return container;
}

// Helper: Attach Quick-Preset chips underneath text fields (cities, cryptos, stocks, world clocks)
function attachQuickPresets(input, fieldId) {
  let presets = [];
  let isMulti = false;

  if (fieldId === 'city' || fieldId === 'weather_city' || fieldId === 'location') {
    presets = [
      "Paris, FR", "New York, US", "Tokyo, JP", "London, GB",
      "Berlin, DE", "Madrid, ES", "Rome, IT", "Los Angeles, US",
      "Sydney, AU", "Dubai, AE", "Singapore, SG", "Toronto, CA"
    ];
  } else if (fieldId === 'tracked_markets' || fieldId === 'symbols' || fieldId === 'coins' || fieldId === 'cryptos') {
    presets = ["BTC", "ETH", "SOL", "DOGE", "XRP", "BNB", "NVDA", "AAPL", "TSLA", "MSFT", "AMZN", "GOOGL"];
    isMulti = true;
  } else if (fieldId === 'world_clocks' || fieldId === 'clocks') {
    presets = ["NYC", "TYO", "LON", "PAR", "BER", "DXB", "SYD", "SFO", "SIN", "HKG", "DEL", "MOW"];
    isMulti = true;
  }

  if (presets.length === 0) return null;

  const presetBox = document.createElement('div');
  presetBox.style = 'display: flex; gap: 0.35rem; flex-wrap: wrap; margin-top: 0.35rem; align-items: center;';
  
  const label = document.createElement('span');
  label.style = 'font-size: 0.72rem; color: var(--text-muted); margin-right: 0.2rem;';
  label.innerText = '⚡ ' + t('quick_presets_label', 'Suggestions :');
  presetBox.appendChild(label);

  presets.forEach(preset => {
    const chip = document.createElement('button');
    chip.type = 'button';
    chip.className = 'btn btn-sm';
    chip.style = 'font-size: 0.72rem; padding: 0.15rem 0.45rem; background: rgba(255,255,255,0.06); border-radius: 4px; border: 1px solid rgba(255,255,255,0.1); cursor: pointer;';
    chip.innerText = preset.includes(',') ? preset.split(',')[0] : preset;
    chip.onclick = () => {
      if (isMulti) {
        const currentTokens = input.value.split(',').map(s => s.trim()).filter(s => s.length > 0);
        const idx = currentTokens.findIndex(t => t.toUpperCase() === preset.toUpperCase());
        if (idx >= 0) {
          currentTokens.splice(idx, 1);
        } else {
          currentTokens.push(preset);
        }
        input.value = currentTokens.join(',');
      } else {
        input.value = preset;
      }
      input.dispatchEvent(new Event('input', { bubbles: true }));
      input.dispatchEvent(new Event('change', { bubbles: true }));
    };
    presetBox.appendChild(chip);
  });

  return presetBox;
}

// 3. Main Dynamic Display Renderer
export async function renderDynamicDisplay(targetActiveInstanceId = null) {
  const container = document.getElementById('page-display');
  if (!container) return;

  let wrapper = document.getElementById('dynamic-display-wrapper');
  if (!wrapper) {
    wrapper = document.createElement('div');
    wrapper.id = 'dynamic-display-wrapper';
    container.appendChild(wrapper);
  }
  wrapper.innerHTML = '';

  try {
    // Parallel fetch of initial descriptors, instances, and rotation loop
    const [descriptors, instancesList, rotationList] = await Promise.all([
      fetchCachedEndpoint('/api/engines'),
      API.get('/api/instances').catch(() => []),
      API.get('/api/rotation').catch(() => [])
    ]);

    const enginesMap = {};
    descriptors.forEach(desc => {
      enginesMap[desc.metadata.id] = desc;
    });

    // 1. Render Visual Engine Catalog directly onto the Dashboard (Home Hub)
    renderEngineCatalog(descriptors, instancesList, enginesMap);

    // 2. Onboarding Guide Banner on Display Page
    const guideCard = document.createElement('div');
    guideCard.className = 'card glass shadow';
    guideCard.style = "margin-bottom: 1.5rem; background: linear-gradient(135deg, rgba(99, 102, 241, 0.12) 0%, rgba(30, 41, 59, 0.6) 100%); border: 1px solid rgba(99, 102, 241, 0.3);";
    guideCard.innerHTML = `
      <div style="display: flex; gap: 1rem; align-items: center;">
        <span style="font-size: 2.2rem;">💡</span>
        <div>
          <h4 style="margin: 0 0 0.25rem 0; font-size: 1.05rem;" data-i18n="onboarding_guide_title">${t('onboarding_guide_title', 'How Screen Rotation Works')}</h4>
          <p class="text-muted" style="margin: 0; font-size: 0.85rem; line-height: 1.4;" data-i18n="onboarding_guide_desc">
            ${t('onboarding_guide_desc', 'Your LED matrix continuously cycles through your configured playlist. Pick any engine from the library, adjust its duration in the loop, and fine-tune its appearance below.')}
          </p>
        </div>
      </div>
    `;
    wrapper.appendChild(guideCard);

    // Anchor point for tabs section
    const tabsAnchor = document.createElement('div');
    tabsAnchor.id = 'tabs-anchor-point';
    wrapper.appendChild(tabsAnchor);

    // 3. Render Unified Rotation Loop Panel (Passing pre-fetched rotationList)
    renderRotationPanel(wrapper, tabsAnchor, rotationList, instancesList, enginesMap, descriptors);

    // 4. Section Header for Configured Screens Tabs
    const screensHeader = document.createElement('div');
    screensHeader.style = "margin: 3rem 0 1rem 0; display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 0.5rem;";
    screensHeader.innerHTML = `
      <div>
        <h3 style="margin: 0; font-size: 1.25rem;" data-i18n="configured_screens_title">⚙️ ${t('configured_screens_title', 'Configured Screens & Settings')}</h3>
        <p class="text-muted" style="margin: 0.25rem 0 0 0; font-size: 0.85rem;" data-i18n="configured_screens_desc">${t('configured_screens_desc', 'Click on a screen tab below to customize its visual layout, data feeds, colors, and options.')}</p>
      </div>
      <button id="btn-header-add-screen" class="btn btn-primary" style="padding: 0.5rem 1rem; font-weight: 600;">➕ ${t('modal_add_title', 'Add New Screen')}</button>
    `;
    wrapper.insertBefore(screensHeader, tabsAnchor);

    screensHeader.querySelector('#btn-header-add-screen').onclick = () => {
      showAddScreenModal(descriptors[0]?.metadata?.id || 'clock', descriptors, instancesList, true);
    };

    if (instancesList.length === 0) {
      const emptyCard = document.createElement('div');
      emptyCard.className = 'card glass shadow';
      emptyCard.style = "text-align: center; padding: 2.5rem; color: var(--text-muted, #888); margin-top: 1rem;";
      emptyCard.innerHTML = `
        <div style="font-size: 3rem; margin-bottom: 0.75rem;">📺</div>
        <h4 data-i18n="no_screens_title">${t('no_screens_title', 'No display screens configured yet')}</h4>
        <p style="font-size: 0.9rem; margin-bottom: 1.25rem;" data-i18n="no_screens_desc">${t('no_screens_desc', 'Choose an engine plugin above and click Add to Loop to launch your first screen.')}</p>
        <button class="btn btn-primary" id="btn-empty-first-screen">➕ ${t('modal_add_title', 'Add First Screen')}</button>
      `;
      wrapper.appendChild(emptyCard);
      emptyCard.querySelector('#btn-empty-first-screen').onclick = () => showAddScreenModal(descriptors[0]?.metadata?.id || 'clock', descriptors, instancesList, true);
      return;
    }

    // 5. Tabs Container
    const tabsContainer = document.createElement('div');
    tabsContainer.className = 'tabs';
    wrapper.insertBefore(tabsContainer, tabsAnchor);

    // Determine initially active tab
    let activeCleanId = '';
    if (targetActiveInstanceId) {
      activeCleanId = targetActiveInstanceId.replace(/[^a-zA-Z0-9]/g, '-');
    } else if (instancesList.length > 0) {
      activeCleanId = instancesList[0].instance_id.replace(/[^a-zA-Z0-9]/g, '-');
    }

    // 6. Generate Dynamic Screen Tabs & Panes
    for (let index = 0; index < instancesList.length; index++) {
      const instance = instancesList[index];
      const engineDesc = enginesMap[instance.engine_id];
      if (!engineDesc) continue;
      
      const info = resolveScreenInfo(instance, engineDesc);
      const cleanId = instance.instance_id.replace(/[^a-zA-Z0-9]/g, '-');
      const tabId = `tab-dyn-${cleanId}`;
      const isTabActive = cleanId === activeCleanId;
      const rotEntry = Array.isArray(rotationList) ? rotationList.find(r => r.instance_id === instance.instance_id) : null;
      
      // Tab Button
      const tabBtn = document.createElement('button');
      tabBtn.id = `btn-tab-${cleanId}`;
      tabBtn.className = 'tab-btn' + (isTabActive ? ' active' : '');
      tabBtn.setAttribute('data-tab-target', tabId);
      tabBtn.innerHTML = `${info.icon} <span class="tab-label-text">${info.title}</span>`;
      tabBtn.onclick = (e) => {
        document.querySelectorAll('#page-display .tab-btn').forEach(btn => btn.classList.remove('active'));
        e.currentTarget.classList.add('active');
        
        document.querySelectorAll('#page-display .tab-pane').forEach(pane => pane.classList.remove('active'));
        const pane = document.getElementById(tabId);
        if (pane) pane.classList.add('active');
      };
      
      tabsContainer.appendChild(tabBtn);
      
      // Tab Pane
      const tabPane = document.createElement('div');
      tabPane.id = tabId;
      tabPane.className = 'tab-pane' + (isTabActive ? ' active' : '');
      
      const card = document.createElement('div');
      card.className = 'card glass shadow';
      card.style.marginTop = '1.5rem';
      
      // Header with Title + In-Rotation Badge + Action Buttons
      const cardHeader = document.createElement('div');
      cardHeader.style = "display: flex; justify-content: space-between; align-items: center; flex-wrap: wrap; gap: 0.75rem; margin-bottom: 1.5rem; padding-bottom: 1rem; border-bottom: 1px solid var(--glass-border);";
      
      const statusBadgeHtml = rotEntry ? 
        `<span class="badge" style="background: rgba(16,185,129,0.2); color:#34d399; font-weight:600; font-size:0.82rem;">🟢 ${t('in_rotation_badge', 'In Rotation')} (${rotEntry.duration_sec}s)</span>` : 
        `<span class="badge" style="background: rgba(255,255,255,0.08); color:var(--text-muted); font-size:0.82rem;">⚪ ${t('not_in_rotation_badge', 'Off Loop (Standby)')}</span>`;

      cardHeader.innerHTML = `
        <div style="display: flex; align-items: center; gap: 0.75rem;">
          <span style="font-size: 1.6rem;" id="header-icon-${cleanId}">${info.icon}</span>
          <div>
            <h3 style="margin: 0; font-size: 1.2rem;" id="header-title-${cleanId}">${info.title}</h3>
            <span style="font-size: 0.78rem; color: var(--text-muted); font-family: monospace;">${instance.instance_id} (${instance.engine_id})</span>
          </div>
          ${statusBadgeHtml}
        </div>
        <div style="display: flex; gap: 0.5rem; align-items: center; flex-wrap: wrap;">
          ${rotEntry ? 
            `<button class="btn btn-rot-toggle" style="background: rgba(239,68,68,0.15); color:#f87171; border:1px solid rgba(239,68,68,0.3); font-size:0.82rem; padding:0.4rem 0.75rem;">${t('remove_from_rotation_btn', '➖ Remove from Loop')}</button>` : 
            `<button class="btn btn-rot-toggle btn-primary" style="font-size:0.82rem; padding:0.4rem 0.75rem;">${t('add_to_rotation_btn', '➕ Add to Loop')}</button>`}
          <button class="btn btn-duplicate" style="background: rgba(255,255,255,0.08); color:#fff; font-size:0.82rem; padding:0.4rem 0.75rem;">📋 ${t('duplicate_btn', 'Duplicate')}</button>
          <button class="btn btn-danger btn-delete" style="font-size:0.82rem; padding:0.4rem 0.75rem;">🗑️</button>
        </div>
      `;

      // Wire rotation toggle
      const rotToggleBtn = cardHeader.querySelector('.btn-rot-toggle');
      if (rotToggleBtn) {
        rotToggleBtn.onclick = async () => {
          rotToggleBtn.disabled = true;
          const curRot = await API.get('/api/rotation').catch(() => []);
          let newRot = Array.isArray(curRot) ? curRot.slice() : [];
          if (rotEntry) {
            newRot = newRot.filter(r => r.instance_id !== instance.instance_id);
            window.showToast(`Removed '${info.title}' from rotation`, 'info');
          } else {
            const allowOverlay = !engineDesc.capabilities || engineDesc.capabilities.allows_overlay !== false;
            newRot.push({
              instance_id: instance.instance_id,
              duration_sec: 30,
              overlays: { fighter: allowOverlay },
              fighter_overlay: allowOverlay
            });
            window.showToast(`Added '${info.title}' to rotation`, 'success');
          }
          await API.post('/api/rotation', newRot);
          await renderDynamicDisplay(instance.instance_id);
        };
      }

      // Wire duplicate button
      const dupBtn = cardHeader.querySelector('.btn-duplicate');
      if (dupBtn) {
        dupBtn.onclick = async () => {
          dupBtn.disabled = true;
          let counter = 2;
          let newId = `${instance.instance_id}_copy`;
          while (instancesList.some(i => i.instance_id === newId)) {
            newId = `${instance.instance_id}_copy${counter++}`;
          }
          const clonedCfg = JSON.parse(JSON.stringify(instance.config || {}));
          clonedCfg.screen_title = `${info.title} (${t('copy_suffix', 'Copy')})`;
          try {
            await API.post('/api/instances', {
              instance_id: newId,
              engine_id: instance.engine_id,
              config: clonedCfg
            });
            window.showToast(`Duplicated screen to '${newId}'!`, 'success');
            await renderDynamicDisplay(newId);
          } catch (e) {
            window.showToast('Failed to duplicate screen', 'error');
            dupBtn.disabled = false;
          }
        };
      }

      // Wire delete button
      const delBtn = cardHeader.querySelector('.btn-delete');
      if (delBtn) {
        delBtn.onclick = async () => {
          if (confirm(`${t('confirm_remove_screen', 'Are you sure you want to remove this screen?')}\n${info.title} (${instance.instance_id})`)) {
            delBtn.disabled = true;
            try {
              await API.delete(`/api/instances/${instance.instance_id}`);
              const curRot = await API.get('/api/rotation').catch(() => []);
              if (Array.isArray(curRot) && curRot.some(r => r.instance_id === instance.instance_id)) {
                const cleanedRot = curRot.filter(r => r.instance_id !== instance.instance_id);
                await API.post('/api/rotation', cleanedRot);
              }
              window.showToast('Screen removed', 'success');
              await renderDynamicDisplay();
            } catch (e) {
              window.showToast('Failed to remove screen', 'error');
              delBtn.disabled = false;
            }
          }
        };
      }

      card.appendChild(cardHeader);
      
      const formGrid = document.createElement('div');
      formGrid.className = 'form-grid';

      // 1. Custom Screen Title field at top of form
      const titleFormGroup = document.createElement('div');
      titleFormGroup.className = 'form-group';
      titleFormGroup.style = "grid-column: 1 / -1; margin-bottom: 0.75rem; background: rgba(255,255,255,0.02); padding: 0.75rem; border-radius: var(--radius-md); border: 1px solid var(--glass-border);";
      titleFormGroup.innerHTML = `
        <label style="font-size: 0.85rem; font-weight: 600; color: var(--accent-primary);" data-i18n="screen_name_label">${t('screen_name_label', 'Screen Title / Label:')}</label>
        <input type="text" id="cfg-custom-title-${cleanId}" class="input" placeholder="e.g. ${info.title}" value="${(instance.config && instance.config.screen_title) ? instance.config.screen_title : ''}">
        <small class="text-muted" style="margin-top: 0.35rem; display: block;" data-i18n="screen_name_help">${t('screen_name_help', 'Custom friendly name displayed in tabs and the rotation playlist.')}</small>
      `;
      formGrid.appendChild(titleFormGroup);
      
      // Robust schema fields extraction
      const fields = getSchemaFields(engineDesc);
      
      if (fields && fields.length > 0) {
        for (const field of fields) {
          const formGroup = document.createElement('div');
          formGroup.className = 'form-group';
          formGroup.id = `fg-${cleanId}-${field.id}`;
          
          const label = document.createElement('label');
          label.innerText = field.label || field.id;
          if (field.description) {
            label.title = field.description;
            label.style.cursor = 'help';
            label.innerHTML += ' ℹ️';
          }
          formGroup.appendChild(label);
          
          let input = null;
          const configMap = instance.config || {};
          let currentVal = configMap[field.id] !== undefined ? configMap[field.id] : field.default_value;
          if (currentVal === undefined || currentVal === null) currentVal = '';

          // 1. Timezone or Options Endpoint (using cached requests)
          if (field.id === 'timezone' || field.options_endpoint === '/api/timezones') {
              input = document.createElement('select');
              input.className = 'input';
              input.id = `cfg-dyn-${instance.instance_id}-${field.id}`;
              GLOBAL_TIMEZONES.forEach(opt => {
                  const option = document.createElement('option');
                  option.value = opt.value;
                  option.innerText = opt.label;
                  if (String(opt.value) === String(currentVal)) option.selected = true;
                  input.appendChild(option);
              });
          } else if (field.options_endpoint) {
             const data = await fetchCachedEndpoint(field.options_endpoint);
             const normalizedOpts = parseOptions(data);
             if (field.multiple || field.id === 'folder' || field.id === 'playlists' || field.id === 'gif_folder') {
                 input = buildMultiSelectGroup(normalizedOpts, field.id, instance.instance_id, currentVal);
             } else {
                 input = document.createElement('select');
                 input.className = 'input';
                 input.id = `cfg-dyn-${instance.instance_id}-${field.id}`;
                 normalizedOpts.forEach(opt => {
                     const option = document.createElement('option');
                     option.value = opt.value;
                     option.innerText = formatOptLabel(field.id, opt.value, opt.label);
                     if (String(opt.value) === String(currentVal)) option.selected = true;
                     input.appendChild(option);
                 });
             }
          }
          // 2. Options / Enum Field (from field.options)
          else if (isOptionsField(field) || (field.options && field.options.length > 0)) {
              const opts = parseOptions(field.options);
              if (field.multiple) {
                  input = buildMultiSelectGroup(opts, field.id, instance.instance_id, currentVal);
              } else {
                  input = document.createElement('select');
                  input.className = 'input';
                  input.id = `cfg-dyn-${instance.instance_id}-${field.id}`;
                  
                  opts.forEach(opt => {
                    const option = document.createElement('option');
                    option.value = opt.value;
                    option.innerText = formatOptLabel(field.id, opt.value, opt.label);
                    if (String(opt.value) === String(currentVal)) option.selected = true;
                    input.appendChild(option);
                  });
              }
          } 
          // 3. Boolean
          else if (isBooleanField(field)) { 
             input = document.createElement('select');
             input.className = 'input';
             input.id = `cfg-dyn-${instance.instance_id}-${field.id}`;
             const o1 = document.createElement('option'); o1.value = 'true'; o1.innerText = 'Enabled';
             const o2 = document.createElement('option'); o2.value = 'false'; o2.innerText = 'Disabled';
             if (String(currentVal).toLowerCase() === 'true' || String(currentVal) === '1') o1.selected = true; else o2.selected = true;
             input.appendChild(o1); input.appendChild(o2);
          } 
          // 4. Color
          else if (isColorField(field)) {
             input = document.createElement('input');
             input.type = 'color';
             input.className = 'input';
             input.id = `cfg-dyn-${instance.instance_id}-${field.id}`;
             let col = String(currentVal).trim();
             if (col.startsWith('0x') || col.startsWith('0X')) {
               col = '#' + col.slice(2).padStart(6, '0');
             } else if (!col.startsWith('#') && col.length === 6) {
               col = '#' + col;
             }
             input.value = col || '#ffffff';
          } 
          // 5. Default Inputs (Text/Number with Quick-Presets)
          else { 
             input = document.createElement('input');
             input.type = isNumberField(field) ? 'number' : 'text';
             input.className = 'input';
             input.id = `cfg-dyn-${instance.instance_id}-${field.id}`;
             if (field.min_val !== undefined && field.min_val !== null && field.min_val !== '') input.min = field.min_val;
             if (field.max_val !== undefined && field.max_val !== null && field.max_val !== '') input.max = field.max_val;
             if (field.step !== undefined && field.step !== null && field.step !== '') input.step = field.step;
             input.value = currentVal;
          }
          
          if (input) {
             formGroup.appendChild(input);
             if (!isBooleanField(field) && !isColorField(field) && !isNumberField(field) && !field.options_endpoint) {
               const quickPresets = attachQuickPresets(input, field.id);
               if (quickPresets) formGroup.appendChild(quickPresets);
             }
          }
          
          formGrid.appendChild(formGroup);
        }
      } else {
         const noSettings = document.createElement('p');
         noSettings.setAttribute('data-i18n', 'no_config_available');
         noSettings.innerText = t('no_config_available', 'No configuration available for this engine.');
         formGrid.appendChild(noSettings);
      }
      
      card.appendChild(formGrid);

      // Reactivity: Handle `visible_when`
      function updateVisibility() {
        const curVals = {};
        fields.forEach(f => {
          const el = document.getElementById(`cfg-dyn-${instance.instance_id}-${f.id}`);
          if (el) curVals[f.id] = el.value;
        });
        fields.forEach(f => {
          if (f.visible_when && f.visible_when.trim().length > 0) {
            const fg = document.getElementById(`fg-${cleanId}-${f.id}`);
            if (fg) {
              const parts = f.visible_when.split('=');
              if (parts.length === 2) {
                const targetKey = parts[0].trim();
                const expected = parts[1].trim();
                const actual = String(curVals[targetKey] !== undefined ? curVals[targetKey] : '');
                fg.style.display = (actual === expected) ? 'block' : 'none';
              }
            }
          }
        });
      }
      formGrid.addEventListener('input', updateVisibility);
      formGrid.addEventListener('change', updateVisibility);
      updateVisibility();
      
      // Save Action Bar
      const actionBar = document.createElement('div');
      actionBar.style = "display: flex; gap: 0.75rem; margin-top: 1.5rem; flex-wrap: wrap;";

      const saveBtn = document.createElement('button');
      saveBtn.className = 'btn btn-primary';
      saveBtn.style = 'flex: 1; min-width: 200px; font-size: 1rem; font-weight: 700; padding: 0.85rem;';
      saveBtn.setAttribute('data-i18n', 'save_screen_config_btn');
      saveBtn.innerText = t('save_screen_config_btn', '💾 Save Screen Configuration');

      async function saveConfig() {
         const customTitleInput = document.getElementById(`cfg-custom-title-${cleanId}`);
         const payload = {
             instance_id: instance.instance_id,
             engine_id: instance.engine_id,
             config: instance.config || {}
         };

         if (customTitleInput && customTitleInput.value.trim()) {
           payload.config.screen_title = customTitleInput.value.trim();
         } else {
           delete payload.config.screen_title;
         }
         
         if (fields && fields.length > 0) {
             fields.forEach(field => {
                const el = document.getElementById(`cfg-dyn-${instance.instance_id}-${field.id}`);
                if (el && el.value !== undefined && el.value.trim().length > 0) {
                    payload.config[field.id] = el.value.trim();
                } else if (field.multiple || field.options_endpoint) {
                    const checkboxes = document.querySelectorAll(`.multi-cb-${instance.instance_id}-${field.id}:checked`);
                    if (checkboxes.length > 0) {
                        const vals = Array.from(checkboxes).map(cb => cb.value);
                        payload.config[field.id] = vals.join(',');
                    } else {
                        payload.config[field.id] = 'all';
                    }
                } else if (el) {
                    payload.config[field.id] = el.value;
                }
             });
         }

         try {
             await API.post(`/api/instances`, payload);
             instance.config = payload.config;
             const updatedInfo = resolveScreenInfo(instance, engineDesc);
             
             // Update tab button label, card header, and playlist row in-place
             const tabBtnEl = document.getElementById(`btn-tab-${cleanId}`);
             if (tabBtnEl) {
               tabBtnEl.innerHTML = `${updatedInfo.icon} <span class="tab-label-text">${updatedInfo.title}</span>`;
             }
             const headerTitleEl = document.getElementById(`header-title-${cleanId}`);
             if (headerTitleEl) headerTitleEl.innerText = updatedInfo.title;
             const headerIconEl = document.getElementById(`header-icon-${cleanId}`);
             if (headerIconEl) headerIconEl.innerText = updatedInfo.icon;
             
             const rotTitleEl = document.getElementById(`rot-title-${instance.instance_id}`);
             if (rotTitleEl) {
               rotTitleEl.innerText = updatedInfo.full;
             }
             
             window.showToast(t('config_saved_live', 'Configuration saved & applied live!'), 'success');
             return true;
         } catch(e) {
             console.error('API Error:', e);
             window.showToast('Failed to save screen config: ' + (e.message || ''), 'warning');
             return false;
         }
      }

      saveBtn.onclick = async () => {
        saveBtn.disabled = true;
        saveBtn.innerText = t('saving_btn', 'Saving...');
        await saveConfig();
        saveBtn.disabled = false;
        saveBtn.innerText = t('save_screen_config_btn', '💾 Save Screen Configuration');
      };

      actionBar.appendChild(saveBtn);
      card.appendChild(actionBar);
      
      tabPane.appendChild(card);
      wrapper.appendChild(tabPane);
    }
    
  } catch (e) {
    console.error('Failed to load dynamic engines:', e);
  }
}

export function initDynamicEngines() {
  return renderDynamicDisplay();
}
