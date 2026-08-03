use crate::audio::{
    AudioDeviceInfo, AudioEngine, AudioEvent, ToneRequest, system_output_volume_percent,
};
use crate::domain::{
    AudioSetup, CEILING_DBFS, Ear, FREQUENCIES_HZ, PROTOCOL_VERSION, SessionStatus, TestSession,
    ThresholdOutcome,
};
use crate::report;
use crate::storage::SessionRepository;
use crate::test_engine::{Presentation, TestEngine};
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};
use rand::Rng;
use std::time::{Duration, Instant};
use uuid::Uuid;

const URGENT_WARNING: &str = "Sudden hearing loss, hearing worsening over days or weeks, or hearing loss with earache or discharge needs urgent medical assessment. Do not wait for a home test.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Page {
    Dashboard,
    Setup,
    Test,
    Results(Uuid),
}

pub struct HearingApp {
    repository: Option<SessionRepository>,
    sessions: Vec<TestSession>,
    page: Page,
    setup: SetupState,
    running: Option<TestRun>,
    display_name: String,
    message: Option<Message>,
    delete_confirmation: Option<Uuid>,
}

#[derive(Debug, Clone)]
struct Message {
    text: String,
    error: bool,
}

struct SetupState {
    devices: Vec<AudioDeviceInfo>,
    selected_device: usize,
    audio: Option<AudioEngine>,
    quiet_room: bool,
    noise_cancellation: bool,
    processing_off: bool,
    comfortable: bool,
    first_ear: Ear,
    volume: Option<f32>,
    next_token: u64,
}

impl Default for SetupState {
    fn default() -> Self {
        Self {
            devices: Vec::new(),
            selected_device: 0,
            audio: None,
            quiet_room: false,
            noise_cancellation: false,
            processing_off: false,
            comfortable: false,
            first_ear: Ear::Right,
            volume: None,
            next_token: 1,
        }
    }
}

struct TestRun {
    session: TestSession,
    audio: AudioEngine,
    engine: Option<TestEngine>,
    phase: RunPhase,
    pending: Option<Presentation>,
    next_token: u64,
    real_since_catch: u32,
    catch_after: u32,
    warning: Option<String>,
}

enum RunPhase {
    Waiting(Instant),
    Playing { token: u64, started: Instant },
    ResponseWindow { started: Instant, deadline: Instant },
    Paused,
}

enum TestAction {
    None,
    SaveProgress,
    Completed(TestSession),
    Abandoned(TestSession),
}

