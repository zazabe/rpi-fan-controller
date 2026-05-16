use log::{debug, error, info, warn};
use rpi_fan_control::config::AppConfig;
use rpi_fan_control::control::{process_tick, should_update_target, ControllerState, TickResult};
use rpi_fan_control::hardware::{
    read_cpu_temp_millideg, read_fan_speed_rpm, DryRunPwmBackend, PwmBackend,
};
use std::thread;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let (cfg, cfg_path) = AppConfig::load()?;
    info!(
        "rpi-fan-control start config_source={} pin={} thermal_path={} fan_speed_path={} loop_ms={} status_interval_s={} target_temp_c={} min_duty={} max_duty={} response={:?} dry_run={}",
        cfg_path.display(),
        cfg.gpio_pin,
        cfg.thermal_path,
        cfg.fan_speed_path.as_deref().unwrap_or("<disabled>"),
        cfg.loop_interval_ms,
        cfg.status_interval_secs,
        cfg.target_temp_c,
        cfg.min_duty,
        cfg.max_duty,
        cfg.response,
        cfg.dry_run
    );

    let mut pwm = build_backend(&cfg)?;
    let mut state = ControllerState::new();
    let mut tick: u64 = 0;
    let tick_sleep = Duration::from_millis(cfg.loop_interval_ms);
    let status_interval = Duration::from_secs(cfg.status_interval_secs);
    let mut next_status_at = Instant::now() + status_interval;

    loop {
        let start = Instant::now();

        let temp_millideg = match read_cpu_temp_millideg(&cfg.thermal_path) {
            Ok(temp) => temp,
            Err(err) => {
                error!("temp_read_failed error={}", err);
                thread::sleep(tick_sleep);
                continue;
            }
        };

        let result = if should_update_target(temp_millideg, &cfg, &state) {
            process_tick(temp_millideg, &cfg, &mut state)
        } else {
            TickResult {
                target_duty: state.current_duty(),
                smoothed_duty: state.current_duty(),
                should_write: false,
            }
        };

        if result.should_write {
            if let Err(err) = pwm.set_duty_percent(result.smoothed_duty) {
                error!(
                    "pwm_write_failed duty={} error={}",
                    result.smoothed_duty, err
                );
            } else {
                debug!("pwm_write duty={}", result.smoothed_duty);
            }
        }

        tick = tick.saturating_add(1);
        if start >= next_status_at {
            let temp_c = f64::from(temp_millideg) / 1_000.0;
            let fan_speed = match cfg.fan_speed_path.as_deref() {
                Some(path) => match read_fan_speed_rpm(path) {
                    Ok(rpm) => format!("{rpm} RPM"),
                    Err(err) => format!("unavailable ({err})"),
                },
                None => "disabled".to_string(),
            };

            info!(
                "status: CPU {temp_c:.1} C, fan {fan_speed}, duty {}% (target {}%), pwm write {}",
                result.smoothed_duty,
                result.target_duty,
                if result.should_write { "yes" } else { "no" },
            );
            while next_status_at <= start {
                next_status_at += status_interval;
            }
        }

        if tick.is_multiple_of(cfg.telemetry_interval_ticks()) {
            debug!(
                "telemetry tick={} temp_mc={} target={} duty={}",
                tick, temp_millideg, result.target_duty, result.smoothed_duty
            );
        }

        let elapsed = start.elapsed();
        if elapsed < tick_sleep {
            thread::sleep(tick_sleep - elapsed);
        }
    }
}

fn build_backend(
    cfg: &AppConfig,
) -> Result<Box<dyn PwmBackend>, Box<dyn std::error::Error + Send + Sync>> {
    if cfg.dry_run {
        warn!("using dry-run backend");
        return Ok(Box::new(DryRunPwmBackend));
    }

    #[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
    {
        let backend = rpi_fan_control::hardware::RpiGpioPwmBackend::new(cfg.gpio_pin)?;
        return Ok(Box::new(backend));
    }

    #[cfg(not(any(target_arch = "arm", target_arch = "aarch64")))]
    {
        warn!("non-rpi architecture detected; forcing dry-run backend");
        Ok(Box::new(DryRunPwmBackend))
    }
}
