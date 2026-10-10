#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
    cat <<'HELP'
Usage: ./build-ubuntu.sh

Builds a single-file portable launcher for Ubuntu 22.04+ x86-64 desktops.
Requires Docker with BuildKit and internet access on the build machine.
Recipients need no package installation, FUSE, Docker, Rust, or Cargo.
Output: dist/hearing-test-ubuntu-x86_64.run and its SHA-256 checksum.
HELP
    exit 0
fi

command -v docker >/dev/null 2>&1 || {
    printf 'Error: Docker is required on the build machine.\n' >&2
    exit 1
}
docker info >/dev/null

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
package_name=hearing-test-ubuntu-x86_64.run
dist_dir="$project_dir/dist"
mkdir -p -- "$dist_dir"

DOCKER_BUILDKIT=1 docker build \
    --platform linux/amd64 \
    --file "$project_dir/Dockerfile.ubuntu" \
    --target portable \
    --output "type=local,dest=$dist_dir" \
    "$project_dir"

chmod +x "$dist_dir/$package_name"
(
    cd -- "$dist_dir"
    sha256sum "$package_name" > "$package_name.sha256"
)
printf '\nPortable launcher: %s\n' "$dist_dir/$package_name"
