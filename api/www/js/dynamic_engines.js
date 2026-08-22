import { API } from './api.js';

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
           window.showToast('Instance created, please refresh', 'success');
           setTimeout(() => location.reload(), 1000);
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
        document.getElementById(tabId).classList.add('active');
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
                  setTimeout(() => location.reload(), 1000);
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

          // 1. Dynamic Options from API
          if (field.options_endpoint) {
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
          // 2. Static Options
          else if (field.field_type === 4 || field.field_type === 'Options') { 
             input = document.createElement('select');
             input.className = 'input';
             input.id = `cfg-dyn-${instance.instance_id}-${field.id}`;
             if (field.options && field.options.length > 0) {
               field.options.forEach(opt => {
                 const option = document.createElement('option');
                 option.value = opt.value;
                 option.innerText = opt.label;
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
  card.style.margin = '1rem 0';

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

      // Left info: order badge + instance title
      const info = document.createElement('div');
      info.className = 'rotation-info';

      const badge = document.createElement('span');
      badge.className = 'rotation-order-badge';
      badge.innerText = `#${idx + 1}`;
      info.appendChild(badge);

      const label = document.createElement('span');
      label.className = 'rotation-name';
      label.title = nameFor(entry.instance_id);
      label.innerText = nameFor(entry.instance_id);
      info.appendChild(label);

      row.appendChild(info);

      // Right controls: duration + unit + fighter + actions
      const controls = document.createElement('div');
      controls.className = 'rotation-controls';

      const countBased = isCountBased(entry.instance_id);

      // Duration input group
      const durGroup = document.createElement('div');
      durGroup.className = 'rotation-dur-group';
      durGroup.title = countBased ? 'Number of GIFs to play before rotating' : 'Duration (seconds)';

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

      controls.appendChild(durGroup);

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
      fightTxt.innerText = '🥊';
      fightLbl.appendChild(fightTxt);
      controls.appendChild(fightLbl);

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

      controls.appendChild(actions);
      row.appendChild(controls);

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

  // Add-to-rotation controls.
  const addRow = document.createElement('div');
  addRow.className = 'rotation-add-row';
  const sel = document.createElement('select');
  sel.className = 'input';
  sel.style = 'flex:1 1 200px; min-width:150px;';
  instancesList.forEach(inst => {
    const opt = document.createElement('option');
    opt.value = inst.instance_id;
    opt.innerText = nameFor(inst.instance_id);
    sel.appendChild(opt);
  });
  const addBtn = document.createElement('button');
  addBtn.className = 'btn btn-primary';
  addBtn.innerText = '➕ Add to rotation';
  addBtn.onclick = () => {
    if (!sel.value) return;
    entries.push({ instance_id: sel.value, duration_sec: 30, fighter_overlay: true });
    render();
  };
  addRow.appendChild(sel);
  addRow.appendChild(addBtn);
  card.appendChild(addRow);

  const saveBtn = document.createElement('button');
  saveBtn.className = 'btn btn-primary';
  saveBtn.style = 'margin-top:1rem; width:100%;';
  saveBtn.innerText = 'Save Rotation';
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
