# Hearing Tracker

Hearing Tracker is an offline Linux desktop application for personal, repeatable hearing checks. It plays quiet pulsed tones in one ear at a time, estimates the lowest digital level you respond to at eight frequencies, stores the result locally, and creates an audiogram-style PDF.

It is deliberately **not** a medical device or diagnostic audiometer. AirPods and normal computer audio are not calibrated for clinical `dB HL`, and their fit, processing, volume, and the room all affect results. The app therefore reports an internal relative scale (`dB-R`) and only compares tests made with the same recorded setup.

## Build and run on Ubuntu

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

## Application language

Use the **Language / Sprache** dropdown in the top-right header to choose
**English** or **Deutsch**. The interface changes immediately, and the selection
is remembered on the next launch. English is the default.

PDF reports use the language selected when you export them, including reports
for older tests. Changing the language preserves setup selections and test
progress and does not change recorded results or baseline comparisons. All
translations are bundled with the app and work offline. System file-dialog
controls follow your Linux desktop language.

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
