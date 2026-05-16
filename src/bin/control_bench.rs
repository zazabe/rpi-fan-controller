use rpi_fan_control::config::AppConfig;
use rpi_fan_control::control::{process_tick, ControllerState};
use std::time::Instant;

fn main() {
    let cfg = AppConfig::default();
    let mut state = ControllerState::new();
    let iterations: usize = 500_000;
    let start = Instant::now();

    for i in 0..iterations {
        let temp = 40_000 + ((i % 45) as i32 * 1_000);
        let _ = process_tick(temp, &cfg, &mut state);
    }

    let elapsed = start.elapsed();
    let per_tick_ns = elapsed.as_nanos() / iterations as u128;
    println!(
        "control_bench iterations={} elapsed_ms={} per_tick_ns={}",
        iterations,
        elapsed.as_millis(),
        per_tick_ns
    );
}
