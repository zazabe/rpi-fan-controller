use clap::{Parser, Subcommand};
use log::{debug, error, info, warn};
use rpi_fan_control::config::AppConfig;
use rpi_fan_control::control::{process_tick, ControllerState};
use rpi_fan_control::hardware::{
    read_cpu_temp_millideg, read_fan_speed_rpm, DryRunPwmBackend, PwmBackend, TachRpmReader,
};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_DISCOVERY_PINS: [u8; 12] = [17, 22, 23, 24, 25, 27, 5, 6, 16, 20, 21, 26];

#[derive(Debug, Parser)]
#[command(name = "rpi-fan-control")]
#[command(about = "Low-overhead Raspberry Pi PWM fan controller")]
struct Cli {
    #[arg(long, global = true, value_name = "FILE")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Run,
    DiscoverTach {
        #[arg(long = "pin", value_name = "BCM_PIN", value_delimiter = ',')]
        pins: Vec<u8>,
        #[arg(long, default_value_t = 100)]
        duty: u8,
        #[arg(long, default_value_t = 4)]
        samples: u8,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();
    let (cfg, cfg_path) = AppConfig::load(cli.config.as_deref())?;

    match cli.command {
        None | Some(Command::Run) => run_controller(cfg, cfg_path),
        Some(Command::DiscoverTach {
            pins,
            duty,
            samples,
        }) => discover_tach(cfg, cfg_path, pins, duty, samples),
    }
}

fn run_controller(
    cfg: AppConfig,
    cfg_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

fn discover_tach(
    cfg: AppConfig,
    cfg_path: PathBuf,
    pins: Vec<u8>,
    duty: u8,
    samples: u8,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let sample_count = samples.max(1);
    let clamped_duty = duty.clamp(cfg.min_duty, 100);
    let mut candidates = if pins.is_empty() {
        DEFAULT_DISCOVERY_PINS.to_vec()
    } else {
        pins
    };
    candidates.sort_unstable();
    candidates.dedup();

    info!(
        "tach discovery start config_source={} candidates={:?} duty={} samples={}",
        cfg_path.display(),
        candidates,
        clamped_duty,
        sample_count
    );

    let mut pwm = build_backend(&cfg)?;
    if let Err(err) = pwm.set_duty_percent(clamped_duty) {
        warn!(
            "unable to set duty to {}% for tach discovery ({})",
            clamped_duty, err
        );
    } else {
        info!(
            "set duty to {}% for tach discovery, waiting for fan to stabilize",
            clamped_duty
        );
        thread::sleep(Duration::from_millis(800));
    }

    let mut results: Vec<(u8, u32)> = Vec::new();
    for pin in candidates {
        if pin == cfg.gpio_pin {
            warn!("skip BCM {} because it is used for PWM output", pin);
            continue;
        }

        let mut reader = match TachRpmReader::new(pin) {
            Ok(reader) => reader,
            Err(err) => {
                warn!("skip BCM {} ({})", pin, err);
                continue;
            }
        };

        let mut values: Vec<u32> = Vec::with_capacity(usize::from(sample_count));
        for _ in 0..sample_count {
            match reader.read_rpm() {
                Ok(rpm) => values.push(rpm),
                Err(err) => {
                    warn!("BCM {} tach read failed ({})", pin, err);
                    values.clear();
                    break;
                }
            }
        }

        if values.is_empty() {
            continue;
        }

        let avg_rpm = values.iter().copied().sum::<u32>() / values.len() as u32;
        if avg_rpm > 0 {
            info!(
                "BCM {} candidate RPM samples={:?} avg={}",
                pin, values, avg_rpm
            );
            results.push((pin, avg_rpm));
        } else {
            info!("BCM {} no tach pulses detected", pin);
        }
    }

    results.sort_by(|a, b| b.1.cmp(&a.1));
    if let Some((best_pin, best_rpm)) = results.first().copied() {
        info!(
            "tach discovery best pin: BCM {} (~{} RPM), add `tach_gpio_pin = {}`",
            best_pin, best_rpm, best_pin
        );
        for (pin, rpm) in results {
            info!("tach discovery result: BCM {} => ~{} RPM", pin, rpm);
        }
    } else {
        warn!("tach discovery found no RPM signal. Check wiring, common ground, and pull-up.");
    }

    Ok(())
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
