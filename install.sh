#!/usr/bin/env bash
set -euo pipefail

REPO="zazabe/rpi-fan-controller"
VERSION="${1:-latest}"

if [[ "${EUID}" -ne 0 ]]; then
  echo "Please run as root (example: curl ... | sudo bash)" >&2
  exit 1
fi

case "$(uname -m)" in
  aarch64|arm64)
    TARGET="aarch64-unknown-linux-gnu"
    ;;
  armv7l|armv6l)
    TARGET="armv7-unknown-linux-gnueabihf"
    ;;
  *)
    echo "Unsupported architecture: $(uname -m)" >&2
    echo "Supported: aarch64/arm64, armv7l/armv6l" >&2
    exit 1
    ;;
esac

ASSET="rpi-fan-control-${TARGET}.tar.gz"
if [[ "${VERSION}" == "latest" ]]; then
  URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"
else
  URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET}"
fi

TMP_DIR="$(mktemp -d)"
cleanup() { rm -rf "${TMP_DIR}"; }
trap cleanup EXIT

echo "Downloading ${URL}"
curl -fL "${URL}" -o "${TMP_DIR}/release.tar.gz"
tar -xzf "${TMP_DIR}/release.tar.gz" -C "${TMP_DIR}"

PKG_DIR="${TMP_DIR}/rpi-fan-control-${TARGET}"
if [[ ! -d "${PKG_DIR}" ]]; then
  echo "Unexpected archive structure: ${PKG_DIR} missing" >&2
  exit 1
fi

install -D -m 0755 "${PKG_DIR}/rpi-fan-control" /usr/local/bin/rpi-fan-control
install -D -m 0644 "${PKG_DIR}/rpi-fan-control.service" /etc/systemd/system/rpi-fan-control.service
if [[ ! -f /etc/rpi-fan-control/config.toml ]]; then
  install -D -m 0644 "${PKG_DIR}/config.toml.example" /etc/rpi-fan-control/config.toml
else
  echo "Keeping existing /etc/rpi-fan-control/config.toml"
fi

if [[ ! -f /etc/default/rpi-fan-control ]]; then
  install -D -m 0644 /dev/stdin /etc/default/rpi-fan-control <<'EOF'
RUST_LOG=info
EOF
fi

systemctl daemon-reload
systemctl enable --now rpi-fan-control
echo "Installed rpi-fan-control (${VERSION}) for ${TARGET}"
