use crate::audio::{
    AudioDeviceInfo, AudioEngine, AudioEvent, ToneRequest, system_output_volume_percent,
};
use crate::domain::{
    AudioSetup, CEILING_DBFS, Ear, FREQUENCIES_HZ, PROTOCOL_VERSION, QualityIssue, SessionStatus,
    TestSession, ThresholdOutcome,
};
use crate::localization::{Language, LocalizedText, Text};
use crate::report;
use crate::storage::SessionRepository;
use crate::test_engine::{Presentation, TestEngine};
use crate::theme;
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};
use rand::Rng;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    language: Language,
    message: Option<Message>,
    delete_confirmation: Option<Uuid>,
}

#[derive(Debug, Clone)]
struct Message {
    text: LocalizedText,
    error: bool,
}

struct SetupState {
    step: SetupStep,
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
enum SetupStep {
    #[default]
    Device,
    Room,
    Sound,
    Ready,
}

impl SetupStep {
    const ALL: [Self; 4] = [Self::Device, Self::Room, Self::Sound, Self::Ready];

    fn index(self) -> usize {
        Self::ALL.iter().position(|step| *step == self).unwrap()
    }
}

impl SetupState {
    fn ready(&self) -> bool {
        self.devices.get(self.selected_device).is_some()
            && self.quiet_room
            && self.noise_cancellation
            && self.processing_off
            && self.comfortable
    }

    fn can_continue(&self) -> bool {
        match self.step {
            SetupStep::Device => self.devices.get(self.selected_device).is_some(),
            SetupStep::Room => self.quiet_room && self.noise_cancellation && self.processing_off,
            SetupStep::Sound => self.comfortable,
            SetupStep::Ready => self.ready(),
        }
    }
}

impl Default for SetupState {
    fn default() -> Self {
        Self {
            step: SetupStep::Device,
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
    warning: Option<LocalizedText>,
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
        theme::configure(&context.egui_ctx);
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
                    text: error.into(),
                    error: true,
                }),
            ),
        };
        let language = repository
            .as_ref()
            .and_then(|repository| repository.preference("language").ok().flatten());
        let language = Language::from_code(language.as_deref());
        Self {
            language,
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

    fn language_selector(&mut self, ui: &mut egui::Ui) -> bool {
        let was_open = egui::Popup::is_any_open(ui.ctx());
        let previous = self.language;
        let response = egui::ComboBox::from_id_salt("application_language")
            .selected_text(self.language.name())
            .width(95.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.language, Language::English, "English");
                ui.selectable_value(&mut self.language, Language::German, "Deutsch");
            })
            .response;
        ui.label(self.language.text(Text::Language));
        if self.language != previous {
            self.set_language(self.language);
        }
        let allow_space = hearing_shortcut_allowed(
            was_open,
            egui::Popup::is_any_open(ui.ctx()),
            response.has_focus(),
            self.message.is_some(),
        );
        if !allow_space {
            ui.ctx().input_mut(|input| {
                input.consume_key(egui::Modifiers::NONE, egui::Key::Space);
            });
        }
        allow_space
    }

    fn set_language(&mut self, language: Language) {
        self.language = language;
        let error = match &self.repository {
            Some(repository) => repository
                .set_preference("language", language.code())
                .err()
                .map(LocalizedText::from),
            None => Some(LocalizedText::from(Text::StorageUnavailable)),
        };
        if let Some(error) = error {
            self.error(LocalizedText::because(Text::LanguageSaveError, error));
        }
    }

    fn refresh_sessions(&mut self) {
        if let Some(repository) = &self.repository {
            match repository.list() {
                Ok(sessions) => self.sessions = sessions,
                Err(error) => self.error(error),
            }
        }
    }

    fn error(&mut self, text: impl Into<LocalizedText>) {
        self.message = Some(Message {
            text: text.into(),
            error: true,
        });
    }

    fn info(&mut self, text: impl Into<LocalizedText>) {
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
                self.setup.comfortable = false;
            }
            Err(error) => self.error(error),
        }
    }

    fn prepare_audio(&mut self) -> bool {
        if self.setup.audio.is_some() {
            return true;
        }
        let Some(device) = self.setup.devices.get(self.setup.selected_device) else {
            self.error(Text::SelectDevice);
            return false;
        };
        match AudioEngine::open(&device.id) {
            Ok(audio) => {
                self.setup.audio = Some(audio);
                true
            }
            Err(error) => {
                self.error(error);
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
            self.error(error);
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
            self.error(error);
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
                    self.error(error);
                }
            }
            TestAction::Completed(session) => {
                let id = session.id;
                if let Some(repository) = &self.repository
                    && let Err(error) = repository.save(&session)
                {
                    self.error(error);
                }
                self.running = None;
                self.refresh_sessions();
                self.page = Page::Results(id);
            }
            TestAction::Abandoned(session) => {
                if let Some(repository) = &self.repository
                    && let Err(error) = repository.save(&session)
                {
                    self.error(error);
                }
                self.running = None;
                self.refresh_sessions();
                self.page = Page::Dashboard;
                self.info(Text::AbandonedSaved);
            }
        }
    }
}

