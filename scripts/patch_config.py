import os

file_path = "src/core/config.rs"

with open(file_path, "r") as f:
    content = f.read()

# Add new structs
structs = """
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInstance {
    pub instance_id: String,
    pub engine_id: String,
    pub config: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotationEntry {
    pub instance_id: String,
    pub duration_sec: u32,
}

"""

if "pub struct EngineInstance" not in content:
    content = content.replace("#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct ConfigSettings {", structs + "#[derive(Debug, Clone, Serialize, Deserialize)]\npub struct ConfigSettings {")
    content = content.replace("pub wifi_disable_internal: bool,", "    pub wifi_disable_internal: bool,\n    pub instances: Vec<EngineInstance>,\n    pub rotation: Vec<RotationEntry>,")
    
    # Update default
    content = content.replace("wifi_disable_internal: false,", "wifi_disable_internal: false,\n            instances: vec![],\n            rotation: vec![],")

# Now rewrite Config implementation to support config.json
if "config.json" not in content:
    old_impl = """impl Config {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {"""
    
    new_impl = """impl Config {
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        let path = path.as_ref().to_path_buf();
        let json_path = path.with_file_name("config.json");
        
        // 1. Try loading JSON first
        if json_path.exists() {
            if let Ok(json_str) = std::fs::read_to_string(&json_path) {
                if let Ok(settings) = serde_json::from_str::<ConfigSettings>(&json_str) {
                    return Self {
                        settings: RwLock::new(settings),
                        path,
                        json_path,
                    };
                }
            }
        }
        
        // 2. Fallback to INI, then migrate
        let mut cfg = Self {
            settings: RwLock::new(ConfigSettings::default()),
            path: path.clone(),
            json_path: json_path.clone(),
        };
        
        if path.exists() {
            cfg.load_legacy_ini();
        }
        
        cfg.migrate_to_json();
        cfg.save(); // save to json
        cfg
    }
    
    fn migrate_to_json(&mut self) {
        let mut s = self.settings.write();
        if s.instances.is_empty() {
            // Migrate Clock
            let mut clock_cfg = std::collections::HashMap::new();
            clock_cfg.insert("theme".to_string(), s.time_theme.to_string());
            clock_cfg.insert("format".to_string(), s.time_format.clone());
            s.instances.push(EngineInstance {
                instance_id: "default_clock".to_string(),
                engine_id: "clock".to_string(),
                config: clock_cfg,
            });
            
            // Migrate Weather
            let mut weather_cfg = std::collections::HashMap::new();
            weather_cfg.insert("city".to_string(), s.weather_city.clone());
            weather_cfg.insert("api_key".to_string(), s.weather_api_key.clone());
            s.instances.push(EngineInstance {
                instance_id: "default_weather".to_string(),
                engine_id: "weather".to_string(),
                config: weather_cfg,
            });
            
            // Build rotation
            s.rotation = vec![
                RotationEntry { instance_id: "default_clock".to_string(), duration_sec: s.idle_clock_duration_sec },
                RotationEntry { instance_id: "default_weather".to_string(), duration_sec: s.idle_weather_duration_sec },
            ];
        }
    }
    
    pub fn save(&self) {
        let s = self.settings.read();
        if let Ok(json) = serde_json::to_string_pretty(&*s) {
            let _ = std::fs::write(&self.json_path, json);
        }
    }
    
    fn load_legacy_ini(&mut self) {"""
    
    content = content.replace(old_impl, new_impl)
    
    # Add json_path to Config struct
    content = content.replace("pub struct Config {\n    settings: RwLock<ConfigSettings>,\n    path: PathBuf,\n}", "pub struct Config {\n    settings: RwLock<ConfigSettings>,\n    path: PathBuf,\n    json_path: PathBuf,\n}")
    
with open(file_path, "w") as f:
    f.write(content)

print("config.rs patched successfully")