impl HearingApp {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        context.egui_ctx.style_mut_of(egui::Theme::Light, |style| {
            style.spacing.button_padding = Vec2::new(14.0, 8.0);
        });
        let (repository, sessions, display_name, message) = match SessionRepository::open_default()
        {
            Ok(repository) => {
                let sessions = repository.list().unwrap_or_default();
                let display_name = repository
                    .preference("display_name")
                    .ok()
                    .flatten()
                    .unwrap_or_default();
                (Some(repository), sessions, display_name, None)
            }
            Err(error) => (
                None,
                Vec::new(),
                String::new(),
                Some(Message {
                    text: error.to_string(),
                    error: true,
                }),
            ),
        };
        Self {
            repository,
            sessions,
            page: Page::Dashboard,
            setup: SetupState::default(),
            running: None,
            display_name,
            message,
            delete_confirmation: None,
        }
    }

    fn refresh_sessions(&mut self) {
        if let Some(repository) = &self.repository {
            match repository.list() {
                Ok(sessions) => self.sessions = sessions,
                Err(error) => self.error(error.to_string()),
            }
        }
    }

    fn error(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: true,
        });
    }

    fn info(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            error: false,
        });
    }

    fn enter_setup(&mut self) {
        self.setup = SetupState::default();
        self.load_devices();
        self.page = Page::Setup;
    }

    fn load_devices(&mut self) {
        match AudioEngine::devices() {
            Ok(devices) => {
                self.setup.devices = devices;
                self.setup.selected_device = self
                    .setup
                    .devices
                    .iter()
                    .position(|device| {
                        device.name.to_ascii_lowercase().contains("airpod")
                            || device.id.to_ascii_lowercase().contains("airpod")
                    })
                    .unwrap_or(0);
                self.setup.volume = system_output_volume_percent();
                self.setup.audio = None;
            }
            Err(error) => self.error(error.to_string()),
        }
    }

    fn prepare_audio(&mut self) -> bool {
        if self.setup.audio.is_some() {
            return true;
        }
        let Some(device) = self.setup.devices.get(self.setup.selected_device) else {
            self.error("Select a stereo output device first");
            return false;
        };
        match AudioEngine::open(&device.id) {
            Ok(audio) => {
                self.setup.audio = Some(audio);
                true
            }
            Err(error) => {
                self.error(error.to_string());
                false
            }
        }
    }

    fn play_setup_tone(&mut self, ear: Ear) {
        if !self.prepare_audio() {
            return;
        }
        let token = self.setup.next_token;
        self.setup.next_token += 1;
        if let Some(audio) = &self.setup.audio
            && let Err(error) = audio.play(ToneRequest {
                token,
                ear,
                frequency_hz: 1_000,
                dbfs: Some(CEILING_DBFS),
            })
        {
            self.error(error.to_string());
        }
    }

    fn start_test(&mut self) {
        if !self.prepare_audio() {
            return;
        }
        self.setup.volume = system_output_volume_percent();
        let audio = self.setup.audio.take().expect("audio was prepared");
        let setup = AudioSetup {
            device_id: audio.info.id.clone(),
            device_name: audio.info.name.clone(),
            backend: audio.info.backend.clone(),
            sample_rate: audio.info.sample_rate,
            channels: audio.info.channels,
            sink_volume_percent: self.setup.volume,
            listening_mode: "Noise cancellation".to_owned(),
            ceiling_dbfs: CEILING_DBFS,
            protocol_version: PROTOCOL_VERSION,
        };
        let session = TestSession::new(setup);
        if let Some(repository) = &self.repository
            && let Err(error) = repository.save(&session)
        {
            self.error(error.to_string());
            self.setup.audio = Some(audio);
            return;
        }
        self.running = Some(TestRun::new(session, audio, self.setup.first_ear));
        self.page = Page::Test;
    }

    fn baseline_for(&self, session: &TestSession) -> Option<TestSession> {
        self.repository
            .as_ref()?
            .baseline_for(&session.setup_fingerprint)
            .ok()?
            .filter(|baseline| baseline.id != session.id && session.is_comparable_with(baseline))
    }

    fn handle_test_action(&mut self, action: TestAction) {
        match action {
            TestAction::None => {}
            TestAction::SaveProgress => {
                if let (Some(repository), Some(run)) = (&self.repository, &self.running)
                    && let Err(error) = repository.save(&run.session)
                {
                    self.error(error.to_string());
                }
            }
            TestAction::Completed(session) => {
                let id = session.id;
                if let Some(repository) = &self.repository
                    && let Err(error) = repository.save(&session)
                {
                    self.error(error.to_string());
                }
                self.running = None;
                self.refresh_sessions();
                self.page = Page::Results(id);
            }
            TestAction::Abandoned(session) => {
                if let Some(repository) = &self.repository
                    && let Err(error) = repository.save(&session)
                {
                    self.error(error.to_string());
                }
                self.running = None;
                self.refresh_sessions();
                self.page = Page::Dashboard;
                self.info("The incomplete test was saved as abandoned and excluded from trends.");
            }
        }
    }
}

