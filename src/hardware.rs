use std::fs;

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
