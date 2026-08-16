use serde_json::Value;
use tauri::{AppHandle, Manager};

/// 将配置保存到应用数据目录下的 config.json
#[tauri::command]
pub fn save_config(app: AppHandle, config: Value) -> Result<(), String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取数据目录失败: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录失败: {e}"))?;
    let path = dir.join("config.json");
    let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("保存配置失败: {e}"))
}

/// 从应用数据目录加载配置，无配置时返回 None
#[tauri::command]
pub fn load_config(app: AppHandle) -> Result<Option<Value>, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取数据目录失败: {e}"))?;
    let path = dir.join("config.json");
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path).map_err(|e| format!("读取配置失败: {e}"))?;
    serde_json::from_str(&content)
        .map(Some)
        .map_err(|e| format!("解析配置失败: {e}"))
}