impl eframe::App for HearingApp {
    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let context = root_ui.ctx().clone();
        egui::Frame::central_panel(root_ui.style()).show(root_ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Hearing Tracker");
                ui.separator();
                ui.label(
                    RichText::new("Personal, uncalibrated monitoring").color(Color32::DARK_GRAY),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.page != Page::Dashboard
                        && ui.button("Home").clicked()
                        && self.running.is_none()
                    {
                        self.page = Page::Dashboard;
                    }
                });
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| match self.page {
                    Page::Dashboard => self.dashboard_ui(ui),
                    Page::Setup => self.setup_ui(ui),
                    Page::Test => {
                        let action = self
                            .running
                            .as_mut()
                            .map(|run| run.ui(ui, &context))
                            .unwrap_or(TestAction::None);
                        self.handle_test_action(action);
                    }
                    Page::Results(id) => self.results_ui(ui, id),
                });
            ui.separator();
            ui.label(
                RichText::new(URGENT_WARNING)
                    .small()
                    .color(Color32::from_rgb(145, 67, 20)),
            );
        });

        if let Some(message) = self.message.clone() {
            egui::Window::new(if message.error {
                "Problem"
            } else {
                "Hearing Tracker"
            })
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(&context, |ui| {
                ui.set_max_width(440.0);
                ui.label(message.text);
                ui.add_space(8.0);
                if ui.button("OK").clicked() {
                    self.message = None;
                }
            });
        }
    }
}

