# Hearing Tracker

Hearing Tracker is an offline Linux desktop application for personal, repeatable hearing checks. It plays quiet pulsed tones in one ear at a time, estimates the lowest digital level you respond to at eight frequencies, stores the result locally, and creates an audiogram-style PDF.

It is deliberately **not** a medical device or diagnostic audiometer. AirPods and normal computer audio are not calibrated for clinical `dB HL`, and their fit, processing, volume, and the room all affect results. The app therefore reports an internal relative scale (`dB-R`) and only compares tests made with the same recorded setup.

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
