#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    cat <<'HELP'
Usage: ./scripts/build-ubuntu.sh

Builds an executable for standard Ubuntu 22.04+ x86-64 Desktop installations.
Requires Docker with BuildKit and internet access on the build machine.
Recipients use Ubuntu Desktop's existing audio/graphics runtime libraries.
They do not need Docker, Rust, Cargo, or development packages.
Output: dist/hearing-test and its SHA-256 checksum.
HELP
    exit 0
fi

command -v docker >/dev/null 2>&1 || {
    printf 'Error: Docker is required on the build machine.\n' >&2
    exit 1
}
docker info >/dev/null

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
binary_name=hearing-test
dist_dir="$project_dir/dist"
mkdir -p -- "$dist_dir"

DOCKER_BUILDKIT=1 docker build \
    --platform linux/amd64 \
    --file "$project_dir/packaging/Dockerfile.ubuntu" \
    --target binary \
    --output "type=local,dest=$dist_dir" \
    "$project_dir"

chmod +x "$dist_dir/$binary_name"
(
    cd -- "$dist_dir"
    sha256sum "$binary_name" > "$binary_name.sha256"
)
printf '\nUbuntu executable: %s\n' "$dist_dir/$binary_name"