impl HearingApp {
    fn dashboard_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading("Track changes using the same AirPods setup");
                ui.label("This app compares repeatable digital thresholds between ears and dates. It cannot diagnose hearing loss or produce clinical dB HL values.");
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_sized([150.0, 42.0], egui::Button::new("Start a new test"))
                    .clicked()
                {
                    self.enter_setup();
                }
            });
        });
        ui.add_space(18.0);
        ui.separator();
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.heading("History");
            ui.label(format!("{} saved sessions", self.sessions.len()));
        });
        if self.sessions.is_empty() {
            ui.add_space(24.0);
            ui.label("No tests yet. Your first complete, high-confidence test becomes the baseline for this exact audio setup.");
        } else {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("history_grid")
                    .num_columns(5)
                    .striped(true)
                    .spacing([24.0, 10.0])
                    .show(ui, |ui| {
                        ui.strong("Date");
                        ui.strong("Status");
                        ui.strong("Quality");
                        ui.strong("Output");
                        ui.strong("");
                        ui.end_row();
                        let rows: Vec<_> = self
                            .sessions
                            .iter()
                            .map(|session| {
                                (
                                    session.id,
                                    date_label(session),
                                    format!("{:?}", session.status),
                                    session
                                        .quality
                                        .as_ref()
                                        .map(|q| q.label())
                                        .unwrap_or("--")
                                        .to_owned(),
                                    session.setup.device_name.clone(),
                                )
                            })
                            .collect();
                        for (id, date, status, quality, output) in rows {
                            ui.label(date);
                            ui.label(status);
                            ui.label(quality);
                            ui.label(output);
                            if ui.button("View").clicked() {
                                self.page = Page::Results(id);
                            }
                            ui.end_row();
                        }
                    });
            });
        }
        ui.add_space(16.0);
        ui.separator();
        ui.collapsing("Report identity and local data", |ui| {
            ui.horizontal(|ui| {
                ui.label("Optional name on PDF reports:");
                let response = ui.text_edit_singleline(&mut self.display_name);
                if response.lost_focus()
                    && let Some(repository) = &self.repository
                    && let Err(error) =
                        repository.set_preference("display_name", &self.display_name)
                {
                    self.error(error.to_string());
                }
            });
            if let Some(repository) = &self.repository {
                ui.small(format!("Local database: {}", repository.path().display()));
            }
            ui.small("Data stays on this computer and is not encrypted or uploaded.");
        });
    }

    fn setup_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(10.0);
        ui.heading("Prepare a repeatable, comfortable setup");
        ui.label("Use the same AirPods, Linux volume, fit, room, and listening mode for every comparable test.");
        ui.add_space(10.0);

        ui.group(|ui| {
            ui.strong("1. Output device");
            ui.horizontal(|ui| {
                let selected_name = self
                    .setup
                    .devices
                    .get(self.setup.selected_device)
                    .map(|device| device.name.as_str())
                    .unwrap_or("No output device");
                egui::ComboBox::from_id_salt("output_device")
                    .selected_text(selected_name)
                    .show_ui(ui, |ui| {
                        for (index, device) in self.setup.devices.iter().enumerate() {
                            if ui
                                .selectable_value(&mut self.setup.selected_device, index, &device.name)
                                .changed()
                            {
                                self.setup.audio = None;
                                self.setup.comfortable = false;
                            }
                        }
                    });
                if ui.button("Refresh").clicked() {
                    self.load_devices();
                }
            });
            if let Some(device) = self.setup.devices.get(self.setup.selected_device) {
                ui.small(format!("{} Hz, {} channels, {} backend", device.sample_rate, device.channels, device.backend));
            }
            ui.label(match self.setup.volume {
                Some(volume) => format!("Current default-output volume: {volume:.0}% (recorded for comparisons)"),
                None => "Linux output volume could not be read; this test can be viewed but will not qualify for trends.".to_owned(),
            });
        });
        ui.add_space(8.0);

        ui.group(|ui| {
            ui.strong("2. AirPods and room checklist");
            ui.checkbox(&mut self.setup.quiet_room, "I am in a quiet room and both AirPods fit securely");
            ui.checkbox(&mut self.setup.noise_cancellation, "Noise Cancellation is on (not Adaptive or Transparency)");
            ui.checkbox(&mut self.setup.processing_off, "Spatial Audio, personalized volume, conversation awareness, headphone accommodations, and hearing assistance are off");
        });
        ui.add_space(8.0);

        ui.group(|ui| {
            ui.strong("3. Check the gentle reference tone and channels");
            ui.label("The test never exceeds this -30 dBFS reference. It should be clearly audible but not loud. Do not turn the volume up if it is uncomfortable or inaudible.");
            ui.horizontal(|ui| {
                if ui.button("Play left").clicked() {
                    self.play_setup_tone(Ear::Left);
                }
                if ui.button("Play right").clicked() {
                    self.play_setup_tone(Ear::Right);
                }
            });
            ui.checkbox(&mut self.setup.comfortable, "I heard the correct side and the reference was comfortable, not loud");
        });
        ui.add_space(8.0);

        ui.group(|ui| {
            ui.strong("4. Ear order");
            ui.label(
                "If you know which ear is better, test it first. Otherwise keep Right selected.",
            );
            ui.horizontal(|ui| {
                ui.radio_value(
                    &mut self.setup.first_ear,
                    Ear::Right,
                    "Right first / unsure",
                );
                ui.radio_value(&mut self.setup.first_ear, Ear::Left, "Left first");
            });
        });
        ui.add_space(14.0);

        let ready = !self.setup.devices.is_empty()
            && self.setup.quiet_room
            && self.setup.noise_cancellation
            && self.setup.processing_off
            && self.setup.comfortable;
        if ui
            .add_enabled(
                ready,
                egui::Button::new(RichText::new("Begin test").size(17.0)),
            )
            .clicked()
        {
            self.start_test();
        }
        ui.label("Expected duration: about 12-18 minutes. Press Space or click only when you hear a tone, even if it is very faint.");
    }

    fn results_ui(&mut self, ui: &mut egui::Ui, id: Uuid) {
        let Some(session) = self
            .sessions
            .iter()
            .find(|session| session.id == id)
            .cloned()
        else {
            ui.label("This session is no longer available.");
            return;
        };
        let baseline = self.baseline_for(&session);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.heading(format!("Results - {}", date_label(&session)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if session.status == SessionStatus::Completed && ui.button("Export PDF").clicked() {
                    let suggested = format!("hearing-report-{}.pdf", session.started_at.date());
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("PDF report", &["pdf"])
                        .set_file_name(&suggested)
                        .save_file()
                    {
                        match report::save_pdf(
                            &path,
                            &session,
                            baseline.as_ref(),
                            Some(&self.display_name),
                        ) {
                            Ok(()) => self.info(format!("Report saved to {}", path.display())),
                            Err(error) => self.error(error.to_string()),
                        }
                    }
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Status: {:?}", session.status));
            ui.separator();
            ui.label(format!("Output: {}", session.setup.device_name));
            ui.separator();
            ui.label(format!(
                "Volume: {}",
                session
                    .setup
                    .sink_volume_percent
                    .map(|value| format!("{value:.0}%"))
                    .unwrap_or_else(|| "unavailable".into())
            ));
            if let Some(quality) = &session.quality {
                ui.separator();
                ui.label(
                    RichText::new(quality.label()).color(if quality.high_confidence {
                        Color32::DARK_GREEN
                    } else {
                        Color32::DARK_RED
                    }),
                );
            }
        });
        if let Some(quality) = &session.quality {
            for reason in &quality.reasons {
                ui.label(RichText::new(format!("• {reason}")).color(Color32::DARK_RED));
            }
        }
        ui.add_space(8.0);
        audiogram(ui, &session, baseline.as_ref());
        ui.add_space(8.0);
        threshold_table(ui, &session, baseline.as_ref());
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if session
                .quality
                .as_ref()
                .is_some_and(|quality| quality.high_confidence)
                && ui.button("Use as baseline for this setup").clicked()
                && let Some(repository) = &self.repository
            {
                match repository.set_baseline(&session) {
                    Ok(()) => self.info("Baseline updated for this exact setup."),
                    Err(error) => self.error(error.to_string()),
                }
            }
            if ui.button("Delete session…").clicked() {
                self.delete_confirmation = Some(id);
            }
        });

        if self.delete_confirmation == Some(id) {
            egui::Window::new("Delete this session?")
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ui.ctx(), |ui| {
                    ui.label("This permanently removes the local session and its raw trials.");
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.delete_confirmation = None;
                        }
                        if ui
                            .button(RichText::new("Delete").color(Color32::RED))
                            .clicked()
                        {
                            if let Some(repository) = &self.repository
                                && let Err(error) = repository.delete(id)
                            {
                                self.error(error.to_string());
                                return;
                            }
                            self.delete_confirmation = None;
                            self.refresh_sessions();
                            self.page = Page::Dashboard;
                        }
                    });
                });
        }
    }
}

