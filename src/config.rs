use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

const SYSTEM_CONFIG_PATH: &str = "/etc/rpi-fan-control/config.toml";
const LOCAL_CONFIG_PATH: &str = "config.toml";

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub dry_run: bool,
    pub gpio_pin: u8,
    pub thermal_path: String,
    pub fan_speed_path: Option<String>,
    pub tach_gpio_pin: Option<u8>,
    pub loop_interval_ms: u64,
    pub status_interval_secs: u64,
    pub target_temp_c: i32,
    pub min_duty: u8,
    pub max_duty: u8,
    pub response: ResponseProfile,
}

#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ResponseProfile {
    Quiet,
    #[default]
    Balanced,
    Aggressive,
}

#[derive(Debug, Clone, Copy)]
pub struct CurveConfig {
    pub off_temp_c: i32,
    pub low_temp_c: i32,
    pub low_duty: u8,
    pub mid_temp_c: i32,
    pub mid_duty: u8,
    pub high_temp_c: i32,
    pub high_duty: u8,
    pub full_temp_c: i32,
    pub full_duty: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct SmoothingConfig {
    pub rise_factor_pct: u8,
    pub fall_factor_pct: u8,
    pub rise_max_step: u8,
    pub fall_max_step: u8,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            dry_run: false,
            gpio_pin: 18,
            thermal_path: "/sys/class/thermal/thermal_zone0/temp".to_string(),
            fan_speed_path: None,
            tach_gpio_pin: None,
            loop_interval_ms: 1_000,
            status_interval_secs: 60,
            target_temp_c: 62,
            min_duty: 25,
            max_duty: 100,
            response: ResponseProfile::Balanced,
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<(Self, PathBuf), Box<dyn std::error::Error + Send + Sync>> {
        let candidates = [Path::new(SYSTEM_CONFIG_PATH), Path::new(LOCAL_CONFIG_PATH)];

        for path in candidates {
            if path.exists() {
                let raw = fs::read_to_string(path)?;
                let cfg = toml::from_str::<Self>(&raw)?;
                cfg.validate()?;
                return Ok((cfg, path.to_path_buf()));
            }
        }

        let cfg = Self::default();
        cfg.validate()?;
        Ok((cfg, PathBuf::from("<built-in defaults>")))
    }

    pub fn validate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        if self.loop_interval_ms == 0 {
            return Err("loop_interval_ms must be > 0".into());
        }
        if self.status_interval_secs == 0 {
            return Err("status_interval_secs must be > 0".into());
        }
        if self.thermal_path.trim().is_empty() {
            return Err("thermal_path must not be empty".into());
        }
        if let Some(path) = &self.fan_speed_path {
            if path.trim().is_empty() {
                return Err("fan_speed_path must not be empty when provided".into());
            }
        }
        if let Some(pin) = self.tach_gpio_pin {
            if pin > 27 {
                return Err("tach_gpio_pin must be a valid BCM pin (0..=27)".into());
            }
        }
        if !(35..=85).contains(&self.target_temp_c) {
            return Err("target_temp_c must be in range 35..=85".into());
        }
        if self.min_duty > 100 || self.max_duty > 100 {
            return Err("duty values must be in range 0..=100".into());
        }
        if self.min_duty > self.max_duty {
            return Err("min_duty must be <= max_duty".into());
        }
        Ok(())
    }

    pub fn telemetry_interval_ticks(&self) -> u64 {
        match self.response {
            ResponseProfile::Quiet => 45,
            ResponseProfile::Balanced => 30,
            ResponseProfile::Aggressive => 20,
        }
    }

    pub fn write_deadband(&self) -> u8 {
        match self.response {
            ResponseProfile::Quiet => 2,
            ResponseProfile::Balanced => 1,
            ResponseProfile::Aggressive => 1,
        }
    }

    pub fn hysteresis_millideg(&self) -> i32 {
        match self.response {
            ResponseProfile::Quiet => 800,
            ResponseProfile::Balanced => 500,
            ResponseProfile::Aggressive => 250,
        }
    }

    pub fn smoothing(&self) -> SmoothingConfig {
        match self.response {
            ResponseProfile::Quiet => SmoothingConfig {
                rise_factor_pct: 40,
                fall_factor_pct: 12,
                rise_max_step: 8,
                fall_max_step: 2,
            },
            ResponseProfile::Balanced => SmoothingConfig {
                rise_factor_pct: 55,
                fall_factor_pct: 18,
                rise_max_step: 12,
                fall_max_step: 4,
            },
            ResponseProfile::Aggressive => SmoothingConfig {
                rise_factor_pct: 75,
                fall_factor_pct: 30,
                rise_max_step: 20,
                fall_max_step: 8,
            },
        }
    }

    pub fn curve(&self) -> CurveConfig {
        let duty_span = self.max_duty.saturating_sub(self.min_duty);
        let mid_duty = self.min_duty + duty_span / 2;
        let high_duty = self.min_duty + ((u16::from(duty_span) * 3) / 4) as u8;
        CurveConfig {
            off_temp_c: self.target_temp_c - 12,
            low_temp_c: self.target_temp_c - 7,
            low_duty: self.min_duty,
            mid_temp_c: self.target_temp_c,
            mid_duty,
            high_temp_c: self.target_temp_c + 7,
            high_duty,
            full_temp_c: self.target_temp_c + 14,
            full_duty: self.max_duty,
        }
    }
}
