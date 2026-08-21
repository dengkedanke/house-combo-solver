mod models;
mod solver;
mod storage;

use models::{EnumerateResponse, SolveRequest, SolveResult};
use std::time::Instant;

/// 最优解求解命令：接收输入，调用求解器，返回结果。
/// #3 修复：改为 async + spawn_blocking，将 ILP 求解/均衡化等重活移出
/// 同步 IPC 处理线程，避免极端情况下阻塞后台线程池。
#[tauri::command]
async fn solve_optimal(request: SolveRequest) -> Result<SolveResult, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let start = Instant::now();
        let mut result = solver::solve(&request)?;
        result.solve_time_ms = start.elapsed().as_millis() as u64;
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 遍历备选方案命令：循环 ILP + no-good cut 生成多个互不相同的方案。
/// #3 修复：同 solve_optimal，异步 + spawn_blocking。
#[tauri::command]
async fn enumerate_solutions(
    request: SolveRequest,
    max_solutions: Option<usize>,
) -> Result<EnumerateResponse, String> {
    // 上限保护：默认 50，最多 200，防止解空间过大导致耗时失控
    let max = max_solutions.unwrap_or(50).clamp(1, 200);
    tauri::async_runtime::spawn_blocking(move || {
        let start = Instant::now();
        let (mut sols, truncated) = solver::enumerate_solutions(&request, max)?;
        let elapsed = start.elapsed().as_millis() as u64;
        for s in sols.iter_mut() {
            s.solve_time_ms = elapsed;
        }
        Ok(EnumerateResponse { solutions: sols, truncated })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            solve_optimal,
            enumerate_solutions,
            storage::save_config,
            storage::load_config
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
