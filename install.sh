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

resolve_download_url() {
  local version="$1"
  local api_url

  if [[ "${version}" == "latest" ]]; then
    api_url="https://api.github.com/repos/${REPO}/releases/latest"
  else
    api_url="https://api.github.com/repos/${REPO}/releases/tags/${version}"
  fi

  local json
  if ! json="$(curl -fsSL -H "Accept: application/vnd.github+json" "${api_url}")"; then
    echo "Failed to query GitHub release metadata: ${api_url}" >&2
    return 1
  fi

  # Prefer exact current asset name, but also accept historical names that end
  # with "-${TARGET}.tar.gz" (for example with version in the filename).
  local url
  url="$(
    printf '%s\n' "${json}" \
      | sed -n 's/.*"browser_download_url":[[:space:]]*"\([^"]*\)".*/\1/p' \
      | awk -v exact="${ASSET}" -v suffix="-${TARGET}.tar.gz" '
          {
            if ($0 ~ ("/" exact "$")) exact_match = $0
            if ($0 ~ (suffix "$")) suffix_match = $0
          }
          END {
            if (exact_match != "") print exact_match
            else if (suffix_match != "") print suffix_match
          }
        '
  )"

  if [[ -z "${url}" ]]; then
    echo "No release asset found for ${TARGET} in ${version}" >&2
    return 1
  fi

  printf '%s\n' "${url}"
}

TMP_DIR="$(mktemp -d)"
cleanup() { rm -rf "${TMP_DIR}"; }
trap cleanup EXIT

URL="$(resolve_download_url "${VERSION}")"
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
