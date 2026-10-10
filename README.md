# Hearing Tracker

Hearing Tracker is an offline Linux desktop application for personal, repeatable hearing checks. It plays quiet pulsed tones in one ear at a time, estimates the lowest digital level you respond to at eight frequencies, stores the result locally, and creates an audiogram-style PDF.

It is deliberately **not** a medical device or diagnostic audiometer. AirPods and normal computer audio are not calibrated for clinical `dB HL`, and their fit, processing, volume, and the room all affect results. The app therefore reports an internal relative scale (`dB-R`) and only compares tests made with the same recorded setup.

## Portable Ubuntu download

To create a release for other people, run this on a machine with Docker and
BuildKit (Rust and Cargo are installed inside the build container):

```sh
./build-ubuntu.sh
```

Share `dist/hearing-test-ubuntu-x86_64.run` and its `.sha256` checksum file.
Recipients can launch it with one command from a terminal, even if the download
doesn't preserve executable permission:

```sh
sh hearing-test-ubuntu-x86_64.run
```

The single file bundles the app, ALSA and its audio plugins/configuration,
X11/Wayland client libraries, a software Vulkan renderer, and their dependencies.
It automatically extracts into a temporary directory, launches the app, and
cleans up when the app closes. Nothing is installed system-wide; it works
offline and needs no FUSE, Rust, Cargo, Docker, or extra packages.

This is a self-extracting bundle rather than a fully static ELF binary. It
targets Ubuntu 22.04 and newer Desktop installations on Intel/AMD 64-bit
machines, using Ubuntu's core C runtime and the running desktop/audio services.
The bundled software renderer avoids requiring a particular GPU or additional
graphics drivers. Results are stored separately in the user's data directory.
ARM machines need a separate build.

## Build from source and run on Ubuntu

On a normal Ubuntu Desktop installation, open a terminal and run:

```sh
./run-ubuntu.sh
```

The script installs missing build tools and ALSA development headers through
APT, installs Rust and Cargo through [rustup](https://rust-lang.github.io/rustup/installation/)
if needed, ensures the current stable Rust toolchain is available, then builds
and launches the app in release mode using `Cargo.lock`. No preinstalled Rust,
Cargo, or development libraries are required. Internet access and a user with
`sudo` access are needed for setup; run the script as your normal user, without
`sudo`. The first build can take several minutes.

You can run the script again after changing the source. It works from any
directory when invoked by its path and reuses installed packages and previous
build output. It relies on the audio/graphics runtime libraries supplied by
Ubuntu Desktop. Its APT package list is limited to `build-essential`,
`pkg-config`, `curl`, `ca-certificates`, and
[`libasound2-dev`](https://github.com/RustAudio/cpal#linux-build-dependencies),
and it only installs packages that are missing.
After setup, you can also launch the built app directly, without setup checks or
internet access:

```sh
./target/release/hearing_test
```

## Build and run on Fedora

Install Rust and CPAL's required ALSA development headers, then run the app:

```sh
sudo dnf install alsa-lib-devel
cargo run --release
```

For a reusable binary:

```sh
cargo build --release
install -Dm755 target/release/hearing_test ~/.local/bin/hearing-test
```

The program uses PipeWire/PulseAudio through CPAL. `wpctl` (preferred) or `pactl` is used read-only to record the default output volume.

## Before each test

- Use the same AirPods and a quiet room.
- Keep Noise Cancellation on.
- Turn off Adaptive/Transparency mode, Spatial Audio, personalized volume, Conversation Awareness, Headphone Accommodations, hearing assistance, and other personalized processing.
- Keep the same Linux output volume and AirPods fit.
- Use the left/right reference buttons. The reference must be clearly audible but comfortable, never loud. The app never generates a tone above `-30 dBFS`.
- Press Space or click **I hear it** whenever you hear a pulsed tone, even if it is very faint. Some intervals are intentionally silent.

The full test covers 250, 500, 1000, 2000, 3000, 4000, 6000, and 8000 Hz in each ear and usually takes 12–18 minutes.

## Safety

This tool does not identify the type or cause of hearing loss and cannot replace examination or calibrated testing by a hearing professional.
