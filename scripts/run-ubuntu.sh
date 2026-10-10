#!/usr/bin/env bash
# Bootstrap and launch Hearing Tracker from an Ubuntu desktop session.
set -euo pipefail

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    cat <<'HELP'
Usage: ./scripts/run-ubuntu.sh

Installs Ubuntu build prerequisites and stable Rust (including Cargo),
then builds and launches Hearing Tracker. Requires internet access and sudo
for package installation. Run as your normal user in a desktop session.
HELP
    exit 0
fi

die() {
    printf 'Error: %s\n' "$*" >&2
    exit 1
}

trap 'printf "Error: setup or launch failed at line %s. See the output above.\n" "$LINENO" >&2' ERR

[[ -r /etc/os-release ]] || die 'Cannot identify this operating system.'
# shellcheck disable=SC1091
source /etc/os-release
[[ "${ID:-}" == ubuntu ]] || die 'This script supports Ubuntu. See README.md for other systems.'
(( EUID != 0 )) || die 'Run this script as your normal user, without sudo. It requests sudo when needed.'
[[ -n "${DISPLAY:-}" || -n "${WAYLAND_DISPLAY:-}" ]] ||
    die 'Run this script from a terminal in your Ubuntu desktop session (X11 or Wayland).'

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
[[ -f "$project_dir/Cargo.toml" && -f "$project_dir/Cargo.lock" ]] ||
    die 'Keep this script in the repository scripts/ directory, with Cargo.toml and Cargo.lock in its parent directory.'

# Ubuntu Desktop supplies the audio/graphics runtime libraries. This project's
# Wayland support uses dlopen, so Wayland development headers aren't needed.
# Install only build tools, ALSA headers, and the Rust download prerequisites.
# SQLite is compiled by rusqlite's bundled feature; no system SQLite is needed.
packages=(
    build-essential
    pkg-config
    curl
    ca-certificates
    libasound2-dev
)

missing_packages=()
for package in "${packages[@]}"; do
    if [[ "$(dpkg-query -W -f='${Status}' "$package" 2>/dev/null || true)" != 'install ok installed' ]]; then
        missing_packages+=("$package")
    fi
done

if (( ${#missing_packages[@]} )); then
    command -v sudo >/dev/null 2>&1 || die 'sudo is required to install missing Ubuntu packages.'
    printf 'Installing missing build prerequisites (sudo may ask for your password)...\n'
    sudo apt-get update
    sudo apt-get install -y --no-install-recommends "${missing_packages[@]}"
fi

# Use rustup even if Ubuntu's older rustc/cargo packages are installed. Respect
# a custom CARGO_HOME, and make a previous rustup install visible immediately.
export PATH="${CARGO_HOME:-$HOME/.cargo}/bin:$PATH"
if ! command -v rustup >/dev/null 2>&1; then
    printf 'Installing Rust and Cargo using rustup...\n'
    curl --proto '=https' --tlsv1.2 --fail --show-error --silent https://sh.rustup.rs |
        sh -s -- -y --profile minimal --default-toolchain stable --no-modify-path
fi

# Select stable explicitly without changing an existing default toolchain.
printf 'Ensuring the current stable Rust toolchain is installed...\n'
rustup toolchain install stable --profile minimal

cd -- "$project_dir"
printf 'Building and launching Hearing Tracker (the first build may take several minutes)...\n'
exec rustup run stable cargo run --locked --release --bin hearing_test -- "$@"