impl TestRun {
    fn new(session: TestSession, audio: AudioEngine, first_ear: Ear) -> Self {
        Self {
            session,
            audio,
            engine: Some(TestEngine::new(first_ear)),
            phase: RunPhase::Waiting(Instant::now() + Duration::from_millis(1_500)),
            pending: None,
            next_token: 10_000,
            real_since_catch: 0,
            catch_after: 8,
            warning: None,
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, context: &egui::Context) -> TestAction {
        context.request_repaint_after(Duration::from_millis(25));
        self.poll_audio();
        let now = Instant::now();
        if let RunPhase::Waiting(deadline) = self.phase
            && now >= deadline
        {
            self.schedule_presentation();
        }
        if let RunPhase::ResponseWindow { deadline, .. } = self.phase
            && now >= deadline
        {
            return self.record(false, None);
        }

        ui.vertical_centered(|ui| {
            ui.add_space(45.0);
            ui.heading("Listen for very faint pulsed tones");
            ui.label("Click the button or press Space whenever you hear one. Some waiting periods contain no tone.");
            ui.add_space(20.0);
            let progress = self.engine.as_ref().map(TestEngine::progress).unwrap_or(1.0);
            ui.add(egui::ProgressBar::new(progress).desired_width(500.0));
            ui.label(format!("{:.0}% complete", progress * 100.0));
            ui.add_space(45.0);
        });

        let heard = ui
            .vertical_centered(|ui| {
                ui.add_sized(
                    [360.0, 120.0],
                    egui::Button::new(RichText::new("I hear it  [Space]").size(24.0)),
                )
                .clicked()
            })
            .inner
            || context.input(|input| input.key_pressed(egui::Key::Space));

        if let Some(warning) = &self.warning {
            ui.add_space(20.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new(warning).color(Color32::DARK_RED));
                ui.label("Restore the original output and volume, then resume. The current presentation was discarded.");
            });
        }

        if heard {
            match self.phase {
                RunPhase::Playing { started, .. } | RunPhase::ResponseWindow { started, .. } => {
                    let latency = now
                        .duration_since(started)
                        .as_millis()
                        .min(u32::MAX as u128) as u32;
                    return self.record(true, Some(latency));
                }
                RunPhase::Waiting(_) => {
                    if let Some(engine) = &mut self.engine {
                        engine.record_out_of_window_response();
                    }
                    self.phase = RunPhase::Waiting(now + self.random_wait());
                    return TestAction::SaveProgress;
                }
                RunPhase::Paused => {}
            }
        }

