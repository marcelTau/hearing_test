# Hearing Tracker

Hearing Tracker is an offline Linux desktop application for personal, repeatable hearing checks. It plays quiet pulsed tones in one ear at a time, estimates the lowest digital level you respond to at eight frequencies, stores the result locally, and creates an audiogram-style PDF.

It is deliberately **not** a medical device or diagnostic audiometer. AirPods and normal computer audio are not calibrated for clinical `dB HL`, and their fit, processing, volume, and the room all affect results. The app therefore reports an internal relative scale (`dB-R`) and only compares tests made with the same recorded setup.

## Prebuilt Ubuntu executable

To create a release for other people, run this on a machine with Docker and
BuildKit (Rust and Cargo are installed inside the build container):

```sh
./scripts/build-ubuntu.sh
```

The script builds the release executable inside an Ubuntu 22.04 container and
exports it to `dist/hearing-test`, alongside `dist/hearing-test.sha256`.
Share those files. Recipients can launch the executable from a terminal:

```sh
chmod +x hearing-test  # Needed once if the download loses executable permission.
./hearing-test
```

The executable targets standard Ubuntu 22.04 and newer Desktop installations
on Intel/AMD 64-bit machines. Docker, Rust, Cargo, compilers, and development
headers are required only on the build machine. The executable uses Ubuntu
Desktop's existing ALSA and graphics runtime libraries; these are not bundled.
The target is a normal Desktop installation, not a bare Ubuntu Server or Docker
image with missing desktop libraries. ARM machines need a separate build.
Results are stored in the user's data directory, independently of the executable.

## Build from source and run on Ubuntu

On a normal Ubuntu Desktop installation, open a terminal and run:

```sh
./scripts/run-ubuntu.sh
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

## Your listening space

The interface uses a light theme with soft blue accents and large controls.
The overview shows your saved checks, with the most recent check marked at the
top. Select **View** to open its chart, compare a matching baseline, or export
a completed check as a PDF. Expand **Detailed measurements** for individual
frequency values. Report names and local storage details are under
**Report identity and local data** on the overview.

**Start a listening check** guides you through headphones, a quiet-space
checklist, a sound-range check, and a final review. Each step explains what is
needed before you can continue. Going back keeps your selections; changing
the output device or volume requires a fresh sound check for both ears.

The sound check plays a comfortable 1000 Hz reference in each ear, then the
quietest available tones at 1000 and 4000 Hz in both ears. Answer after each
tone finishes. If the quietest tones are still audible, lower the shared system
volume and repeat the sound check. Never adjust each ear separately. If no
shared setting works for both ears, you can explicitly continue with a limited
range. An inaudible reference can be recorded without raising the volume.

During a check, click **I hear it** or press Space only when you hear a tone.
You can pause at any time; responses are disabled while paused. Decorative
sound graphics stay static and do not signal tone playback.

If a tone remains audible at the minimum during the full check, the test pauses.
Choose **Restart with lower volume** to save the incomplete check as abandoned
and repeat setup, or continue at the original volume. Completed results also
provide a restart button while keeping the saved check. A changed volume uses
a different setup baseline; results at different volumes are not mixed.

**At/below minimum** means the actual threshold was not found. Up arrows mark
these limits on charts; PDFs use `<=0` in the values table. They are excluded
from exact ear differences and numerical changes over time. Two ears at the
minimum show **Undetermined**, not a zero difference. This interpretation also
applies to previously saved results without rewriting their raw measurements.

## Before each test

- Use the same AirPods and a quiet room.
- Keep Noise Cancellation on.
- Turn off Adaptive/Transparency mode, Spatial Audio, personalized volume, Conversation Awareness, Headphone Accommodations, hearing assistance, and other personalized processing.
- Keep the same Linux output volume and AirPods fit.
- Complete the sound-range check for both ears. Keep the reference comfortable, never loud, and record it as not heard if inaudible. Lower the shared volume and repeat setup if the quietest tones remain audible. The app never generates a tone above `-30 dBFS`.
- Press Space or click **I hear it** whenever you hear a pulsed tone, even if it is very faint. Some intervals are intentionally silent.

The full test covers 250, 500, 1000, 2000, 3000, 4000, 6000, and 8000 Hz in each ear and usually takes 12–18 minutes.

## Safety

This tool does not identify the type or cause of hearing loss and cannot replace examination or calibrated testing by a hearing professional.
