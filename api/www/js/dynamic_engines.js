import { API } from './api.js';
import { translations } from './i18n.js';

export const GLOBAL_TIMEZONES = [
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

export async function initDynamicEngines() {
  const container = document.getElementById('page-display');
  const tabsContainer = document.querySelector('.tabs');
  
  if (!container || !tabsContainer) return;
  
  try {
    const descriptors = await API.get('/api/engines');
    const instancesList = await API.get('/api/instances').catch(() => []);
    
    // Convert descriptors to map for easy lookup
    const enginesMap = {};
    descriptors.forEach(desc => {
      enginesMap[desc.metadata.id] = desc;
    });

    // Rotation manager panel (order + duration of instances in the loop).
    await renderRotationPanel(container, tabsContainer, instancesList, enginesMap);

    // Add New Instance Controls
    const addContainer = document.createElement('div');
    addContainer.style = "display: flex; gap: 0.5rem; align-items: center; margin-left: auto;";
    
    const engineSelect = document.createElement('select');
    engineSelect.className = 'input';
    engineSelect.style = "width: auto; padding: 0.2rem 0.5rem;";
    descriptors.forEach(desc => {
        const opt = document.createElement('option');
        opt.value = desc.metadata.id;
        opt.innerText = desc.metadata.name;
        engineSelect.appendChild(opt);
    });
    
    const addBtn = document.createElement('button');
    addBtn.className = 'btn btn-primary';
    addBtn.innerHTML = `➕ Add Engine`;
    addBtn.style = "padding: 0.2rem 0.5rem;";
    addBtn.onclick = () => {
       const eng = engineSelect.value;
       const inst = prompt("Enter a unique name for this instance (e.g. main_" + eng + "):", eng);
       if (!inst) return;
       API.post('/api/instances', { instance_id: inst, engine_id: eng, config: {} }).then(() => {
           window.showToast('Instance created', 'success');
           localStorage.setItem('activePage', 'display');
           setTimeout(() => location.reload(), 500);
       });
    };
    
    addContainer.appendChild(engineSelect);
    addContainer.appendChild(addBtn);
    tabsContainer.appendChild(addContainer);

    instancesList.forEach(async instance => {
      const engineDesc = enginesMap[instance.engine_id];
      if (!engineDesc) return;
      
      const tabId = `tab-dyn-${instance.instance_id.replace(/[^a-zA-Z0-9]/g, '-')}`;
      
      // 1. Create Tab Button
      const tabBtn = document.createElement('button');
      tabBtn.className = 'tab-btn';
      tabBtn.innerHTML = `🧩 ${engineDesc.metadata.name} (${instance.instance_id})`;
      tabBtn.onclick = (e) => {
        document.querySelectorAll('#page-display .tab-btn').forEach(btn => btn.classList.remove('active'));
        e.currentTarget.classList.add('active');
        
        document.querySelectorAll('#page-display .tab-pane').forEach(pane => pane.classList.remove('active'));
        const targetPane = document.getElementById(tabId);
        if (targetPane) targetPane.classList.add('active');
      };
      
      tabsContainer.insertBefore(tabBtn, addContainer);
      
      // 2. Create Tab Pane
      const tabPane = document.createElement('div');
      tabPane.id = tabId;
      tabPane.className = 'tab-pane';
      
      const card = document.createElement('div');
      card.className = 'card glass shadow';
      card.style.marginTop = '2rem';
      
      const title = document.createElement('h3');
      title.innerText = `⚙️ ${engineDesc.metadata.name} - ${instance.instance_id}`;
      
      // Delete instance button
      const delBtn = document.createElement('button');
      delBtn.className = 'btn btn-danger';
      delBtn.style = "float: right; padding: 0.2rem 0.5rem; font-size: 0.8rem;";
      delBtn.innerText = '🗑️ Remove';
      delBtn.onclick = () => {
          if (confirm('Are you sure you want to remove this instance?')) {
              API.delete(`/api/instances/${instance.instance_id}`).then(() => {
                  window.showToast('Instance removed', 'success');
                  localStorage.setItem('activePage', 'display');
                  setTimeout(() => location.reload(), 500);
              });
          }
      };
      title.appendChild(delBtn);
      card.appendChild(title);
      
      const formGrid = document.createElement('div');
      formGrid.className = 'form-grid';
      
      // Generate fields from schema
      if (engineDesc.schema && engineDesc.schema.fields && engineDesc.schema.fields.length > 0) {
        for (const field of engineDesc.schema.fields) {
          const formGroup = document.createElement('div');
          formGroup.className = 'form-group';
          
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
          const currentVal = configMap[field.id] !== undefined ? configMap[field.id] : field.default_value;

          // 1. Dynamic Options from API or Local Data
          if (field.id === 'timezone' || field.options_endpoint === '/api/timezones') {
              input = document.createElement('select');
              input.className = 'input';
              input.id = `cfg-dyn-${instance.instance_id}-${field.id}`;
              GLOBAL_TIMEZONES.forEach(opt => {
                  const option = document.createElement('option');
                  option.value = opt.value;
                  option.innerText = opt.label;
                  if (opt.value === currentVal) option.selected = true;
                  input.appendChild(option);
              });
          } else if (field.options_endpoint) {
             const data = await API.get(field.options_endpoint).catch(() => []);
             if (field.multiple) {
                 // Checkboxes grid
                 input = document.createElement('div');
                 input.className = 'rotation-toggles';
                 input.style = 'display: flex; gap: 0.5rem; flex-wrap: wrap;';
                 
                 const selectedValues = currentVal ? currentVal.split(',') : [];
                 
                 data.forEach(opt => {
                     const isChecked = selectedValues.includes(opt.value);
                     const lbl = document.createElement('label');
                     lbl.className = 'toggle-btn';
                     lbl.innerHTML = `<input type="checkbox" value="${opt.value}" class="multi-cb-${instance.instance_id}-${field.id}" style="display:none;" ${isChecked ? 'checked' : ''}> <span class="toggle-label">${opt.label}</span>`;
                     // Handle click styling manually if needed or rely on CSS
                     input.appendChild(lbl);
                 });
             } else {
                 // Dropdown
                 input = document.createElement('select');
                 input.className = 'input';
                 input.id = `cfg-dyn-${instance.instance_id}-${field.id}`;
                 data.forEach(opt => {
                     const option = document.createElement('option');
                     option.value = opt.value;
                     option.innerText = opt.label;
                     if (opt.value === currentVal) option.selected = true;
                     input.appendChild(option);
                 });
             }
          }
          else if (field.field_type === 4 || field.field_type === 'Options') { 
              input = document.createElement('select');
              input.className = 'input';
              input.id = `cfg-dyn-${instance.instance_id}-${field.id}`;
              const formatOptLabel = (val, raw) => {
                const lang = localStorage.getItem('lang') || 'en';
                const dict = (typeof translations !== 'undefined' && translations && translations[lang]) ? translations[lang] : ((typeof translations !== 'undefined' && translations) ? translations.en : {});
                const v = String(val).trim();
                const lower = v.toLowerCase();
                if (field.id === 'direction' || ['rtl', 'ltr', 'ttb', 'btt', 'static', 'left', 'right', 'up', 'down', 'none'].includes(lower)) {
                  if (lower === 'rtl' || lower === 'left') return dict.dir_rtl || "Right to Left (RTL)";
                  if (lower === 'ltr' || lower === 'right') return dict.dir_ltr || "Left to Right (LTR)";
                  if (lower === 'ttb' || lower === 'down') return dict.dir_ttb || "Top to Bottom (TTB)";
                  if (lower === 'btt' || lower === 'up') return dict.dir_btt || "Bottom to Top (BTT)";
                  if (lower === 'static' || lower === 'none') return dict.dir_static || "Static (No Scroll)";
                }
                if (field.id === 'units' || field.id === 'unit') {
                  if (lower === 'c' || lower === 'metric') return "Celsius (°C)";
                  if (lower === 'f' || lower === 'imperial') return "Fahrenheit (°F)";
                }
                if (field.id === 'lang') {
                  if (lower === 'fr') return "Français (French)";
                  if (lower === 'en') return "English";
                  if (lower === 'es') return "Español (Spanish)";
                  if (lower === 'de') return "Deutsch (German)";
                  if (lower === 'it') return "Italiano (Italian)";
                }
                if (field.id === 'chart_timeframe') {
                  if (lower === 'hourly') return "Hourly (1h)";
                  if (lower === 'daily') return "Daily (24h)";
                  if (lower === 'weekly') return "Weekly (7d)";
                  if (lower === 'monthly') return "Monthly (30d)";
                }
                if (field.id === 'currency') {
                  if (lower === 'usd') return "US Dollar ($ USD)";
                  if (lower === 'eur') return "Euro (€ EUR)";
                  if (lower === 'gbp') return "British Pound (£ GBP)";
                  if (lower === 'jpy') return "Japanese Yen (¥ JPY)";
                }
                if (field.id === 'style') {
                  if (lower === 'spectrum') return "Spectrum Bars";
                  if (lower === 'waveform') return "Waveform Oscilloscope";
                  if (lower === 'radial') return "Radial Circular";
                  if (lower === 'neon_fire') return "Neon Fire";
                }
                if (field.id === 'date_format') {
                  if (v === '%d/%m/%Y') return "DD/MM/YYYY (24/08/2026)";
                  if (v === '%Y-%m-%d') return "YYYY-MM-DD (2026-08-24)";
                  if (v === '%d %b %Y') return "DD Mon YYYY (24 Aug 2026)";
                  if (v === '%A %d %B') return "Weekday DD Month (Monday 24 August)";
                }
                if (field.id === 'clock_format') {
                  if (v === '%H:%M:%S') return "24h with Seconds (HH:MM:SS)";
                  if (v === '%H:%M') return "24h (HH:MM)";
                  if (v === '%I:%M:%S %p') return "12h with Seconds (HH:MM:SS AM/PM)";
                  if (v === '%I:%M %p') return "12h (HH:MM AM/PM)";
                }
                return raw || val;
              };
              if (field.options && field.options.length > 0) {
                field.options.forEach(opt => {
                  const option = document.createElement('option');
                  option.value = opt.value;
                  option.innerText = formatOptLabel(opt.value, opt.label);
                  if (opt.value === currentVal) option.selected = true;
                  input.appendChild(option);
                });
              }
           } 
          // 3. Boolean
          else if (field.field_type === 0 || field.field_type === 'Boolean') { 
             input = document.createElement('select');
             input.className = 'input';
             input.id = `cfg-dyn-${instance.instance_id}-${field.id}`;
             const o1 = document.createElement('option'); o1.value = 'true'; o1.innerText = 'Enabled';
             const o2 = document.createElement('option'); o2.value = 'false'; o2.innerText = 'Disabled';
             if (String(currentVal) === 'true') o1.selected = true; else o2.selected = true;
             input.appendChild(o1); input.appendChild(o2);
          } 
          // 4. Color
          else if (field.field_type === 'Color' || (field.id && field.id.includes('color'))) {
             input = document.createElement('input');
             input.type = 'color';
             input.className = 'input';
             input.id = `cfg-dyn-${instance.instance_id}-${field.id}`;
             input.value = currentVal;
          } 
          // 5. Default Inputs (Text/Number)
          else { 
             input = document.createElement('input');
             input.type = (field.field_type === 1 || field.field_type === 2 || field.field_type === 'Integer' || field.field_type === 'Float') ? 'number' : 'text';
             input.className = 'input';
             input.id = `cfg-dyn-${instance.instance_id}-${field.id}`;
             if (field.min_val !== null) input.min = field.min_val;
             if (field.max_val !== null) input.max = field.max_val;
             input.value = currentVal;
          }
          
          if (input) {
             formGroup.appendChild(input);
          }
          
          formGrid.appendChild(formGroup);
        }
      } else {
         const noSettings = document.createElement('p');
         noSettings.innerText = 'No configuration available for this engine.';
         formGrid.appendChild(noSettings);
      }
      
      card.appendChild(formGrid);
      
      const saveBtn = document.createElement('button');
      saveBtn.className = 'btn btn-primary';
      saveBtn.style = 'margin-top: 1rem; width: 100%;';
      saveBtn.innerText = 'Save Configuration';
      saveBtn.onclick = async () => {
         const payload = {
             instance_id: instance.instance_id,
             engine_id: instance.engine_id,
             config: instance.config || {}
         };
         
         if (engineDesc.schema && engineDesc.schema.fields) {
             engineDesc.schema.fields.forEach(field => {
                if (field.multiple && field.options_endpoint) {
                    const checkboxes = document.querySelectorAll(`.multi-cb-${instance.instance_id}-${field.id}:checked`);
                    const vals = Array.from(checkboxes).map(cb => cb.value);
                    payload.config[field.id] = vals.join(',');
                } else {
                    const el = document.getElementById(`cfg-dyn-${instance.instance_id}-${field.id}`);
                    if (el) {
                       payload.config[field.id] = el.value;
                    }
                }
             });
         }

         // POST /api/instances upserts by instance_id on the backend, so saving
         // an existing engine updates it in place rather than duplicating it.
         try {
             await API.post(`/api/instances`, payload);
             window.showToast('Engine configuration saved!', 'success');
         } catch(e) {
             console.error('API Error:', e);
             window.showToast('Failed to save engine config', 'warning');
         }
      };
      card.appendChild(saveBtn);
      
      tabPane.appendChild(card);
      document.getElementById('page-display').appendChild(tabPane);
    });
    
  } catch (e) {
    console.error('Failed to load dynamic engines:', e);
  }
}

// ---------------------------------------------------------------------------
// Rotation manager: lets the user pick which instances are shown, in which
// order, and for how long. Reads/writes GET/POST /api/rotation.
// ---------------------------------------------------------------------------
async function renderRotationPanel(container, tabsContainer, instancesList, enginesMap) {
  const rotation = await API.get('/api/rotation').catch(() => []);
  // Working copy: array of { instance_id, duration_sec }.
  let entries = Array.isArray(rotation) ? rotation.slice() : [];

  const nameFor = (instId) => {
    const inst = instancesList.find(i => i.instance_id === instId);
    if (!inst) return instId;
    const eng = enginesMap[inst.engine_id];
    return `${eng ? eng.metadata.name : inst.engine_id} (${instId})`;
  };

  // The rotation "value" is a duration in seconds for time-based engines, but
  // for the GIF engine it is a number of clips to play before advancing.
  const engineIdFor = (instId) => {
    const inst = instancesList.find(i => i.instance_id === instId);
    return inst ? inst.engine_id : null;
  };
  const isCountBased = (instId) => engineIdFor(instId) === 'gifs';

  const card = document.createElement('div');
  card.className = 'card glass shadow';
  card.style = 'margin: 1rem 0; box-sizing: border-box; max-width: 100%; overflow: hidden;';

  const title = document.createElement('h3');
  title.innerText = '🔁 Rotation';
  card.appendChild(title);

  const list = document.createElement('div');
  list.className = 'rotation-list';
  card.appendChild(list);

  function render() {
    list.innerHTML = '';
    entries.forEach((entry, idx) => {
      const row = document.createElement('div');
      row.className = 'rotation-row';

      // 1. Top Header: Badge + Instance Name + Action Buttons
      const header = document.createElement('div');
      header.className = 'rotation-row-header';

      const titleGroup = document.createElement('div');
      titleGroup.className = 'rotation-title-group';

      const badge = document.createElement('span');
      badge.className = 'rotation-order-badge';
      badge.innerText = `#${idx + 1}`;
      titleGroup.appendChild(badge);

      const label = document.createElement('span');
      label.className = 'rotation-title';
      label.title = nameFor(entry.instance_id);
      label.innerText = nameFor(entry.instance_id);
      titleGroup.appendChild(label);

      header.appendChild(titleGroup);

      // Action buttons (Up, Down, Delete)
      const actions = document.createElement('div');
      actions.className = 'rotation-actions';

      const up = document.createElement('button');
      up.type = 'button';
      up.className = 'btn btn-icon';
      up.title = 'Move Up';
      up.innerText = '▲';
      up.disabled = idx === 0;
      up.onclick = () => { entries.splice(idx - 1, 0, entries.splice(idx, 1)[0]); render(); };
      actions.appendChild(up);

      const down = document.createElement('button');
      down.type = 'button';
      down.className = 'btn btn-icon';
      down.title = 'Move Down';
      down.innerText = '▼';
      down.disabled = idx === entries.length - 1;
      down.onclick = () => { entries.splice(idx + 1, 0, entries.splice(idx, 1)[0]); render(); };
      actions.appendChild(down);

      const del = document.createElement('button');
      del.type = 'button';
      del.className = 'btn btn-icon btn-danger';
      del.title = 'Remove from rotation';
      del.innerText = '🗑️';
      del.onclick = () => { entries.splice(idx, 1); render(); };
      actions.appendChild(del);

      header.appendChild(actions);
      row.appendChild(header);

      // 2. Bottom Settings Bar: Duration + Fighter Overlay
      const settingsBar = document.createElement('div');
      settingsBar.className = 'rotation-settings-bar';

      const countBased = isCountBased(entry.instance_id);

      // Duration input group
      const durGroup = document.createElement('div');
      durGroup.className = 'rotation-dur-group';
      durGroup.title = countBased ? 'Number of GIFs to play before rotating' : 'Duration in seconds';

      const durLbl = document.createElement('span');
      durLbl.className = 'rotation-dur-label';
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
      unit.className = 'rotation-unit';
      unit.innerText = countBased ? 'GIFs' : 'sec';
      durGroup.appendChild(unit);

      settingsBar.appendChild(durGroup);

      // Per-entry Fighter overlay toggle
      const fightLbl = document.createElement('label');
      fightLbl.className = 'rotation-fighter-lbl';
      fightLbl.title = 'Show the Fighter overlay on this screen';
      const fightCb = document.createElement('input');
      fightCb.type = 'checkbox';
      fightCb.checked = entry.fighter_overlay !== false;
      fightCb.onchange = () => { entry.fighter_overlay = fightCb.checked; };
      fightLbl.appendChild(fightCb);
      const fightTxt = document.createElement('span');
      fightTxt.innerText = '🥊 Overlay';
      fightLbl.appendChild(fightTxt);
      settingsBar.appendChild(fightLbl);

      row.appendChild(settingsBar);
      list.appendChild(row);
    });
    if (entries.length === 0) {
      const empty = document.createElement('p');
      empty.className = 'text-muted';
      empty.style.padding = '0.5rem 0';
      empty.innerText = 'No instances in rotation. Add one below.';
      list.appendChild(empty);
    }
  }

  // Add-to-rotation card with custom compact select
  const addCard = document.createElement('div');
  addCard.className = 'rotation-add-card';

  const addTitle = document.createElement('span');
  addTitle.className = 'rotation-add-title';
  addTitle.innerText = '➕ Add Instance to Rotation';
  addCard.appendChild(addTitle);

  const addRow = document.createElement('div');
  addRow.className = 'rotation-add-row';

  const sel = document.createElement('select');
  sel.className = 'rotation-select';
  instancesList.forEach(inst => {
    const opt = document.createElement('option');
    opt.value = inst.instance_id;
    opt.innerText = nameFor(inst.instance_id);
    sel.appendChild(opt);
  });

  const addBtn = document.createElement('button');
  addBtn.type = 'button';
  addBtn.className = 'btn btn-primary rotation-add-btn';
  addBtn.innerText = 'Add';
  addBtn.onclick = () => {
    if (!sel.value) return;
    entries.push({ instance_id: sel.value, duration_sec: 30, fighter_overlay: true });
    render();
  };

  addRow.appendChild(sel);
  addRow.appendChild(addBtn);
  addCard.appendChild(addRow);
  card.appendChild(addCard);

  const saveBtn = document.createElement('button');
  saveBtn.type = 'button';
  saveBtn.className = 'btn btn-primary rotation-save-btn';
  saveBtn.innerText = '💾 Save Rotation';
  saveBtn.onclick = async () => {
    const payload = entries.map(e => ({
      instance_id: e.instance_id,
      duration_sec: parseInt(e.duration_sec) || 1,
      fighter_overlay: e.fighter_overlay !== false,
    }));
    try {
      await API.post('/api/rotation', payload);
      window.showToast('Rotation saved!', 'success');
    } catch (e) {
      window.showToast('Failed to save rotation', 'warning');
    }
  };
  card.appendChild(saveBtn);

  render();
  // Insert the panel above the engine tabs.
  container.insertBefore(card, tabsContainer);
}
