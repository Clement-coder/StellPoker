use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::SystemTime;

use tokio::sync::RwLock;
use tracing;
use wasmtime::{Engine, Instance, Linker, Module, Store};

use super::sandbox;
use super::types::{BlindStructure, LoadedPlugin, PluginInfo, PluginManifest};

const PLUGIN_WASM_DIR: &str = "./plugins";

pub type PluginRegistry = Arc<RwLock<HashMap<String, Arc<LoadedPlugin>>>>;

fn read_wasm_string(
    instance: &Instance,
    store: &mut Store<()>,
    ptr: i32,
    len: i32,
) -> Result<String, String> {
    let memory = instance
        .get_memory(&mut *store, "memory")
        .ok_or_else(|| "plugin has no exported memory".to_string())?;

    let mut buffer = vec![0u8; len as usize];
    memory
        .read(&mut *store, ptr as usize, &mut buffer)
        .map_err(|e| format!("failed to read plugin memory: {}", e))?;

    String::from_utf8(buffer).map_err(|e| format!("invalid UTF-8 in plugin string: {}", e))
}

fn copy_to_wasm_memory(
    instance: &Instance,
    store: &mut Store<()>,
    data: &[u8],
) -> Result<i32, String> {
    let memory = instance
        .get_memory(&mut *store, "memory")
        .ok_or_else(|| "plugin has no exported memory".to_string())?;

    let alloc = instance
        .get_typed_func::<(i32,), i32>(&mut *store, "alloc")
        .map_err(|_| "plugin does not export alloc function".to_string())?;

    let ptr = alloc
        .call(&mut *store, (data.len() as i32,))
        .map_err(|e| format!("alloc failed: {}", e))?;

    memory
        .write(&mut *store, ptr as usize, data)
        .map_err(|e| format!("failed to write plugin memory: {}", e))?;

    Ok(ptr)
}

pub struct PluginLoader {
    engine: Engine,
    plugins: PluginRegistry,
}

impl PluginLoader {
    pub fn new(engine: Engine) -> Self {
        Self {
            engine,
            plugins: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn plugins(&self) -> &PluginRegistry {
        &self.plugins
    }

    pub async fn load_plugin_from_path(&self, wasm_path: &Path) -> Result<String, String> {
        let wasm_bytes =
            std::fs::read(wasm_path).map_err(|e| format!("failed to read wasm file: {}", e))?;
        self.load_plugin_from_bytes(&wasm_bytes, wasm_path).await
    }

    pub async fn load_plugin_from_bytes(
        &self,
        wasm_bytes: &[u8],
        source: &Path,
    ) -> Result<String, String> {
        let module = Module::new(&self.engine, wasm_bytes)
            .map_err(|e| format!("invalid wasm module: {}", e))?;

        let mut linker = Linker::new(&self.engine);
        self.register_host_functions(&mut linker)?;

        let mut store = sandbox::create_sandbox_store(&self.engine, ())?;
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(|e| format!("failed to instantiate plugin: {}", e))?;

        let name = self.read_plugin_metadata(&instance, &mut store)?;
        let version = self.read_plugin_version(&instance, &mut store)?;

        let manifest = PluginManifest {
            name: name.clone(),
            version,
            description: String::new(),
            author: None,
            game_type: "custom".to_string(),
        };
        manifest.validate()?;

        let path_str = source.to_string_lossy().to_string();
        let plugin_info = PluginInfo {
            manifest,
            path: path_str,
            loaded_at: SystemTime::now(),
            memory_usage_bytes: wasm_bytes.len() as u64,
            active_tables: Vec::new(),
        };

        let loaded = LoadedPlugin {
            info: plugin_info,
            module: Arc::new(module),
            instance: Arc::new(instance),
            linker: Arc::new(linker),
        };

        let mut plugins = self.plugins.write().await;
        plugins.insert(name.clone(), Arc::new(loaded));

        tracing::info!("Loaded plugin '{}' from {}", name, source.display());
        Ok(name)
    }

    pub async fn unload_plugin(&self, name: &str) -> Result<(), String> {
        let mut plugins = self.plugins.write().await;
        plugins
            .remove(name)
            .ok_or_else(|| format!("plugin '{}' not found", name))?;
        tracing::info!("Unloaded plugin '{}'", name);
        Ok(())
    }

    pub async fn scan_plugin_directory(&self, dir: Option<&Path>) -> Vec<String> {
        let dir = dir.unwrap_or_else(|| Path::new(PLUGIN_WASM_DIR));
        if !dir.exists() {
            tracing::warn!("Plugin directory {:?} does not exist", dir);
            return Vec::new();
        }

        let mut loaded = Vec::new();
        let mut entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::error!("Failed to read plugin directory: {}", e);
                return Vec::new();
            }
        };

        while let Some(Ok(entry)) = entries.next() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("wasm") {
                match self.load_plugin_from_path(&path).await {
                    Ok(name) => {
                        tracing::info!("Auto-loaded plugin '{}' from {}", name, path.display());
                        loaded.push(name);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load plugin {}: {}", path.display(), e);
                    }
                }
            }
        }

