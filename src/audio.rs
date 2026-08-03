use crate::domain::{CEILING_DBFS, Ear};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, Stream, StreamConfig};
use crossbeam_channel::{Receiver, Sender, unbounded};
use std::f32::consts::TAU;
use std::process::Command;
use thiserror::Error;

const TONE_SECONDS: f32 = 1.0;
const BURST_SECONDS: f32 = 0.250;
const BURST_PERIOD_SECONDS: f32 = 0.375;
const RAMP_SECONDS: f32 = 0.020;

#[derive(Debug, Clone)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub backend: String,
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, Copy)]
pub struct ToneRequest {
    pub token: u64,
    pub ear: Ear,
    pub frequency_hz: u32,
    pub dbfs: Option<f32>,
}

#[derive(Debug, Clone)]
pub enum AudioEvent {
    Started(u64),
    Finished(u64),
    Stopped,
    Error(String),
}

#[derive(Debug)]
enum AudioCommand {
    Play(ToneRequest),
    Stop,
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no output devices are available")]
    NoDevices,
    #[error("the selected output device is no longer available")]
    DeviceMissing,
    #[error("the selected output must provide at least two channels")]
    NotStereo,
    #[error("unsupported sample format: {0}")]
    UnsupportedFormat(SampleFormat),
    #[error("audio system error: {0}")]
    Cpal(#[from] cpal::Error),
}

pub struct AudioEngine {
    _stream: Stream,
    commands: Sender<AudioCommand>,
    events: Receiver<AudioEvent>,
    pub info: AudioDeviceInfo,
}

impl AudioEngine {
    pub fn devices() -> Result<Vec<AudioDeviceInfo>, AudioError> {
        let host = cpal::default_host();
        let backend = host.id().name().to_owned();
        let mut result = Vec::new();
        for device in host.output_devices()? {
            let Ok(config) = device.default_output_config() else {
                continue;
            };
            let Ok(id) = device.id() else {
                continue;
            };
            let name = device
                .description()
                .map(|description| description.name().to_owned())
                .unwrap_or_else(|_| id.to_string());
            result.push(AudioDeviceInfo {
                id: id.to_string(),
                name,
                backend: backend.clone(),
                sample_rate: config.sample_rate(),
                channels: config.channels(),
            });
        }
        if result.is_empty() {
            Err(AudioError::NoDevices)
        } else {
            Ok(result)
        }
    }

    pub fn open(device_id: &str) -> Result<Self, AudioError> {
        let host = cpal::default_host();
        let device = host
            .output_devices()?
            .find(|device| device.id().is_ok_and(|id| id.to_string() == device_id))
            .ok_or(AudioError::DeviceMissing)?;
        let supported = device.default_output_config()?;
        if supported.channels() < 2 {
            return Err(AudioError::NotStereo);
        }
        let id = device.id()?.to_string();
        let name = device
            .description()
            .map(|description| description.name().to_owned())
            .unwrap_or_else(|_| id.clone());
        let info = AudioDeviceInfo {
            id,
            name,
            backend: host.id().name().to_owned(),
            sample_rate: supported.sample_rate(),
            channels: supported.channels(),
        };
        let config: StreamConfig = supported.into();
        let (command_tx, command_rx) = unbounded();
        let (event_tx, event_rx) = unbounded();
        let stream = match supported.sample_format() {
            SampleFormat::I8 => build_stream::<i8>(&device, config, command_rx, event_tx)?,
            SampleFormat::I16 => build_stream::<i16>(&device, config, command_rx, event_tx)?,
            SampleFormat::I32 => build_stream::<i32>(&device, config, command_rx, event_tx)?,
            SampleFormat::I64 => build_stream::<i64>(&device, config, command_rx, event_tx)?,
            SampleFormat::U8 => build_stream::<u8>(&device, config, command_rx, event_tx)?,
            SampleFormat::U16 => build_stream::<u16>(&device, config, command_rx, event_tx)?,
            SampleFormat::U32 => build_stream::<u32>(&device, config, command_rx, event_tx)?,
            SampleFormat::U64 => build_stream::<u64>(&device, config, command_rx, event_tx)?,
            SampleFormat::F32 => build_stream::<f32>(&device, config, command_rx, event_tx)?,
            SampleFormat::F64 => build_stream::<f64>(&device, config, command_rx, event_tx)?,
            format => return Err(AudioError::UnsupportedFormat(format)),
        };
        stream.play()?;
        Ok(Self {
            _stream: stream,
            commands: command_tx,
            events: event_rx,
            info,
        })
    }