        ui.add_space(30.0);
        let mut abandon = false;
        ui.horizontal(|ui| {
            if matches!(self.phase, RunPhase::Paused) {
                if ui.button("Check setup and resume").clicked() {
                    if self.volume_matches() {
                        self.warning = None;
                        self.phase = RunPhase::Waiting(Instant::now() + Duration::from_secs(1));
                    } else {
                        self.warning = Some(
                            "The Linux output volume still differs from the start of the test."
                                .into(),
                        );
                    }
                }
            } else if ui.button("Pause").clicked() {
                self.audio.stop();
                self.pending = None;
                if let Some(engine) = &mut self.engine {
                    engine.cancel_pending();
                }
                self.phase = RunPhase::Paused;
            }
            if ui.button("Abandon test").clicked() {
                abandon = true;
            }
        });
        if abandon {
            self.audio.stop();
            self.session.status = SessionStatus::Abandoned;
            return TestAction::Abandoned(self.session.clone());
        }
        TestAction::None
    }

    fn poll_audio(&mut self) {
        while let Some(event) = self.audio.try_event() {
            match event {
                AudioEvent::Started(token) => {
                    if matches!(self.phase, RunPhase::Playing { token: expected, .. } if token == expected)
                    {
                        // Callback timing is authoritative for response latency.
                        self.phase = RunPhase::Playing {
                            token,
                            started: Instant::now(),
                        };
                    }
                }
                AudioEvent::Finished(token) => {
                    if let RunPhase::Playing {
                        token: expected,
                        started,
                    } = self.phase
                        && token == expected
                    {
                        self.phase = RunPhase::ResponseWindow {
                            started,
                            deadline: Instant::now() + Duration::from_millis(750),
                        };
                    }
                }
                AudioEvent::Stopped => {}
                AudioEvent::Error(error) => {
                    self.audio.stop();
                    self.pending = None;
                    if let Some(engine) = &mut self.engine {
                        engine.cancel_pending();
                    }
                    self.warning = Some(format!("Audio stopped: {error}"));
                    self.phase = RunPhase::Paused;
                }
            }
        }
    }

    fn schedule_presentation(&mut self) {
        if !self.volume_matches() {
            self.warning = Some("The Linux output volume changed during the test.".into());
            self.phase = RunPhase::Paused;
            return;
        }
        let catch = self.real_since_catch >= self.catch_after;
        let Some(presentation) = self
            .engine
            .as_mut()
            .and_then(|engine| engine.next_presentation(catch))
        else {
            return;
        };
        if catch {
            self.real_since_catch = 0;
            self.catch_after = rand::rng().random_range(8..=12);
        } else {
            self.real_since_catch += 1;
        }
        let token = self.next_token;
        self.next_token += 1;
        match self.audio.play(ToneRequest {
            token,
            ear: presentation.ear,
            frequency_hz: presentation.frequency_hz,
            dbfs: presentation.dbfs,
        }) {
            Ok(()) => {
                self.pending = Some(presentation);
                self.phase = RunPhase::Playing {
                    token,
                    started: Instant::now(),
                };
            }
            Err(error) => {
                self.warning = Some(error.to_string());
                self.phase = RunPhase::Paused;
            }
        }
    }

    fn record(&mut self, heard: bool, latency: Option<u32>) -> TestAction {
        self.audio.stop();
        if self.pending.take().is_none() {
            return TestAction::None;
        }
        if !self.volume_matches() {
            if let Some(engine) = &mut self.engine {
                engine.cancel_pending();
            }
            self.warning = Some("The Linux output volume changed during the presentation.".into());
            self.phase = RunPhase::Paused;
            return TestAction::None;
        }
        let Some(engine) = &mut self.engine else {
            return TestAction::None;
        };
        if let Err(error) = engine.record_response(heard, latency) {
            self.warning = Some(error.into());
            self.phase = RunPhase::Paused;
            return TestAction::None;
        }
        self.session.trials = engine.trials().to_vec();
        if engine.is_complete() {
            let engine = self.engine.take().expect("engine exists");
            let Some((thresholds, trials, mut quality)) = engine.finish() else {
                return TestAction::None;
            };
            if self.session.setup.sink_volume_percent.is_none() {
                quality.high_confidence = false;
                quality.reasons.push(
                    "Linux output volume was unavailable, so setup matching cannot be verified"
                        .into(),
                );
            }
            self.session.thresholds = thresholds;
            self.session.trials = trials;
            self.session.quality = Some(quality);
            self.session.status = SessionStatus::Completed;
            self.session.completed_at = Some(time::OffsetDateTime::now_utc());
            return TestAction::Completed(self.session.clone());
        }
        self.phase = RunPhase::Waiting(Instant::now() + self.random_wait());
        TestAction::SaveProgress
    }

    fn random_wait(&self) -> Duration {
        Duration::from_millis(rand::rng().random_range(1_200..=2_500))
    }

    fn volume_matches(&self) -> bool {
        match (
            self.session.setup.sink_volume_percent,
            system_output_volume_percent(),
        ) {
            (Some(expected), Some(current)) => (expected - current).abs() <= 0.5,
            (None, _) => true,
            (Some(_), None) => false,
        }
    }
}