        loaded
    }

    pub async fn call_evaluate_hand(
        &self,
        plugin_name: &str,
        cards: &[u32; 7],
    ) -> Result<u32, String> {
        let plugins = self.plugins.read().await;
        let loaded = plugins
            .get(plugin_name)
            .ok_or_else(|| format!("plugin '{}' not found", plugin_name))?;

        let mut store = sandbox::create_sandbox_store(&self.engine, ())?;
        let func = loaded
            .instance
            .get_typed_func::<(i32,), i32>(&mut store, "evaluate_hand")
            .map_err(|_| "plugin does not export evaluate_hand".to_string())?;

        let bytes: Vec<u8> = cards.iter().flat_map(|c| c.to_le_bytes()).collect();
        let ptr = copy_to_wasm_memory(&loaded.instance, &mut store, &bytes)?;

        let result = func
            .call(&mut store, (ptr,))
            .map_err(|e| format!("evaluate_hand call failed: {}", e))?;

        Ok(result as u32)
    }

    pub async fn call_get_blind_structure(
        &self,
        plugin_name: &str,
        small_blind: i128,
        big_blind: i128,
        level: u32,
    ) -> Result<BlindStructure, String> {
        let plugins = self.plugins.read().await;
        let loaded = plugins
            .get(plugin_name)
            .ok_or_else(|| format!("plugin '{}' not found", plugin_name))?;

        let mut store = sandbox::create_sandbox_store(&self.engine, ())?;
        let func = loaded
            .instance
            .get_typed_func::<(i64, i64, i32), i64>(&mut store, "get_blind_amounts")
            .map_err(|_| "plugin does not export get_blind_amounts".to_string())?;

        let packed = func
            .call(
                &mut store,
                (small_blind as i64, big_blind as i64, level as i32),
            )
            .map_err(|e| format!("get_blind_amounts call failed: {}", e))?;

        let sb = (packed >> 32) as i128;
        let bb = (packed & 0xFFFF_FFFF) as i128;

        Ok(BlindStructure {
            small_blind: sb,
            big_blind: bb,
            ante: 0,
        })
    }

    pub async fn call_can_raise(&self, plugin_name: &str, round: u32) -> Result<bool, String> {
        let plugins = self.plugins.read().await;
        let loaded = plugins
            .get(plugin_name)
            .ok_or_else(|| format!("plugin '{}' not found", plugin_name))?;

        let mut store = sandbox::create_sandbox_store(&self.engine, ())?;
        let func = loaded
            .instance
            .get_typed_func::<(i32,), i32>(&mut store, "can_raise")
            .map_err(|_| "plugin does not export can_raise".to_string())?;

        let result = func
            .call(&mut store, (round as i32,))
            .map_err(|e| format!("can_raise call failed: {}", e))?;

        Ok(result != 0)
    }

    pub async fn call_get_betting_rounds(&self, plugin_name: &str) -> Result<u32, String> {
        let plugins = self.plugins.read().await;
        let loaded = plugins
            .get(plugin_name)
            .ok_or_else(|| format!("plugin '{}' not found", plugin_name))?;

        let mut store = sandbox::create_sandbox_store(&self.engine, ())?;
        let func = loaded
            .instance
            .get_typed_func::<(), i32>(&mut store, "get_betting_rounds")
            .map_err(|_| "plugin does not export get_betting_rounds".to_string())?;

        let result = func
            .call(&mut store, ())
            .map_err(|e| format!("get_betting_rounds call failed: {}", e))?;

        Ok(result as u32)
    }

    fn read_plugin_metadata(
        &self,
        instance: &Instance,
        store: &mut Store<()>,
    ) -> Result<String, String> {
        let func = instance
            .get_typed_func::<(), (i32, i32)>(&mut *store, "plugin_name")
            .map_err(|_| "plugin does not export plugin_name".to_string())?;

        let (ptr, len) = func
            .call(&mut *store, ())
            .map_err(|e| format!("plugin_name call failed: {}", e))?;

        read_wasm_string(instance, store, ptr, len)
    }

    fn read_plugin_version(
        &self,
        instance: &Instance,
        store: &mut Store<()>,
    ) -> Result<String, String> {
        let func = instance
            .get_typed_func::<(), (i32, i32)>(&mut *store, "plugin_version")
            .map_err(|_| "plugin does not export plugin_version".to_string())?;

        let (ptr, len) = func
            .call(&mut *store, ())
            .map_err(|e| format!("plugin_version call failed: {}", e))?;

        read_wasm_string(instance, store, ptr, len)
    }

    fn register_host_functions(&self, linker: &mut Linker<()>) -> Result<(), String> {
        linker
            .func_wrap("env", "log_message", |ptr: i32, len: i32| {
                tracing::info!("[plugin log] ptr={}, len={}", ptr, len);
            })
            .map_err(|e| format!("failed to register log_message: {}", e))?;

        Ok(())
    }
}

impl std::fmt::Debug for PluginLoader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginLoader")
            .field("plugins", &"PluginRegistry")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasmtime::Config;

    fn test_engine() -> Engine {
        let config = Config::new();
        Engine::new(&config).unwrap()
    }

    #[tokio::test]
    async fn test_plugin_loader_creation() {
        let engine = test_engine();
        let loader = PluginLoader::new(engine);
        assert!(loader.plugins().try_read().unwrap().is_empty());
    }
}
