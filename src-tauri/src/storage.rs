use serde_json::Value;
use tauri::{AppHandle, Manager};

/// 配置格式版本号。载入时校验，未来数据模型变更可据此迁移。
const CONFIG_VERSION: u32 = 1;

/// 将配置保存到应用数据目录下的 config.json
///
/// 为支持版本控制与后续迁移，实际落盘格式为带版本信封的包裹：
/// `{ "version": 1, "config": <原始配置> }`。
/// 前端仍按原始 `AppConfig` 收发，版本细节对上层透明。
#[tauri::command]
pub fn save_config(app: AppHandle, config: Value) -> Result<(), String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("获取数据目录失败: {e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录失败: {e}"))?;
    let path = dir.join("config.json");
    let envelope = serde_json::json!({ "version": CONFIG_VERSION, "config": config });
    let json = serde_json::to_string_pretty(&envelope).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| format!("保存配置失败: {e}"))
}

/// 从应用数据目录加载配置，无配置时返回 None。
///
/// 加载时：
/// 1. 含 `version` 信封 → 校验版本并取出内层 `config`；
/// 2. 旧版（无 `version`，v0） → 整体作为配置返回（兼容迁移）；
/// 3. 版本不被支持 → 返回 None，交由上层使用默认配置（避免静默不一致）。
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
    let parsed: Value = serde_json::from_str(&content).map_err(|e| format!("解析配置失败: {e}"))?;

    match parsed.get("version") {
        Some(Value::Number(n)) => {
            let v = n.as_u64().unwrap_or(0);
            if v != CONFIG_VERSION as u64 {
                // 未来可在此插入迁移逻辑（migrate(v, inner)）；
                // 当前仅支持 v1，未知版本视为不可读，回退默认。
                return Ok(None);
            }
            Ok(parsed.get("config").cloned())
        }
        // 旧版无信封：整体视为配置（v0 兼容迁移）
        None => Ok(Some(parsed)),
        // 信封结构损坏：无法安全解析，回退默认
        Some(_) => Ok(None),
    }
}