fn audiogram(ui: &mut egui::Ui, session: &TestSession, baseline: Option<&TestSession>) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 350.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 6.0, Color32::from_rgb(250, 251, 252));
    let plot = Rect::from_min_max(
        Pos2::new(rect.left() + 62.0, rect.top() + 38.0),
        Pos2::new(rect.right() - 30.0, rect.bottom() - 38.0),
    );
    painter.rect_stroke(
        plot,
        0.0,
        Stroke::new(1.0, Color32::GRAY),
        StrokeKind::Inside,
    );
    for (index, frequency) in FREQUENCIES_HZ.into_iter().enumerate() {
        let x = plot.left() + index as f32 * plot.width() / 7.0;
        painter.line_segment(
            [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
            Stroke::new(0.6, Color32::LIGHT_GRAY),
        );
        painter.text(
            Pos2::new(x, plot.top() - 12.0),
            Align2::CENTER_CENTER,
            if frequency >= 1_000 {
                format!("{}k", frequency / 1_000)
            } else {
                frequency.to_string()
            },
            FontId::proportional(12.0),
            Color32::DARK_GRAY,
        );
    }
    for level in (0..=60).step_by(10) {
        let y = plot.top() + level as f32 / 60.0 * plot.height();
        painter.line_segment(
            [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(0.6, Color32::LIGHT_GRAY),
        );
        painter.text(
            Pos2::new(plot.left() - 18.0, y),
            Align2::CENTER_CENTER,
            level,
            FontId::proportional(12.0),
            Color32::DARK_GRAY,
        );
    }
    painter.text(
        Pos2::new(plot.left() - 32.0, plot.top() - 24.0),
        Align2::CENTER_CENTER,
        "dB-R",
        FontId::proportional(12.0),
        Color32::DARK_GRAY,
    );
    painter.text(
        Pos2::new(plot.center().x, rect.bottom() - 12.0),
        Align2::CENTER_CENTER,
        "Frequency (Hz)  •  lower dB-R means a quieter digital tone was heard",
        FontId::proportional(12.0),
        Color32::DARK_GRAY,
    );
    if let Some(baseline) = baseline {
        draw_screen_curves(&painter, plot, baseline, true);
    }
    draw_screen_curves(&painter, plot, session, false);
}

fn draw_screen_curves(painter: &egui::Painter, plot: Rect, session: &TestSession, baseline: bool) {
    for ear in [Ear::Right, Ear::Left] {
        let colour = if baseline {
            Color32::GRAY
        } else if ear == Ear::Right {
            Color32::from_rgb(205, 35, 45)
        } else {
            Color32::from_rgb(30, 80, 200)
        };
        let points: Vec<_> = FREQUENCIES_HZ
            .into_iter()
            .enumerate()
            .filter_map(|(index, frequency)| {
                session
                    .threshold(ear, frequency)
                    .and_then(|threshold| threshold.relative_db())
                    .map(|level| {
                        Pos2::new(
                            plot.left() + index as f32 * plot.width() / 7.0,
                            plot.top() + level.clamp(0.0, 60.0) / 60.0 * plot.height(),
                        )
                    })
            })
            .collect();
        for pair in points.windows(2) {
            if baseline {
                dashed_line(painter, pair[0], pair[1], Stroke::new(1.2, colour));
            } else {
                painter.line_segment([pair[0], pair[1]], Stroke::new(2.0, colour));
            }
        }
        if !baseline {
            for point in points {
                if ear == Ear::Right {
                    painter.circle_stroke(point, 6.0, Stroke::new(2.0, colour));
                } else {
                    painter.line_segment(
                        [point - Vec2::splat(5.0), point + Vec2::splat(5.0)],
                        Stroke::new(2.0, colour),
                    );
                    painter.line_segment(
                        [point + Vec2::new(-5.0, 5.0), point + Vec2::new(5.0, -5.0)],
                        Stroke::new(2.0, colour),
                    );
                }
            }
            for (index, frequency) in FREQUENCIES_HZ.into_iter().enumerate() {
                if session
                    .threshold(ear, frequency)
                    .is_some_and(|threshold| threshold.outcome == ThresholdOutcome::NoResponse)
                {
                    let point = Pos2::new(
                        plot.left() + index as f32 * plot.width() / 7.0,
                        plot.bottom() - 5.0,
                    );
                    painter.text(
                        point,
                        Align2::CENTER_CENTER,
                        "NR↓",
                        FontId::proportional(12.0),
                        colour,
                    );
                }
            }
        }
    }
}

fn dashed_line(painter: &egui::Painter, start: Pos2, end: Pos2, stroke: Stroke) {
    let vector = end - start;
    let length = vector.length();
    if length <= f32::EPSILON {
        return;
    }
    let direction = vector / length;
    let mut position = 0.0;
    while position < length {
        let dash_end = (position + 6.0).min(length);
        painter.line_segment(
            [start + direction * position, start + direction * dash_end],
            stroke,
        );
        position += 10.0;
    }
}

fn threshold_table(ui: &mut egui::Ui, session: &TestSession, baseline: Option<&TestSession>) {
    ui.horizontal(|ui| {
        ui.colored_label(Color32::from_rgb(205, 35, 45), "○ Right ear");
        ui.colored_label(Color32::from_rgb(30, 80, 200), "× Left ear");
        if baseline.is_some() {
            ui.colored_label(Color32::GRAY, "- - matching baseline");
        } else {
            ui.label("No earlier high-confidence session with this exact setup is available.");
        }
    });
    egui::Grid::new("threshold_table")
        .striped(true)
        .spacing([22.0, 6.0])
        .show(ui, |ui| {
            ui.strong("Frequency");
            ui.strong("Right dB-R");
            ui.strong("Left dB-R");
            ui.strong("Ear difference");
            ui.end_row();
            for frequency in FREQUENCIES_HZ {
                let right = screen_threshold(session, Ear::Right, frequency);
                let left = screen_threshold(session, Ear::Left, frequency);
                let difference = right
                    .zip(left)
                    .map(|(right, left)| format!("{:+.0} dB (R-L)", right - left))
                    .unwrap_or_else(|| "--".into());
                ui.label(format!("{frequency} Hz"));
                ui.label(
                    right
                        .map(|value| format!("{value:.0}"))
                        .unwrap_or_else(|| "NR".into()),
                );
                ui.label(
                    left.map(|value| format!("{value:.0}"))
                        .unwrap_or_else(|| "NR".into()),
                );
                ui.label(difference);
                ui.end_row();
            }
        });
    ui.small("dB-R is an app-defined digital relative scale: 0 = -90 dBFS and 60 = -30 dBFS. It is not clinical dB HL or sound-pressure level.");
}

fn screen_threshold(session: &TestSession, ear: Ear, frequency: u32) -> Option<f32> {
    session
        .threshold(ear, frequency)
        .and_then(|threshold| threshold.relative_db())
}

fn date_label(session: &TestSession) -> String {
    let date = session.started_at.date();
    format!(
        "{:04}-{:02}-{:02}",
        date.year(),
        u8::from(date.month()),
        date.day()
    )
}
