mod models;
mod solver;
mod storage;

use models::{SolveRequest, SolveResult};
use std::time::Instant;

/// 最优解求解命令：接收输入，调用求解器，返回结果
#[tauri::command]
fn solve_optimal(request: SolveRequest) -> Result<SolveResult, String> {
    let start = Instant::now();
    let mut result = solver::solve(&request)?;
    result.solve_time_ms = start.elapsed().as_millis() as u64;
    Ok(result)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            solve_optimal,
            storage::save_config,
            storage::load_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
