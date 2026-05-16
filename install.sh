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

resolve_release_tag() {
  local version="$1"
  if [[ "${version}" != "latest" ]]; then
    printf '%s\n' "${version}"
    return 0
  fi

  local json tag
  if ! json="$(curl -fsSL -H "Accept: application/vnd.github+json" "https://api.github.com/repos/${REPO}/releases/latest")"; then
    echo "Failed to query latest release metadata from GitHub" >&2
    return 1
  fi
  tag="$(printf '%s\n' "${json}" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1)"
  if [[ -z "${tag}" ]]; then
    echo "Could not determine latest release tag" >&2
    return 1
  fi
  printf '%s\n' "${tag}"
}

TMP_DIR="$(mktemp -d)"
cleanup() { rm -rf "${TMP_DIR}"; }
trap cleanup EXIT

TAG="$(resolve_release_tag "${VERSION}")"
BASE_URL="https://github.com/${REPO}/releases/download/${TAG}"
CANDIDATES=(
  "${BASE_URL}/rpi-fan-control-${TARGET}.tar.gz"
  "${BASE_URL}/rpi-fan-control-${TAG}-${TARGET}.tar.gz"
)

URL=""
for candidate in "${CANDIDATES[@]}"; do
  echo "Trying ${candidate}"
  if curl -fL "${candidate}" -o "${TMP_DIR}/release.tar.gz"; then
    URL="${candidate}"
    break
  fi
done

if [[ -z "${URL}" ]]; then
  echo "Could not download a release asset for ${TARGET} from tag ${TAG}" >&2
  exit 1
fi

echo "Downloaded ${URL}"
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