    pub fn play(&self, request: ToneRequest) -> Result<(), AudioError> {
        self.commands
            .send(AudioCommand::Play(request))
            .map_err(|_| AudioError::DeviceMissing)
    }

    pub fn stop(&self) {
        let _ = self.commands.send(AudioCommand::Stop);
    }

    pub fn try_event(&self) -> Option<AudioEvent> {
        self.events.try_recv().ok()
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug)]
struct ActiveTone {
    request: ToneRequest,
    frame: u64,
    phase: f32,
}

fn build_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    commands: Receiver<AudioCommand>,
    events: Sender<AudioEvent>,
) -> Result<Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let sample_rate = config.sample_rate as f32;
    let error_events = events.clone();
    let mut active: Option<ActiveTone> = None;
    device.build_output_stream(
        config,
        move |output: &mut [T], _| {
            while let Ok(command) = commands.try_recv() {
                match command {
                    AudioCommand::Play(request) => {
                        active = Some(ActiveTone {
                            request,
                            frame: 0,
                            phase: 0.0,
                        });
                        let _ = events.send(AudioEvent::Started(request.token));
                    }
                    AudioCommand::Stop => {
                        active = None;
                        let _ = events.send(AudioEvent::Stopped);
                    }
                }
            }

            for frame in output.chunks_mut(channels) {
                frame.fill(T::from_sample(0.0));
                let Some(tone) = active.as_mut() else {
                    continue;
                };
                let elapsed = tone.frame as f32 / sample_rate;
                if elapsed >= TONE_SECONDS {
                    let token = tone.request.token;
                    active = None;
                    let _ = events.send(AudioEvent::Finished(token));
                    continue;
                }
                let value = tone
                    .request
                    .dbfs
                    .map(|dbfs| {
                        let amplitude = db_to_amplitude(dbfs.min(CEILING_DBFS));
                        let position = elapsed % BURST_PERIOD_SECONDS;
                        let envelope = burst_envelope(position);
                        let sample = tone.phase.sin() * amplitude * envelope;
                        tone.phase = (tone.phase
                            + TAU * tone.request.frequency_hz as f32 / sample_rate)
                            % TAU;
                        sample
                    })
                    .unwrap_or(0.0);
                if let Some(sample) = frame.get_mut(tone.request.ear.channel()) {
                    *sample = T::from_sample(value);
                }
                tone.frame += 1;
            }
        },
        move |error| {
            let _ = error_events.send(AudioEvent::Error(error.to_string()));
        },
        None,
    )
}

pub fn db_to_amplitude(dbfs: f32) -> f32 {
    10.0_f32.powf(dbfs / 20.0)
}

fn burst_envelope(position_seconds: f32) -> f32 {
    if position_seconds >= BURST_SECONDS {
        return 0.0;
    }
    let ramp_in = (position_seconds / RAMP_SECONDS).clamp(0.0, 1.0);
    let ramp_out = ((BURST_SECONDS - position_seconds) / RAMP_SECONDS).clamp(0.0, 1.0);
    let amount = ramp_in.min(ramp_out);
    0.5 - 0.5 * (std::f32::consts::PI * amount).cos()
}

pub fn system_output_volume_percent() -> Option<f32> {
    let wpctl = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| {
            text.split_whitespace()
                .find_map(|part| part.parse::<f32>().ok())
                .map(|volume| volume * 100.0)
        });
    wpctl.or_else(pactl_volume_percent)
}

fn pactl_volume_percent() -> Option<f32> {
    let output = Command::new("pactl")
        .args(["get-sink-volume", "@DEFAULT_SINK@"])
        .output()
        .ok()?
        .stdout;
    String::from_utf8(output)
        .ok()?
        .split_whitespace()
        .find_map(|part| part.strip_suffix('%')?.parse().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digital_level_conversion_is_bounded_and_predictable() {
        assert!((db_to_amplitude(-20.0) - 0.1).abs() < 0.000_01);
        assert!(db_to_amplitude(CEILING_DBFS) < 0.032);
    }

    #[test]
    fn envelope_fades_and_is_silent_between_bursts() {
        assert_eq!(burst_envelope(0.0), 0.0);
        assert!(burst_envelope(0.010) > 0.0);
        assert!((burst_envelope(0.100) - 1.0).abs() < 0.000_01);
        assert_eq!(burst_envelope(0.250), 0.0);
        assert_eq!(burst_envelope(0.300), 0.0);
    }
}
