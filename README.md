# rpi-fan-control

Low-overhead Raspberry Pi fan controller written in Rust. It reads CPU temperature from thermal sysfs and drives a PWM signal with an asymmetric control curve (faster ramp-up, slower ramp-down).

> **Tested on:** Raspberry Pi 4 Model B Rev 1.5 (`aarch64`), Debian 13 (Trixie), kernel `6.18.29+rpt-rpi-v8`, with Noctua [NF-A4x10 5V PWM](https://www.noctua.at/en/products/nf-a4x10-5v-pwm); validated via both manual run and `systemd` service mode.

## Hardware Notes

This project targets 4-wire PWM fans such as the Noctua NF-A4x10 5V PWM.

- Power fan VCC/GND from a stable 5V source.
- Drive the fan PWM input using open-drain/transistor style wiring from the selected GPIO pin.
- Fan tach wiring is optional. RPM logging can use `hwmon` or a direct tach GPIO input.
- Verify common ground between Raspberry Pi and fan power source.
- For low CPU usage, enable hardware PWM in boot config (for example `dtoverlay=pwm-2chan`) and reboot.
- If hardware PWM is unavailable, the app automatically falls back to software PWM and logs a warning at startup.

## Configuration

At startup, the app loads config from:

1. `--config <path>` when provided on the command line
2. `/etc/rpi-fan-control/config.toml`
3. `./config.toml`
4. built-in defaults

Reference example: `packaging/config.toml.example`.

Important keys:

- `gpio_pin`: BCM pin used for PWM output. `12`/`18` (PWM0) and `13`/`19` (PWM1) use hardware PWM (preferred, low CPU). Other pins work via automatic software-PWM fallback (higher CPU, warning logged).
- `thermal_path`: Linux sysfs temperature source path.
- `fan_speed_path`: optional Linux sysfs tach path for RPM status logging (first choice).
- `tach_gpio_pin`: optional BCM GPIO input for tach RPM fallback (uses internal pull-up).
- `loop_interval_ms`: control loop period (default `1000`).
- `status_interval_secs`: interval for `info` status logs in `journalctl` (default `60`).
- `target_temp_c`: desired cooling target used to derive the fan curve.
- `min_duty`: minimum duty used once fan turns on.
- `max_duty`: maximum duty cap.
- `response`: one of `quiet`, `balanced`, `aggressive`.
- `dry_run`: skips physical PWM writes.

The controller derives detailed curve/smoothing internals from this simple config. `response` adjusts hysteresis, deadband, and ramp dynamics without exposing low-level tuning knobs.

## Local Run

```bash
cargo run
```

Run with a custom config file:

```bash
cargo run -- --config ./config.toml run
```

Tach auto-discovery (probe candidate GPIO pins and report RPM):

```bash
cargo run -- --config ./config.toml discover-tach
```

Show CLI version:

```bash
cargo run -- --version
```

Enable debug telemetry:

```bash
RUST_LOG=debug cargo run
```

Run benchmark smoke target:

```bash
cargo run --release --bin control_bench
```

## Install on Raspberry Pi (systemd)

Quick install from GitHub Releases:

```bash
curl -fsSL https://raw.githubusercontent.com/zazabe/rpi-fan-controller/main/install.sh | sudo bash
```

Install a specific release tag:

```bash
curl -fsSL https://raw.githubusercontent.com/zazabe/rpi-fan-controller/main/install.sh | sudo bash -s -- v0.1.1
```

Manual install:

```bash
sudo install -D -m 0755 target/release/rpi-fan-control /usr/local/bin/rpi-fan-control
sudo install -D -m 0644 packaging/rpi-fan-control.service /etc/systemd/system/rpi-fan-control.service
sudo install -D -m 0644 packaging/config.toml.example /etc/rpi-fan-control/config.toml
sudo install -D -m 0644 packaging/rpi-fan-control.default /etc/default/rpi-fan-control
sudo systemctl daemon-reload
sudo systemctl enable --now rpi-fan-control
```

Useful commands:

```bash
sudo systemctl status rpi-fan-control
journalctl -u rpi-fan-control -f
```

## Manual Validation Checklist

- Idle temp: fan stays off or at low stable duty.
- Heat-up test: fan ramps quickly as temp rises.
- Cooldown test: fan decays smoothly, no abrupt drops.
- Reboot test: service starts automatically and applies duty.
- Footprint check: validate low CPU/memory usage via `top` and `systemd-cgtop`.

## Safety Notes

- Start with conservative thresholds and verify thermals under your workload.
- Keep `target_temp_c` below your thermal throttle zone and validate under sustained load.
- Use `dry_run = true` while validating config logic without hardware attached.

## CI, Release, and Tagging

- CI workflow (`.github/workflows/ci.yml`) runs fmt, clippy, tests, and benchmark smoke.
- Release workflow (`.github/workflows/release.yml`) publishes ARM tarballs for tags matching `v*.*.*`.
- Recommended convention: semantic tags like `v0.1.0`.
- Release tags must match `Cargo.toml` version (e.g. `version = "0.1.2"` -> tag `v0.1.2`).
- Release notes template: `.github/release_template.md`.

To bump a release, create and push a new semantic tag:

```bash
git tag v0.1.1
git push origin v0.1.1
```

Or use the helper command to update `Cargo.toml`, commit, and create a matching tag:

```bash
make release-tag VERSION=0.1.1
git push && git push --tags
```

Pick the release asset that matches your Pi OS architecture:

- `*aarch64-unknown-linux-gnu.tar.gz` for 64-bit OS (`uname -m` shows `aarch64`).
- `*armv7-unknown-linux-gnueabihf.tar.gz` for 32-bit OS (`uname -m` shows `armv7l` or similar).

## License

This project is licensed under the MIT License. See `LICENSE`.

## LLM-Generated Disclaimer

Parts of this repository may include AI/LLM-generated content. Always review
and validate generated code/configuration before using it in production or
safety-critical environments.
