import { API } from './api.js';

export async function initDynamicEngines() {
  const container = document.getElementById('dynamic-engines-container');
  const tabsContainer = document.querySelector('.tabs');
  
  if (!container || !tabsContainer) return;
  
  try {
    const engines = await API.get('/api/engines');
    
    // Clear container (keep legacy ones, just clear dynamic)
    container.innerHTML = '';
    
    engines.forEach(engine => {
      // 1. Create Tab Button
      const tabId = `tab-dyn-${engine.metadata.id.replace(/\./g, '-')}`;
      
      const tabBtn = document.createElement('button');
      tabBtn.className = 'tab-btn';
      tabBtn.innerHTML = `🧩 ${engine.metadata.name}`;
      tabBtn.onclick = (e) => {
        // Simple manual switch since switchDisplayTab might not cover dynamic IDs
        document.querySelectorAll('#page-display .tab-btn').forEach(btn => btn.classList.remove('active'));
        e.currentTarget.classList.add('active');
        
        document.querySelectorAll('#page-display .tab-pane').forEach(pane => pane.classList.remove('active'));
        document.getElementById(tabId).classList.add('active');
      };
      
      // Append tab button to tabs row
      tabsContainer.appendChild(tabBtn);
      
      // 2. Create Tab Pane
      const tabPane = document.createElement('div');
      tabPane.id = tabId;
      tabPane.className = 'tab-pane';
      
      const card = document.createElement('div');
      card.className = 'card glass shadow';
      card.style.marginTop = '2rem';
      
      const title = document.createElement('h3');
      title.innerText = `⚙️ ${engine.metadata.name} Settings`;
      card.appendChild(title);
      
      const desc = document.createElement('p');
      desc.className = 'text-muted';
      desc.innerText = engine.metadata.category;
      card.appendChild(desc);
      
      const formGrid = document.createElement('div');
      formGrid.className = 'form-grid';
      
      // Generate fields from schema
      if (engine.schema && engine.schema.length > 0) {
        engine.schema.forEach(field => {
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
          
          if (field.field_type === 4 || field.field_type === 7) { // ENUM or LIST
             input = document.createElement('select');
             input.className = 'input';
             input.id = `cfg-dyn-${engine.metadata.id}-${field.id}`;
             if (field.options && field.options.length > 0) {
               field.options.forEach(opt => {
                 const option = document.createElement('option');
                 option.value = opt.value;
                 option.innerText = opt.label;
                 input.appendChild(option);
               });
             }
          } else if (field.field_type === 0) { // BOOLEAN
             input = document.createElement('select');
             input.className = 'input';
             input.id = `cfg-dyn-${engine.metadata.id}-${field.id}`;
             input.innerHTML = `<option value="true">Enabled</option><option value="false">Disabled</option>`;
          } else if (field.field_type === 5) { // COLOR
             input = document.createElement('input');
             input.type = 'color';
             input.className = 'input';
             input.id = `cfg-dyn-${engine.metadata.id}-${field.id}`;
          } else { // STRING, INTEGER, FLOAT
             input = document.createElement('input');
             input.type = (field.field_type === 1 || field.field_type === 2) ? 'number' : 'text';
             input.className = 'input';
             input.id = `cfg-dyn-${engine.metadata.id}-${field.id}`;
          }
          
          if (input) {
            if (field.default_value) {
                input.value = field.default_value;
            }
            formGroup.appendChild(input);
          }
          
          formGrid.appendChild(formGroup);
        });
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
         // Gather values
         const payload = {};
         engine.schema.forEach(field => {
            const el = document.getElementById(`cfg-dyn-${engine.metadata.id}-${field.id}`);
            if (el) {
               payload[field.id] = el.value;
            }
         });
         try {
             // For Sprint 6: we post to a new generic endpoint, but for now fallback to settings
             // Because Sprint 6 is not implemented on backend, this will fail or be ignored.
             await API.post(`/api/engines/${engine.metadata.id}/config`, payload);
             window.showToast('Engine configuration saved!', 'info');
         } catch(e) {
             console.error('API Error:', e);
             window.showToast('Failed to save engine config (Backend Sprint 6 required)', 'warning');
         }
      };
      card.appendChild(saveBtn);
      
      tabPane.appendChild(card);
      container.appendChild(tabPane);
    });
    
  } catch (e) {
    console.error('Failed to load dynamic engines:', e);
  }
}