impl HearingApp {
    fn draw(&mut self, root_ui: &mut egui::Ui) {
        let context = root_ui.ctx().clone();
        egui::Frame::central_panel(root_ui.style())
            .inner_margin(24)
            .show(root_ui, |ui| {
                let mut allow_space = false;
                ui.horizontal(|ui| {
                    theme::sound_mark(ui, 36.0);
                    ui.label(RichText::new("Hearing Tracker").size(22.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        allow_space = self.language_selector(ui);
                    });
                });
                let language = self.language;
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.running.is_none(),
                            egui::Button::new(language.text(Text::Overview))
                                .selected(self.page == Page::Dashboard),
                        )
                        .clicked()
                    {
                        self.page = Page::Dashboard;
                    }
                    if self.page != Page::Dashboard {
                        let label = match self.page {
                            Page::Setup => Text::Prepare,
                            Page::Test => Text::Listen,
                            Page::Results(_) => Text::Results,
                            Page::Dashboard => unreachable!(),
                        };
                        ui.label(RichText::new("/ ").color(theme::MUTED));
                        theme::badge(ui, language.text(label), theme::BLUE, theme::BABY_BLUE);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.small(RichText::new(language.text(Text::Subtitle)).color(theme::MUTED));
                    });
                });
                ui.add_space(10.0);
                let warning: egui::WidgetText = RichText::new(language.text(Text::UrgentWarning))
                    .small()
                    .color(theme::AMBER)
                    .into();
                let warning_height = warning
                    .clone()
                    .into_galley(
                        ui,
                        Some(egui::TextWrapMode::Wrap),
                        (ui.available_width() - 24.0).max(1.0),
                        egui::TextStyle::Small,
                    )
                    .size()
                    .y;
                let setup_navigation_height = if self.page == Page::Setup {
                    let duration: egui::WidgetText =
                        RichText::new(language.text(Text::Duration)).small().into();
                    duration
                        .into_galley(
                            ui,
                            Some(egui::TextWrapMode::Wrap),
                            ui.available_width(),
                            egui::TextStyle::Small,
                        )
                        .size()
                        .y
                        + 74.0
                } else {
                    0.0
                };
                egui::ScrollArea::vertical()
                    .id_salt((
                        "page_content",
                        self.page,
                        (self.page == Page::Setup).then_some(self.setup.step),
                    ))
                    .auto_shrink([false, false])
                    .max_height(
                        (ui.available_height() - warning_height - setup_navigation_height - 40.0)
                            .max(0.0),
                    )
                    .show(ui, |ui| match self.page {
                        Page::Dashboard => self.dashboard_ui(ui),
                        Page::Setup => self.setup_ui(ui),
                        Page::Test => {
                            let action = self
                                .running
                                .as_mut()
                                .map(|run| run.ui(ui, &context, language, allow_space))
                                .unwrap_or(TestAction::None);
                            self.handle_test_action(action);
                        }
                        Page::Results(id) => self.results_ui(ui, id),
                    });
                if self.page == Page::Setup {
                    self.setup_navigation(ui);
                }
                ui.add_space(8.0);
                egui::Frame::new()
                    .fill(theme::AMBER_BG)
                    .corner_radius(10)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(warning);
                    });
            });

        let language = self.language;
        if let Some(message) = self.message.clone() {
            egui::Window::new(if message.error {
                language.text(Text::Problem)
            } else {
                "Hearing Tracker"
            })
            .id(egui::Id::new("app_message"))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(&context, |ui| {
                ui.set_max_width(440.0);
                ui.label(message.text.render(language));
                ui.add_space(8.0);
                if ui.add(theme::primary(language.text(Text::Ok))).clicked() {
                    self.message = None;
                }
            });
        }
    }
}

