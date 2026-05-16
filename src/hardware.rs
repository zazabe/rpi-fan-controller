use std::fs;
#[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
use std::thread;
#[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
use std::time::Duration;
#[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
use std::time::Instant;

pub trait PwmBackend {
    fn set_duty_percent(
        &mut self,
        duty_percent: u8,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
}

pub struct DryRunPwmBackend;

impl PwmBackend for DryRunPwmBackend {
    fn set_duty_percent(
        &mut self,
        _duty_percent: u8,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

#[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
pub struct RpiGpioPwmBackend {
    pin: rppal::gpio::OutputPin,
}

#[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
impl RpiGpioPwmBackend {
    pub fn new(gpio_pin: u8) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        use rppal::gpio::Gpio;
        let gpio = Gpio::new()?;
        let pin = gpio.get(gpio_pin)?.into_output();
        Ok(Self { pin })
    }
}

#[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
impl PwmBackend for RpiGpioPwmBackend {
    fn set_duty_percent(
        &mut self,
        duty_percent: u8,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.pin
            .set_pwm_frequency(25_000.0, f64::from(duty_percent) / 100.0)?;
        Ok(())
    }
}

pub fn read_cpu_temp_millideg(
    thermal_path: &str,
) -> Result<i32, Box<dyn std::error::Error + Send + Sync>> {
    let raw = fs::read_to_string(thermal_path)?;
    let parsed = raw.trim().parse::<i32>()?;
    Ok(parsed)
}

pub fn read_fan_speed_rpm(path: &str) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
    let raw = fs::read_to_string(path)?;
    let parsed = raw.trim().parse::<u32>()?;
    Ok(parsed)
}

#[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
pub struct TachRpmReader {
    sample_window: Duration,
    poll_delay: Duration,
    pulses_per_rev: u32,
    pin: rppal::gpio::InputPin,
}

#[cfg(not(any(target_arch = "arm", target_arch = "aarch64")))]
pub struct TachRpmReader;

#[cfg(any(target_arch = "arm", target_arch = "aarch64"))]
impl TachRpmReader {
    pub fn new(tach_gpio_pin: u8) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        use rppal::gpio::Gpio;
        let gpio = Gpio::new()?;
        let pin = gpio.get(tach_gpio_pin)?.into_input_pullup();
        Ok(Self {
            sample_window: Duration::from_millis(300),
            poll_delay: Duration::from_micros(500),
            pulses_per_rev: 2,
            pin,
        })
    }

    pub fn read_rpm(&mut self) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
        let start = Instant::now();
        let mut rising_edges: u32 = 0;
        let mut previous_high = self.pin.is_high();

        while start.elapsed() < self.sample_window {
            let current_high = self.pin.is_high();
            if !previous_high && current_high {
                rising_edges = rising_edges.saturating_add(1);
            }
            previous_high = current_high;
            thread::sleep(self.poll_delay);
        }

        let window_secs = self.sample_window.as_secs_f64();
        let hz = f64::from(rising_edges) / window_secs;
        let rpm = (hz * 60.0 / f64::from(self.pulses_per_rev)).round() as u32;
        Ok(rpm)
    }
}

#[cfg(not(any(target_arch = "arm", target_arch = "aarch64")))]
impl TachRpmReader {
    pub fn new(_tach_gpio_pin: u8) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Err("tach GPIO RPM requires arm/aarch64 with rppal".into())
    }

    pub fn read_rpm(&mut self) -> Result<u32, Box<dyn std::error::Error + Send + Sync>> {
        Err("tach GPIO RPM requires arm/aarch64 with rppal".into())
    }
}
