use log::{debug, error, info, warn};
use rpi_fan_control::config::AppConfig;
use rpi_fan_control::control::{process_tick, ControllerState};
use rpi_fan_control::hardware::{
    read_cpu_temp_millideg, read_fan_speed_rpm, DryRunPwmBackend, PwmBackend, TachRpmReader,
};
use std::ffi::OsString;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let config_override = parse_config_arg()?;
    let (cfg, cfg_path) = AppConfig::load(config_override.as_deref())?;
    info!(
        "rpi-fan-control start config_source={} pin={} thermal_path={} fan_speed_path={} tach_gpio_pin={} loop_ms={} status_interval_s={} target_temp_c={} min_duty={} max_duty={} response={:?} dry_run={}",
        cfg_path.display(),
        cfg.gpio_pin,
        cfg.thermal_path,
        cfg.fan_speed_path.as_deref().unwrap_or("<disabled>"),
        cfg.tach_gpio_pin
            .map(|pin| pin.to_string())
            .unwrap_or_else(|| "<disabled>".to_string()),
        cfg.loop_interval_ms,
        cfg.status_interval_secs,
        cfg.target_temp_c,
        cfg.min_duty,
        cfg.max_duty,
        cfg.response,
        cfg.dry_run
    );

    let mut pwm = build_backend(&cfg)?;
    let mut tach_reader = build_tach_reader(&cfg);
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

        let result = process_tick(temp_millideg, &cfg, &mut state);

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
            let fan_rpm = read_fan_rpm(&cfg, tach_reader.as_mut());
            if let Some(fan_rpm) = fan_rpm {
                info!(
                    "status: CPU {temp_c:.1} C, fan {fan_rpm} RPM, duty {}% (target {}%), pwm write {}",
                    result.smoothed_duty,
                    result.target_duty,
                    if result.should_write { "yes" } else { "no" },
                );
            } else {
                info!(
                    "status: CPU {temp_c:.1} C, duty {}% (target {}%), pwm write {}",
                    result.smoothed_duty,
                    result.target_duty,
                    if result.should_write { "yes" } else { "no" },
                );
            }
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

fn parse_config_arg() -> Result<Option<PathBuf>, Box<dyn std::error::Error + Send + Sync>> {
    let mut args = std::env::args_os();
    let _bin = args.next();
    let mut config_path: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        if arg == "--config" {
            let next = args.next().ok_or("--config requires a file path value")?;
            if config_path.replace(PathBuf::from(next)).is_some() {
                return Err("--config may only be provided once".into());
            }
            continue;
        }

        if let Some(value) = parse_inline_config_value(&arg) {
            if value.is_empty() {
                return Err("--config= requires a file path value".into());
            }
            if config_path.replace(PathBuf::from(value)).is_some() {
                return Err("--config may only be provided once".into());
            }
            continue;
        }

        return Err(format!("unknown argument: {}", arg.to_string_lossy()).into());
    }

    Ok(config_path)
}

fn parse_inline_config_value(arg: &OsString) -> Option<String> {
    let value = arg.to_string_lossy();
    value
        .strip_prefix("--config=")
        .map(std::string::ToString::to_string)
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

fn build_tach_reader(cfg: &AppConfig) -> Option<TachRpmReader> {
    let tach_pin = cfg.tach_gpio_pin?;
    match TachRpmReader::new(tach_pin) {
        Ok(reader) => {
            info!("tach reader enabled on BCM pin {}", tach_pin);
            Some(reader)
        }
        Err(err) => {
            warn!(
                "tach reader unavailable on pin {} ({}); RPM logging disabled unless hwmon works",
                tach_pin, err
            );
            None
        }
    }
}

fn read_fan_rpm(cfg: &AppConfig, tach_reader: Option<&mut TachRpmReader>) -> Option<u32> {
    if let Some(path) = cfg.fan_speed_path.as_deref() {
        match read_fan_speed_rpm(path) {
            Ok(rpm) => return Some(rpm),
            Err(err) => warn!("hwmon fan speed read failed at {} ({})", path, err),
        }
    }

    if let Some(reader) = tach_reader {
        match reader.read_rpm() {
            Ok(rpm) => return Some(rpm),
            Err(err) => warn!("tach RPM read failed ({})", err),
        }
    }

    None
}
