use crate::config::AppConfig;

#[derive(Debug, Clone)]
pub struct ControllerState {
    current_duty: u8,
    last_written_duty: u8,
    last_temp_millideg: i32,
    initialized: bool,
}

impl ControllerState {
    pub fn new() -> Self {
        Self {
            current_duty: 0,
            last_written_duty: 0,
            last_temp_millideg: 0,
            initialized: false,
        }
    }

    pub fn current_duty(&self) -> u8 {
        self.current_duty
    }
}

impl Default for ControllerState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickResult {
    pub target_duty: u8,
    pub smoothed_duty: u8,
    pub should_write: bool,
}

pub fn process_tick(
    temp_millideg: i32,
    cfg: &AppConfig,
    state: &mut ControllerState,
) -> TickResult {
    let target = map_temp_to_target_duty(temp_millideg, cfg);

    let smoothed = if state.initialized {
        smooth_asymmetric(target, state.current_duty, cfg)
    } else {
        state.initialized = true;
        target
    };

    let should_write = abs_diff(smoothed, state.last_written_duty) >= cfg.write_deadband();

    state.current_duty = smoothed;

    if should_write {
        state.last_written_duty = smoothed;
    }

    state.last_temp_millideg = temp_millideg;

    TickResult {
        target_duty: target,
        smoothed_duty: smoothed,
        should_write,
    }
}

pub fn should_update_target(temp_millideg: i32, cfg: &AppConfig, state: &ControllerState) -> bool {
    if !state.initialized {
        return true;
    }
    (temp_millideg - state.last_temp_millideg).abs() >= cfg.hysteresis_millideg()
}

pub fn map_temp_to_target_duty(temp_millideg: i32, cfg: &AppConfig) -> u8 {
    let c = cfg.curve();
    let t = temp_millideg / 1_000;

    if t <= c.off_temp_c {
        return 0;
    }
    if t <= c.low_temp_c {
        return interpolate(t, c.off_temp_c, 0, c.low_temp_c, c.low_duty);
    }
    if t <= c.mid_temp_c {
        return interpolate(t, c.low_temp_c, c.low_duty, c.mid_temp_c, c.mid_duty);
    }
    if t <= c.high_temp_c {
        return interpolate(t, c.mid_temp_c, c.mid_duty, c.high_temp_c, c.high_duty);
    }
    if t <= c.full_temp_c {
        return interpolate(t, c.high_temp_c, c.high_duty, c.full_temp_c, c.full_duty);
    }
    c.full_duty
}

fn smooth_asymmetric(target: u8, current: u8, cfg: &AppConfig) -> u8 {
    let smoothing = cfg.smoothing();
    if target == current {
        return current;
    }

    if target > current {
        let delta = target - current;
        let scaled = scaled_step(delta, smoothing.rise_factor_pct);
        let step = scaled.min(smoothing.rise_max_step).max(1);
        current.saturating_add(step.min(delta))
    } else {
        let delta = current - target;
        let scaled = scaled_step(delta, smoothing.fall_factor_pct);
        let step = scaled.min(smoothing.fall_max_step).max(1);
        current.saturating_sub(step.min(delta))
    }
}

fn scaled_step(delta: u8, factor_pct: u8) -> u8 {
    let raw = (u16::from(delta) * u16::from(factor_pct)).div_ceil(100);
    raw as u8
}

fn interpolate(x: i32, x0: i32, y0: u8, x1: i32, y1: u8) -> u8 {
    if x1 == x0 {
        return y1;
    }

    let dx = x - x0;
    let span = x1 - x0;
    let y0i = i32::from(y0);
    let y1i = i32::from(y1);
    let delta = y1i - y0i;
    let interpolated = y0i + (delta * dx + span / 2) / span;
    interpolated.clamp(0, 100) as u8
}

fn abs_diff(a: u8, b: u8) -> u8 {
    a.max(b) - a.min(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{AppConfig, ResponseProfile};

    #[test]
    fn temp_mapping_respects_breakpoints() {
        let cfg = AppConfig::default();
        let curve = cfg.curve();

        assert_eq!(
            map_temp_to_target_duty((curve.off_temp_c - 1) * 1_000, &cfg),
            0
        );
        assert_eq!(
            map_temp_to_target_duty(curve.low_temp_c * 1_000, &cfg),
            curve.low_duty
        );
        assert_eq!(
            map_temp_to_target_duty(curve.mid_temp_c * 1_000, &cfg),
            curve.mid_duty
        );
        assert_eq!(
            map_temp_to_target_duty(curve.high_temp_c * 1_000, &cfg),
            curve.high_duty
        );
        assert_eq!(
            map_temp_to_target_duty((curve.full_temp_c + 5) * 1_000, &cfg),
            curve.full_duty
        );
    }

    #[test]
    fn rise_is_faster_than_fall_for_same_delta() {
        let cfg = AppConfig {
            response: ResponseProfile::Aggressive,
            ..AppConfig::default()
        };

        let rising = smooth_asymmetric(80, 40, &cfg);
        let falling = smooth_asymmetric(40, 80, &cfg);

        assert!(rising - 40 > 80 - falling);
    }

    #[test]
    fn deadband_avoids_noisy_pwm_writes() {
        let cfg = AppConfig {
            response: ResponseProfile::Quiet,
            ..AppConfig::default()
        };

        let mut state = ControllerState::new();
        let first = process_tick(65_000, &cfg, &mut state);
        assert!(first.should_write);

        let second = process_tick(65_500, &cfg, &mut state);
        assert!(!second.should_write);
    }
}