impl HearingApp {
    fn dashboard_ui(&mut self, ui: &mut egui::Ui) {
        let language = self.language;
        let compact = ui.ctx().content_rect().height() < 700.0;
        theme::card()
            .inner_margin(if compact { 14 } else { 20 })
            .fill(Color32::from_rgb(237, 246, 253))
            .stroke(Stroke::NONE)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if compact {
                    ui.spacing_mut().item_spacing.y = 6.0;
                }
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 160.0).max(240.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(width, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            theme::eyebrow(ui, language.text(Text::PersonalCheck));
                            ui.label(
                                RichText::new(language.text(Text::DashboardHeading))
                                    .size(30.0)
                                    .strong(),
                            );
                            theme::muted(ui, language.text(Text::DashboardIntro));
                            if ui
                                .add(theme::primary(language.text(Text::StartTest)))
                                .clicked()
                            {
                                self.enter_setup();
                            }
                        },
                    );
                    theme::sound_mark(ui, 140.0);
                });
            });
        ui.add_space(4.0);
        ui.columns(3, |columns| {
            summary_card(
                &mut columns[0],
                language.text(Text::CheckTime),
                language.text(Text::CheckTimeLabel),
            );
            summary_card(
                &mut columns[1],
                language.text(Text::CheckFrequencies),
                language.text(Text::CheckFrequenciesLabel),
            );
            summary_card(
                &mut columns[2],
                language.text(Text::Offline),
                language.text(Text::OfflineLabel),
            );
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.heading(language.text(Text::History));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                theme::muted(
                    ui,
                    language.format(
                        if self.sessions.len() == 1 {
                            Text::SavedSession
                        } else {
                            Text::SavedSessions
                        },
                        &[self.sessions.len().to_string()],
                    ),
                );
            });
        });
        if self.sessions.is_empty() {
            theme::card().show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.strong(language.text(Text::FirstCheck));
                theme::muted(ui, language.text(Text::NoTests));
            });
        } else {
            let rows: Vec<_> = self
                .sessions
                .iter()
                .map(|session| {
                    (
                        session.id,
                        language.date(session.started_at.date()),
                        session.status,
                        session
                            .quality
                            .as_ref()
                            .map(|quality| quality.high_confidence),
                        session.setup.device_name.clone(),
                    )
                })
                .collect();
            for (index, (id, date, status, confidence, device)) in rows.into_iter().enumerate() {
                theme::card().inner_margin(14).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        let detail_width = (ui.available_width() - 180.0).max(200.0);
                        ui.allocate_ui_with_layout(
                            Vec2::new(detail_width, 0.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(date).size(17.0).strong());
                                    if index == 0 {
                                        theme::eyebrow(ui, language.text(Text::LatestCheck));
                                    }
                                    theme::badge(
                                        ui,
                                        language.status(status),
                                        theme::BLUE,
                                        theme::BABY_BLUE,
                                    );
                                    if let Some(high) = confidence {
                                        theme::badge(
                                            ui,
                                            language.confidence(high),
                                            if high { theme::GREEN } else { theme::AMBER },
                                            if high {
                                                theme::GREEN_BG
                                            } else {
                                                theme::AMBER_BG
                                            },
                                        );
                                    }
                                });
                                theme::muted(ui, device);
                            },
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(language.text(Text::View)).clicked() {
                                self.page = Page::Results(id);
                            }
                        });
                    });
                });
            }
        }
        ui.add_space(6.0);
        theme::muted(ui, language.text(Text::PersonalOnly));
        theme::card().inner_margin(14).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::CollapsingHeader::new(language.text(Text::LocalData))
                .id_salt("local_data")
                .show(ui, |ui| {
                    ui.label(language.text(Text::ReportName));
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.display_name).desired_width(300.0),
                    );
                    if response.lost_focus()
                        && let Some(repository) = &self.repository
                        && let Err(error) =
                            repository.set_preference("display_name", &self.display_name)
                    {
                        self.error(error);
                    }
                    if let Some(repository) = &self.repository {
                        ui.small(language.format(
                            Text::DatabasePath,
                            &[repository.path().display().to_string()],
                        ));
                    }
                    ui.small(language.text(Text::Privacy));
                });
        });
    }

    fn setup_ui(&mut self, ui: &mut egui::Ui) {
        let language = self.language;
        ui.heading(language.text(Text::SetupHeading));
        let compact = ui.ctx().content_rect().height() < 700.0;
        if !compact {
            theme::muted(ui, language.text(Text::SetupIntro));
        }
        ui.add_space(4.0);
        ui.columns(4, |columns| {
            for (index, (column, label)) in columns
                .iter_mut()
                .zip([Text::Headphones, Text::Room, Text::Sound, Text::Ready])
                .enumerate()
            {
                let current = self.setup.step.index();
                let fill = if index <= current {
                    theme::BABY_BLUE
                } else {
                    Color32::WHITE
                };
                let color = if index <= current {
                    theme::BLUE
                } else {
                    theme::MUTED
                };
                egui::Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(
                        if index == current { 1.5 } else { 0.0 },
                        theme::BLUE,
                    ))
                    .corner_radius(10)
                    .inner_margin(10)
                    .show(column, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(format!("{}. {}", index + 1, language.text(label)))
                                .size(13.0)
                                .strong()
                                .color(color),
                        );
                    });
            }
        });
        ui.add_space(8.0);
        theme::card()
            .inner_margin(if compact { 14 } else { 20 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if compact {
                    ui.spacing_mut().item_spacing.y = 6.0;
                }
                theme::eyebrow(
                    ui,
                    &language.format(
                        Text::StepCount,
                        &[(self.setup.step.index() + 1).to_string()],
                    ),
                );
                match self.setup.step {
                    SetupStep::Device => {
                        ui.heading(language.text(Text::OutputStep));
                        theme::muted(ui, language.text(Text::ChooseHeadphonesHelp));
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let selected_name = self
                                .setup
                                .devices
                                .get(self.setup.selected_device)
                                .map(|device| device.name.as_str())
                                .unwrap_or(language.text(Text::NoOutput));
                            egui::ComboBox::from_id_salt("output_device")
                                .width((ui.available_width() - 150.0).min(460.0))
                                .selected_text(selected_name)
                                .show_ui(ui, |ui| {
                                    ui.set_max_width(480.0);
                                    for (index, device) in self.setup.devices.iter().enumerate() {
                                        if ui
                                            .selectable_value(
                                                &mut self.setup.selected_device,
                                                index,
                                                &device.name,
                                            )
                                            .changed()
                                        {
                                            self.setup.audio = None;
                                            self.setup.comfortable = false;
                                        }
                                    }
                                });
                            if ui.button(language.text(Text::Refresh)).clicked() {
                                self.load_devices();
                            }
                        });
                        if self.setup.devices.is_empty() {
                            theme::muted(ui, language.text(Text::NoHeadphonesHelp));
                        }
                        if let Some(device) = self.setup.devices.get(self.setup.selected_device) {
                            ui.small(
                                RichText::new(language.format(
                                    Text::DeviceDetails,
                                    &[
                                        device.sample_rate.to_string(),
                                        device.channels.to_string(),
                                        device.backend.clone(),
                                    ],
                                ))
                                .color(theme::MUTED),
                            );
                        }
                        ui.label(match self.setup.volume {
                            Some(volume) => {
                                language.format(Text::CurrentVolume, &[language.number(volume, 0)])
                            }
                            None => language.text(Text::VolumeUnknown).to_owned(),
                        });
                    }
                    SetupStep::Room => {
                        ui.heading(language.text(Text::ChecklistStep));
                        theme::muted(ui, language.text(Text::RoomHelp));
                        ui.add_space(6.0);
                        ui.checkbox(&mut self.setup.quiet_room, language.text(Text::QuietRoom));
                        ui.checkbox(
                            &mut self.setup.noise_cancellation,
                            language.text(Text::CancellationOn),
                        );
                        ui.checkbox(
                            &mut self.setup.processing_off,
                            language.text(Text::ProcessingOff),
                        );
                    }
                    SetupStep::Sound => {
                        ui.heading(language.text(Text::ReferenceStep));
                        theme::muted(ui, language.text(Text::ReferenceHelp));
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui.button(language.text(Text::PlayLeft)).clicked() {
                                self.play_setup_tone(Ear::Left);
                            }
                            if ui.button(language.text(Text::PlayRight)).clicked() {
                                self.play_setup_tone(Ear::Right);
                            }
                        });
                        ui.checkbox(
                            &mut self.setup.comfortable,
                            language.text(Text::ReferenceComfort),
                        );
                    }
                    SetupStep::Ready => {
                        ui.heading(language.text(Text::EarOrderStep));
                        theme::badge(
                            ui,
                            language.text(Text::ReadySummary),
                            theme::GREEN,
                            theme::GREEN_BG,
                        );
                        if let Some(device) = self.setup.devices.get(self.setup.selected_device) {
                            theme::muted(ui, &device.name);
                        }
                        ui.add_space(6.0);
                        ui.strong(language.text(Text::FirstEar));
                        theme::muted(ui, language.text(Text::EarOrderHelp));
                        ui.horizontal(|ui| {
                            ui.radio_value(
                                &mut self.setup.first_ear,
                                Ear::Right,
                                language.text(Text::RightFirst),
                            );
                            ui.radio_value(
                                &mut self.setup.first_ear,
                                Ear::Left,
                                language.text(Text::LeftFirst),
                            );
                        });
                    }
                }
                if !self.setup.can_continue() {
                    ui.small(RichText::new(language.text(Text::FinishSetup)).color(theme::MUTED));
                }
            });
    }

    // Keep navigation reachable even when translated instructions need scrolling.
    fn setup_navigation(&mut self, ui: &mut egui::Ui) {
        let language = self.language;
        ui.horizontal(|ui| {
            if self.setup.step != SetupStep::Device
                && ui.button(language.text(Text::Back)).clicked()
            {
                self.setup.step = SetupStep::ALL[self.setup.step.index() - 1];
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.setup.step == SetupStep::Ready {
                    if ui
                        .add_enabled(
                            self.setup.ready(),
                            theme::primary(language.text(Text::BeginTest)),
                        )
                        .clicked()
                    {
                        self.start_test();
                    }
                } else if ui
                    .add_enabled(
                        self.setup.can_continue(),
                        theme::primary(language.text(Text::Continue)),
                    )
                    .clicked()
                {
                    self.setup.step = SetupStep::ALL[self.setup.step.index() + 1];
                }
            });
        });
        ui.small(RichText::new(language.text(Text::Duration)).color(theme::MUTED));
    }

    fn results_ui(&mut self, ui: &mut egui::Ui, id: Uuid) {
        let language = self.language;
        let Some(session) = self
            .sessions
            .iter()
            .find(|session| session.id == id)
            .cloned()
        else {
            ui.label(language.text(Text::SessionMissing));
            return;
        };
        let baseline = self.baseline_for(&session);
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.heading(language.format(
                Text::ResultsDate,
                &[language.date(session.started_at.date())],
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if session.status == SessionStatus::Completed
                    && ui
                        .add(theme::primary(language.text(Text::ExportPdf)))
                        .clicked()
                {
                    let suggested = format!("hearing-report-{}.pdf", session.started_at.date());
                    if let Some(path) = rfd::FileDialog::new()
                        .set_title(language.text(Text::ExportPdf))
                        .add_filter(language.text(Text::PdfReport), &["pdf"])
                        .set_file_name(&suggested)
                        .save_file()
                    {
                        match report::save_pdf(
                            &path,
                            &session,
                            baseline.as_ref(),
                            Some(&self.display_name),
                            language,
                        ) {
                            Ok(()) => self.info(LocalizedText::with(
                                Text::ReportSaved,
                                [path.display().to_string()],
                            )),
                            Err(error) => self.error(error),
                        }
                    }
                }
            });
        });
        theme::card().inner_margin(14).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                theme::badge(
                    ui,
                    language.status(session.status),
                    theme::BLUE,
                    theme::BABY_BLUE,
                );
                if let Some(quality) = &session.quality {
                    theme::badge(
                        ui,
                        language.confidence(quality.high_confidence),
                        if quality.high_confidence {
                            theme::GREEN
                        } else {
                            theme::AMBER
                        },
                        if quality.high_confidence {
                            theme::GREEN_BG
                        } else {
                            theme::AMBER_BG
                        },
                    );
                }
                theme::muted(ui, &session.setup.device_name);
                theme::muted(
                    ui,
                    language.format(
                        Text::VolumeValue,
                        &[session
                            .setup
                            .sink_volume_percent
                            .map(|value| format!("{}%", language.number(value, 0)))
                            .unwrap_or_else(|| language.text(Text::Unavailable).into())],
                    ),
                );
            });
            if let Some(quality) = &session.quality {
                for reason in &quality.reasons {
                    ui.label(
                        RichText::new(format!("• {}", language.quality_reason(reason)))
                            .color(theme::AMBER),
                    );
                }
            }
        });
        theme::card().inner_margin(16).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.heading(language.text(Text::HearingChart));
            chart_legend(ui, baseline.is_some(), language);
            audiogram(ui, &session, baseline.as_ref(), language);
        });
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::CollapsingHeader::new(language.text(Text::DetailedValues))
                .id_salt(("detailed_values", id))
                .show(ui, |ui| {
                    threshold_table(ui, &session, language);
                });
            if baseline.is_none() {
                ui.small(RichText::new(language.text(Text::NoBaseline)).color(theme::MUTED));
            }
            ui.small(RichText::new(language.text(Text::RelativeScale)).color(theme::MUTED));
        });
        theme::card().inner_margin(14).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.strong(language.text(Text::SessionActions));
            ui.horizontal_wrapped(|ui| {
                if session
                    .quality
                    .as_ref()
                    .is_some_and(|quality| quality.high_confidence)
                    && ui.button(language.text(Text::UseBaseline)).clicked()
                    && let Some(repository) = &self.repository
                {
                    match repository.set_baseline(&session) {
                        Ok(()) => self.info(Text::BaselineUpdated),
                        Err(error) => self.error(error),
                    }
                }
                if ui
                    .button(RichText::new(language.text(Text::DeleteSession)).color(theme::RED))
                    .clicked()
                {
                    self.delete_confirmation = Some(id);
                }
            });
        });

        if self.delete_confirmation == Some(id) {
            egui::Window::new(language.text(Text::DeleteQuestion))
                .id(egui::Id::new("delete_session"))
                .collapsible(false)
                .resizable(false)
                .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ui.ctx(), |ui| {
                    ui.label(language.text(Text::DeleteWarning));
                    ui.horizontal(|ui| {
                        if ui.button(language.text(Text::Cancel)).clicked() {
                            self.delete_confirmation = None;
                        }
                        if ui
                            .button(RichText::new(language.text(Text::Delete)).color(Color32::RED))
                            .clicked()
                        {
                            if let Some(repository) = &self.repository
                                && let Err(error) = repository.delete(id)
                            {
                                self.error(error);
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

    fn ui(
        &mut self,
        ui: &mut egui::Ui,
        context: &egui::Context,
        language: Language,
        allow_space: bool,
    ) -> TestAction {
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

        let controls = test_controls(
            ui,
            context,
            language,
            allow_space,
            self.engine
                .as_ref()
                .map(TestEngine::progress)
                .unwrap_or(1.0),
            self.warning.as_ref(),
            matches!(self.phase, RunPhase::Paused),
        );

        if controls.heard {
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

        if controls.resume {
            if self.volume_matches() {
                self.warning = None;
                self.phase = RunPhase::Waiting(Instant::now() + Duration::from_secs(1));
            } else {
                self.warning = Some(Text::VolumeStillChanged.into());
            }
        } else if controls.pause {
            self.audio.stop();
            self.pending = None;
            if let Some(engine) = &mut self.engine {
                engine.cancel_pending();
            }
            self.phase = RunPhase::Paused;
        }
        if controls.abandon {
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
                    self.warning = Some(LocalizedText::with(Text::AudioStopped, [error]));
                    self.phase = RunPhase::Paused;
                }
            }
        }
    }

    fn schedule_presentation(&mut self) {
        if !self.volume_matches() {
            self.warning = Some(Text::VolumeChanged.into());
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
                self.warning = Some(error.into());
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
            self.warning = Some(Text::PresentationVolumeChanged.into());
            self.phase = RunPhase::Paused;
            return TestAction::None;
        }
        let Some(engine) = &mut self.engine else {
            return TestAction::None;
        };
        if engine.record_response(heard, latency).is_err() {
            self.warning = Some(Text::NoPending.into());
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
                quality.reasons.push(QualityIssue::VolumeUnavailable.into());
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

#[derive(Default)]
struct TestControls {
    heard: bool,
    pause: bool,
    resume: bool,
    abandon: bool,
}

fn summary_card(ui: &mut egui::Ui, value: &str, label: &str) {
    theme::card().inner_margin(10).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.label(RichText::new(value).size(17.0).strong().color(theme::BLUE));
        ui.small(RichText::new(label).color(theme::MUTED));
    });
}

fn test_controls(
    ui: &mut egui::Ui,
    context: &egui::Context,
    language: Language,
    allow_space: bool,
    progress: f32,
    warning: Option<&LocalizedText>,
    paused: bool,
) -> TestControls {
    let mut controls = TestControls::default();
    let compact = context.content_rect().height() < 700.0;
    ui.horizontal_wrapped(|ui| {
        if paused {
            controls.resume = ui
                .add(theme::primary(language.text(Text::Resume)))
                .clicked();
        } else {
            controls.pause = ui.button(language.text(Text::Pause)).clicked();
        }
        controls.abandon = ui
            .button(RichText::new(language.text(Text::Abandon)).color(theme::MUTED))
            .clicked();
    });
    theme::card()
        .inner_margin(if compact { 14 } else { 20 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if compact {
                ui.spacing_mut().item_spacing.y = 8.0;
            }
            ui.vertical_centered(|ui| {
                theme::eyebrow(ui, language.text(Text::ListeningCheck));
                theme::sound_mark(ui, if compact { 48.0 } else { 76.0 });
                ui.heading(language.text(if paused {
                    Text::PausedHeading
                } else {
                    Text::ListenHeading
                }));
                theme::muted(
                    ui,
                    language.text(if paused {
                        Text::PausedHelp
                    } else {
                        Text::ListenHelp
                    }),
                );
                ui.add_space(6.0);
                let width = ui.available_width().min(520.0);
                ui.scope(|ui| {
                    ui.visuals_mut().extreme_bg_color = theme::BABY_BLUE;
                    ui.add(
                        egui::ProgressBar::new(progress)
                            .desired_width(width)
                            .desired_height(8.0)
                            .fill(theme::BLUE),
                    );
                });
                ui.small(
                    RichText::new(
                        language.format(Text::Progress, &[language.number(progress * 100.0, 0)]),
                    )
                    .color(theme::MUTED),
                );
                ui.add_space(8.0);
                controls.heard = ui
                    .add_enabled(
                        !paused,
                        theme::primary_text(RichText::new(language.text(Text::HearIt)).size(22.0))
                            .min_size(Vec2::new(width, 82.0)),
                    )
                    .clicked();
                ui.small(RichText::new(language.text(Text::ListeningDuration)).color(theme::MUTED));
            });
        });
    controls.heard |=
        !paused && allow_space && context.input(|input| input.key_pressed(egui::Key::Space));
    if let Some(warning) = warning {
        egui::Frame::new()
            .fill(theme::AMBER_BG)
            .corner_radius(12)
            .inner_margin(14)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new(warning.render(language)).color(theme::AMBER));
                theme::muted(ui, language.text(Text::RestoreSetup));
            });
    }

    controls
}

fn chart_legend(ui: &mut egui::Ui, has_baseline: bool, language: Language) {
    ui.horizontal_wrapped(|ui| {
        theme::badge(
            ui,
            language.text(Text::RightLegend),
            theme::RED,
            Color32::from_rgb(252, 237, 239),
        );
        theme::badge(
            ui,
            language.text(Text::LeftLegend),
            theme::BLUE,
            theme::BABY_BLUE,
        );
        if has_baseline {
            theme::badge(
                ui,
                language.text(Text::BaselineLegend),
                theme::MUTED,
                theme::BACKGROUND,
            );
        }
    });
}

fn audiogram(
    ui: &mut egui::Ui,
    session: &TestSession,
    baseline: Option<&TestSession>,
    language: Language,
) {
    let height = (ui.ctx().content_rect().height() * 0.34).clamp(220.0, 320.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 12.0, theme::BACKGROUND);
    let plot = Rect::from_min_max(
        Pos2::new(rect.left() + 62.0, rect.top() + 38.0),
        Pos2::new(rect.right() - 30.0, rect.bottom() - 38.0),
    );
    painter.rect_stroke(plot, 0.0, Stroke::new(1.0, theme::LINE), StrokeKind::Inside);
    for (index, frequency) in FREQUENCIES_HZ.into_iter().enumerate() {
        let x = plot.left() + index as f32 * plot.width() / 7.0;
        painter.line_segment(
            [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
            Stroke::new(0.8, theme::LINE),
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
            theme::MUTED,
        );
    }
    for level in (0..=60).step_by(10) {
        let y = plot.top() + level as f32 / 60.0 * plot.height();
        painter.line_segment(
            [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(0.8, theme::LINE),
        );
        painter.text(
            Pos2::new(plot.left() - 18.0, y),
            Align2::CENTER_CENTER,
            level,
            FontId::proportional(12.0),
            theme::MUTED,
        );
    }
    painter.text(
        Pos2::new(plot.left() - 32.0, plot.top() - 24.0),
        Align2::CENTER_CENTER,
        "dB-R",
        FontId::proportional(12.0),
        theme::MUTED,
    );
    if let Some(baseline) = baseline {
        draw_screen_curves(&painter, plot, baseline, true, language);
    }
    draw_screen_curves(&painter, plot, session, false, language);
    ui.small(language.text(Text::ChartCaption));
}

fn draw_screen_curves(
    painter: &egui::Painter,
    plot: Rect,
    session: &TestSession,
    baseline: bool,
    language: Language,
) {
    for ear in [Ear::Right, Ear::Left] {
        let colour = if baseline {
            Color32::GRAY
        } else if ear == Ear::Right {
            theme::RED
        } else {
            theme::BLUE
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
                        format!("{}↓", language.text(Text::NoResponse)),
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

fn threshold_table(ui: &mut egui::Ui, session: &TestSession, language: Language) {
    egui::Grid::new("threshold_table")
        .striped(true)
        .spacing([22.0, 6.0])
        .show(ui, |ui| {
            ui.strong(language.text(Text::Frequency));
            ui.strong(language.text(Text::RightThreshold));
            ui.strong(language.text(Text::LeftThreshold));
            ui.strong(language.text(Text::EarDifference));
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
                    right.map(|value| format!("{value:.0}")).unwrap_or_else(|| {
                        screen_outcome(session, Ear::Right, frequency, language)
                    }),
                );
                ui.label(
                    left.map(|value| format!("{value:.0}"))
                        .unwrap_or_else(|| screen_outcome(session, Ear::Left, frequency, language)),
                );
                ui.label(difference);
                ui.end_row();
            }
        });
    ui.small(language.text(Text::NoResponseHelp));
}

fn screen_threshold(session: &TestSession, ear: Ear, frequency: u32) -> Option<f32> {
    session
        .threshold(ear, frequency)
        .and_then(|threshold| threshold.relative_db())
}

fn screen_outcome(session: &TestSession, ear: Ear, frequency: u32, language: Language) -> String {
    if session
        .threshold(ear, frequency)
        .is_some_and(|threshold| threshold.outcome == ThresholdOutcome::NoResponse)
    {
        language.text(Text::NoResponse).into()
    } else {
        "--".into()
    }
}

fn hearing_shortcut_allowed(
    was_open: bool,
    is_open: bool,
    selector_focused: bool,
    message_open: bool,
) -> bool {
    !was_open && !is_open && !selector_focused && !message_open
}

impl eframe::App for HearingApp {
    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(root_ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{QualityAssessment, Threshold};
    use egui::epaint::{ClippedShape, Shape};

    fn sample_app(repository: Option<SessionRepository>) -> HearingApp {
        let setup = AudioSetup {
            device_id: "test".into(),
            device_name: "AirPods Pro".into(),
            backend: "ALSA".into(),
            sample_rate: 48_000,
            channels: 2,
            sink_volume_percent: Some(50.0),
            listening_mode: "Noise cancellation".into(),
            ceiling_dbfs: CEILING_DBFS,
            protocol_version: PROTOCOL_VERSION,
        };
        let mut session = TestSession::new(setup);
        session.status = SessionStatus::Completed;
        session.quality = Some(QualityAssessment {
            high_confidence: true,
            catch_trials: 6,
            false_positive_catches: 0,
            max_retest_difference_db: 0.0,
            reasons: vec![],
        });
        for ear in [Ear::Right, Ear::Left] {
            for frequency_hz in FREQUENCIES_HZ {
                session.thresholds.push(Threshold {
                    ear,
                    frequency_hz,
                    dbfs: Some(-60.0),
                    outcome: ThresholdOutcome::Measured,
                });
            }
        }
        HearingApp {
            repository,
            sessions: vec![session],
            page: Page::Dashboard,
            setup: SetupState::default(),
            running: None,
            display_name: "Jörg Weiß".into(),
            language: Language::English,
            message: None,
            delete_confirmation: None,
        }
    }

    fn input() -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(850.0, 620.0))),
            ..Default::default()
        }
    }

    fn visit_text(shape: &Shape, visit: &mut impl FnMut(&egui::epaint::TextShape)) {
        match shape {
            Shape::Text(text) => visit(text),
            Shape::Vec(shapes) => {
                for shape in shapes {
                    visit_text(shape, visit);
                }
            }
            _ => {}
        }
    }

    fn visible_text(shapes: &[ClippedShape]) -> String {
        visible_text_at_width(shapes, 850.0)
    }

    fn visible_text_at_width(shapes: &[ClippedShape], width: f32) -> String {
        let mut values = Vec::new();
        for shape in shapes {
            visit_text(&shape.shape, &mut |text| {
                let bounds = text.galley.rect.translate(text.pos.to_vec2());
                if bounds.intersects(shape.clip_rect) {
                    assert!(
                        bounds.min.x >= -1.0 && bounds.max.x <= width + 1.0,
                        "text exceeds minimum window width: {} at {bounds:?}",
                        text.galley.job.text
                    );
                    values.push(text.galley.job.text.clone());
                }
            });
        }
        values.join("\n")
    }

    #[test]
    fn switching_language_preserves_setup_and_handles_save_failures() {
        let directory = tempfile::tempdir().unwrap();
        let repository = SessionRepository::open(directory.path().join("test.sqlite")).unwrap();
        let mut app = sample_app(Some(repository));
        app.page = Page::Setup;
        app.setup.quiet_room = true;
        app.setup.noise_cancellation = true;
        app.setup.processing_off = true;
        app.setup.comfortable = true;
        app.setup.first_ear = Ear::Left;
        app.setup.step = SetupStep::Sound;
        let original = serde_json::to_string(&app.sessions).unwrap();
        app.set_language(Language::German);
        assert_eq!(app.page, Page::Setup);
        assert_eq!(app.setup.step, SetupStep::Sound);
        assert!(
            app.setup.quiet_room
                && app.setup.noise_cancellation
                && app.setup.processing_off
                && app.setup.comfortable
        );
        assert_eq!(app.setup.first_ear, Ear::Left);
        assert_eq!(app.display_name, "Jörg Weiß");
        assert_eq!(serde_json::to_string(&app.sessions).unwrap(), original);
        assert_eq!(
            app.repository
                .as_ref()
                .unwrap()
                .preference("language")
                .unwrap()
                .as_deref(),
            Some("de")
        );
        // An existing database can become unwritable after startup.
        let failing = SessionRepository::open(directory.path().join("readonly.sqlite")).unwrap();
        failing.make_read_only_for_test();
        app.repository = Some(failing);
        app.set_language(Language::English);
        assert_eq!(app.language, Language::English);
        assert!(
            app.message
                .as_ref()
                .unwrap()
                .text
                .render(app.language)
                .contains("could not be saved")
        );
        app.set_language(Language::German);
        assert!(
            app.message
                .as_ref()
                .unwrap()
                .text
                .render(app.language)
                .contains("konnte aber nicht gespeichert werden")
        );
    }

    #[test]
    fn desktop_windows_render_all_pages_and_translated_dialogs() {
        for size in [Vec2::new(850.0, 620.0), Vec2::new(1050.0, 760.0)] {
            for language in [Language::English, Language::German] {
                for page in [
                    "dashboard",
                    "empty-dashboard",
                    "setup",
                    "setup-room",
                    "setup-sound",
                    "setup-ready",
                    "setup-end",
                    "results",
                    "results-end",
                    "message",
                    "delete",
                ] {
                    let mut app = sample_app(None);
                    app.language = language;
                    app.setup.devices.push(AudioDeviceInfo {
                        id: "test".into(),
                        name: "AirPods Pro".into(),
                        backend: "ALSA".into(),
                        sample_rate: 48_000,
                        channels: 2,
                    });
                    app.setup.volume = Some(50.0);
                    app.page = match page {
                        "setup" | "setup-room" | "setup-sound" | "setup-ready" | "setup-end" => {
                            Page::Setup
                        }
                        "results" | "results-end" | "delete" => Page::Results(app.sessions[0].id),
                        _ => Page::Dashboard,
                    };
                    if page == "empty-dashboard" {
                        app.sessions.clear();
                    }
                    app.setup.step = match page {
                        "setup-room" => SetupStep::Room,
                        "setup-sound" => SetupStep::Sound,
                        "setup-ready" | "setup-end" => SetupStep::Ready,
                        _ => SetupStep::Device,
                    };
                    if matches!(app.setup.step, SetupStep::Ready) {
                        app.setup.quiet_room = true;
                        app.setup.noise_cancellation = true;
                        app.setup.processing_off = true;
                        app.setup.comfortable = true;
                    }
                    if page == "message" {
                        app.info(Text::BaselineUpdated);
                    }
                    if page == "delete" {
                        app.delete_confirmation = Some(app.sessions[0].id);
                    }
                    let context = egui::Context::default();
                    theme::configure(&context);

                    let mut deltas = Vec::new();
                    let mut output = None;
                    for pass in 0..5 {
                        let mut raw = input();
                        raw.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, size));
                        raw.time = Some(pass as f64 * 0.25);
                        if page.ends_with("-end") && pass > 1 {
                            raw.events = vec![
                                egui::Event::PointerMoved(Pos2::new(400.0, 300.0)),
                                egui::Event::MouseWheel {
                                    unit: egui::MouseWheelUnit::Point,
                                    phase: egui::TouchPhase::Move,
                                    delta: Vec2::new(0.0, -800.0),
                                    modifiers: egui::Modifiers::NONE,
                                },
                            ];
                        }
                        let frame = context.run_ui(raw, |ui| app.draw(ui));
                        deltas.extend(frame.textures_delta.set.clone());
                        output = Some(frame);
                    }
                    let output = output.unwrap();
                    let text = visible_text_at_width(&output.shapes, size.x);
                    assert!(text.contains(language.name()));
                    assert!(
                        text.contains(language.text(Text::UrgentWarning)),
                        "footer not visible: {text}"
                    );
                    match page {
                        "dashboard" => assert!(text.contains(language.text(Text::History))),
                        "setup" | "setup-room" | "setup-sound" | "setup-ready" => {
                            assert!(text.contains(language.text(Text::SetupHeading)))
                        }
                        "empty-dashboard" => {
                            assert!(text.contains(language.text(Text::FirstCheck)))
                        }
                        "setup-end" => assert!(text.contains(language.text(Text::Duration))),
                        "results-end" => assert!(text.contains(language.text(Text::RelativeScale))),
                        "results" => assert!(text.contains(language.text(Text::HighConfidence))),
                        "message" => assert!(text.contains(language.text(Text::BaselineUpdated))),
                        "delete" => assert!(text.contains(language.text(Text::DeleteQuestion))),
                        _ => unreachable!(),
                    }
                    export_preview(
                        &context,
                        &output,
                        deltas,
                        &format!(
                            "{page}{}-{}",
                            if size.x > 850.0 { "-default" } else { "" },
                            language.code()
                        ),
                    );
                }
            }
        }
    }

    fn text_center(output: &egui::FullOutput, value: &str) -> Pos2 {
        let mut center = None;
        for shape in &output.shapes {
            visit_text(&shape.shape, &mut |text| {
                if text.galley.job.text == value {
                    center = Some(text.galley.rect.translate(text.pos.to_vec2()).center());
                }
            });
        }
        center.unwrap_or_else(|| {
            panic!(
                "missing UI text: {value}; visible: {}",
                visible_text(&output.shapes)
            )
        })
    }

    fn click(app: &mut HearingApp, context: &egui::Context, position: Pos2) -> egui::FullOutput {
        let mut output = None;
        for pressed in [true, false] {
            let mut raw = input();
            raw.events = vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ];
            output = Some(context.run_ui(raw, |ui| app.draw(ui)));
        }
        output.unwrap()
    }

    #[test]
    fn guided_setup_requires_confirmations_and_preserves_them_when_going_back() {
        for language in [Language::English, Language::German] {
            let mut app = sample_app(None);
            app.language = language;
            app.page = Page::Setup;
            let context = egui::Context::default();
            theme::configure(&context);
            let render = |app: &mut HearingApp| {
                let _ = context.run_ui(input(), |ui| app.draw(ui));
                context.run_ui(input(), |ui| app.draw(ui))
            };
            let output = render(&mut app);
            click(
                &mut app,
                &context,
                text_center(&output, language.text(Text::Continue)),
            );
            assert_eq!(app.setup.step, SetupStep::Device);

            app.setup.devices.push(AudioDeviceInfo {
                id: "test".into(),
                name: "AirPods Pro".into(),
                backend: "ALSA".into(),
                sample_rate: 48_000,
                channels: 2,
            });
            let output = render(&mut app);
            click(
                &mut app,
                &context,
                text_center(&output, language.text(Text::Continue)),
            );
            assert_eq!(app.setup.step, SetupStep::Room);

            for (label, should_advance) in [
                (Text::QuietRoom, false),
                (Text::CancellationOn, false),
                (Text::ProcessingOff, true),
            ] {
                let output = render(&mut app);
                click(
                    &mut app,
                    &context,
                    text_center(&output, language.text(label)),
                );
                let output = render(&mut app);
                click(
                    &mut app,
                    &context,
                    text_center(&output, language.text(Text::Continue)),
                );
                assert_eq!(
                    app.setup.step,
                    if should_advance {
                        SetupStep::Sound
                    } else {
                        SetupStep::Room
                    }
                );
            }

            let output = render(&mut app);
            click(
                &mut app,
                &context,
                text_center(&output, language.text(Text::Continue)),
            );
            assert_eq!(app.setup.step, SetupStep::Sound);
            let output = render(&mut app);
            click(
                &mut app,
                &context,
                text_center(&output, language.text(Text::ReferenceComfort)),
            );
            let output = render(&mut app);
            click(
                &mut app,
                &context,
                text_center(&output, language.text(Text::Continue)),
            );
            assert_eq!(app.setup.step, SetupStep::Ready);
            assert!(app.setup.ready());

            let output = render(&mut app);
            click(
                &mut app,
                &context,
                text_center(&output, language.text(Text::LeftFirst)),
            );
            let output = render(&mut app);
            click(
                &mut app,
                &context,
                text_center(&output, language.text(Text::Back)),
            );
            assert_eq!(app.setup.step, SetupStep::Sound);
            assert!(app.setup.ready());
            assert_eq!(app.setup.first_ear, Ear::Left);

            // Changing the reference confirmation prevents advancing again.
            let output = render(&mut app);
            click(
                &mut app,
                &context,
                text_center(&output, language.text(Text::ReferenceComfort)),
            );
            let output = render(&mut app);
            click(
                &mut app,
                &context,
                text_center(&output, language.text(Text::Continue)),
            );
            assert_eq!(app.setup.step, SetupStep::Sound);
            assert!(!app.setup.ready());
        }
    }

    #[test]
    fn paused_listening_ignores_the_response_shortcut() {
        let context = egui::Context::default();
        theme::configure(&context);
        let mut raw = input();
        raw.events.push(egui::Event::Key {
            key: egui::Key::Space,
            physical_key: Some(egui::Key::Space),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = context.run_ui(raw, |ui| {
            let controls = test_controls(ui, &context, Language::English, true, 0.5, None, true);
            assert!(!controls.heard);
        });
    }

    #[test]
    fn dropdown_switches_an_open_dialog_and_persists_the_choice() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.sqlite");
        let repository = SessionRepository::open(&path).unwrap();
        let mut app = sample_app(Some(repository));
        app.page = Page::Setup;
        app.setup.quiet_room = true;
        app.info(Text::BaselineUpdated);
        let context = egui::Context::default();
        theme::configure(&context);
        let _ = context.run_ui(input(), |ui| app.draw(ui));
        let first = context.run_ui(input(), |ui| app.draw(ui));
        click(&mut app, &context, text_center(&first, "English"));
        let opened = context.run_ui(input(), |ui| app.draw(ui));
        assert!(
            visible_text(&opened.shapes).contains("Deutsch"),
            "{}",
            visible_text(&opened.shapes)
        );
        click(&mut app, &context, text_center(&opened, "Deutsch"));
        let changed = context.run_ui(input(), |ui| app.draw(ui));
        assert_eq!(app.language, Language::German);
        assert!(app.setup.quiet_room);
        assert_eq!(app.page, Page::Setup);
        assert!(
            visible_text(&changed.shapes).contains(Language::German.text(Text::BaselineUpdated))
        );
        assert_eq!(app.display_name, "Jörg Weiß");
        drop(app);
        let reopened = SessionRepository::open(&path).unwrap();
        assert_eq!(
            reopened.preference("language").unwrap().as_deref(),
            Some("de")
        );
    }

    #[test]
    fn language_popup_space_does_not_record_a_hearing_response() {
        let context = egui::Context::default();
        theme::configure(&context);
        let mut app = sample_app(None);
        let _ = context.run_ui(input(), |ui| {
            app.language_selector(ui);
        });
        egui::Popup::open_id(&context, egui::Id::new("test_language_popup"));
        let mut raw = input();
        raw.events.push(egui::Event::Key {
            key: egui::Key::Space,
            physical_key: Some(egui::Key::Space),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = context.run_ui(raw, |ui| {
            let allow_space = app.language_selector(ui);
            let controls = test_controls(ui, &context, app.language, allow_space, 0.5, None, false);
            assert!(!controls.heard);
        });
        egui::Popup::close_all(&context);
        let mut raw = input();
        raw.events = vec![egui::Event::Key {
            key: egui::Key::Space,
            physical_key: Some(egui::Key::Space),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }];
        let _ = context.run_ui(raw, |ui| {
            let controls = test_controls(ui, &context, Language::German, true, 0.5, None, false);
            assert!(controls.heard);
        });
    }

    #[test]
    fn running_and_paused_controls_fit_and_translate_warnings() {
        for language in [Language::English, Language::German] {
            for paused in [false, true] {
                let context = egui::Context::default();
                theme::configure(&context);
                context.set_theme(egui::Theme::Light);
                let warning = LocalizedText::from(Text::VolumeChanged);
                let mut deltas = Vec::new();
                let mut output = None;
                for _ in 0..3 {
                    let frame = context.run_ui(input(), |ui| {
                        egui::Frame::central_panel(ui.style()).show(ui, |ui| {
                            let controls = test_controls(
                                ui,
                                &context,
                                language,
                                true,
                                0.5,
                                paused.then_some(&warning),
                                paused,
                            );
                            assert!(
                                !controls.heard
                                    && !controls.pause
                                    && !controls.resume
                                    && !controls.abandon
                            );
                        });
                    });
                    deltas.extend(frame.textures_delta.set.clone());
                    output = Some(frame);
                }
                let output = output.unwrap();
                let text = visible_text(&output.shapes);
                assert!(text.contains(language.text(Text::HearIt)));
                assert!(text.contains(language.text(if paused {
                    Text::Resume
                } else {
                    Text::Pause
                })));
                if paused {
                    assert!(text.contains(language.text(Text::VolumeChanged)));
                }
                export_preview(
                    &context,
                    &output,
                    deltas,
                    &format!(
                        "test-{}-{}",
                        if paused { "paused" } else { "running" },
                        language.code()
                    ),
                );
            }
        }
    }

    // Optional output for visual QA, using the real egui geometry and font atlas.
    fn export_preview(
        context: &egui::Context,
        output: &egui::FullOutput,
        deltas: Vec<(egui::TextureId, egui::epaint::ImageDelta)>,
        name: &str,
    ) {
        let Ok(directory) = std::env::var("HEARING_TEST_QA_DIR") else {
            return;
        };
        let textures: Vec<_> = deltas
            .into_iter()
            .map(|(id, delta)| {
                let egui::ImageData::Color(image) = delta.image;
                serde_json::json!({"id": format!("{id:?}"), "size": image.size, "pos": delta.pos,
                "pixels": image.pixels.iter().map(Color32::to_array).collect::<Vec<_>>()})
            })
            .collect();
        let meshes: Vec<_> = context.tessellate(output.shapes.clone(), output.pixels_per_point).into_iter().filter_map(|primitive| {
            let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else { return None; };
            Some(serde_json::json!({"texture": format!("{:?}", mesh.texture_id),
                "clip": [primitive.clip_rect.min.x, primitive.clip_rect.min.y, primitive.clip_rect.max.x, primitive.clip_rect.max.y],
                "indices": mesh.indices,
                "vertices": mesh.vertices.iter().map(|vertex| serde_json::json!({"pos": [vertex.pos.x, vertex.pos.y], "uv": [vertex.uv.x, vertex.uv.y], "color": vertex.color.to_array()})).collect::<Vec<_>>() }))
        }).collect();
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            std::path::Path::new(&directory).join(format!("{name}.json")),
            serde_json::to_vec(
                &serde_json::json!({"size": [context.content_rect().width() as u32, context.content_rect().height() as u32], "textures": textures, "meshes": meshes}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}
