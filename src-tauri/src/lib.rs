mod models;
mod solver;
mod storage;

use models::{EnumerateResponse, SolveRequest, SolveResult};
use std::time::Instant;

/// 最优解求解命令：接收输入，调用求解器，返回结果
#[tauri::command]
fn solve_optimal(request: SolveRequest) -> Result<SolveResult, String> {
    let start = Instant::now();
    let mut result = solver::solve(&request)?;
    result.solve_time_ms = start.elapsed().as_millis() as u64;
    Ok(result)
}

/// 遍历备选方案命令：循环 ILP + no-good cut 生成多个互不相同的方案
#[tauri::command]
fn enumerate_solutions(
    request: SolveRequest,
    max_solutions: Option<usize>,
) -> Result<EnumerateResponse, String> {
    // 上限保护：默认 50，最多 200，防止解空间过大导致耗时失控
    let max = max_solutions.unwrap_or(50).clamp(1, 200);
    let start = Instant::now();
    let (mut sols, truncated) = solver::enumerate_solutions(&request, max)?;
    let elapsed = start.elapsed().as_millis() as u64;
    for s in sols.iter_mut() {
        s.solve_time_ms = elapsed;
    }
    Ok(EnumerateResponse { solutions: sols, truncated })
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
