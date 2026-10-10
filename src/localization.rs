//! Offline translations. Every key requires both languages at compile time.
use crate::audio::AudioError;
use crate::domain::{QualityIssue, QualityReason, SessionStatus};
use crate::report::ReportError;
use crate::storage::StorageError;
use time::Date;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    #[default]
    English,
    German,
}

macro_rules! catalog {
    ($($key:ident => ($en:literal, $de:literal)),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Text { $($key),* }
        impl Language {
            pub const fn text(self, key: Text) -> &'static str {
                match key { $(Text::$key => match self { Self::English => $en, Self::German => $de }),* }
            }
        }
        #[cfg(test)]
        const ALL_TEXT: &[Text] = &[$(Text::$key),*];
    }
}
catalog! {
    Language => ("Language", "Sprache"),
    UrgentWarning => ("Sudden hearing loss, hearing worsening over days or weeks, or hearing loss with earache or discharge needs urgent medical assessment. Do not wait for a home test.", "Plötzlicher Hörverlust, eine Verschlechterung über Tage oder Wochen oder Hörverlust mit Ohrenschmerzen oder Ausfluss erfordern eine dringende ärztliche Untersuchung. Warten Sie nicht auf einen Heimtest."),
    SelectDevice => ("Select a stereo output device first", "Wählen Sie zuerst ein Stereo-Ausgabegerät aus"),
    AbandonedSaved => ("The incomplete test was saved as abandoned and excluded from trends.", "Der unvollständige Test wurde als abgebrochen gespeichert und von Verlaufsvergleichen ausgeschlossen."),
    Subtitle => ("A little time for your listening", "Ein wenig Zeit für Ihr Hören"),
    Problem => ("Problem", "Problem"),
    Ok => ("OK", "OK"),
    DashboardHeading => ("Tune in to your hearing.", "Zeit, genau hinzuhören."),
    DashboardIntro => ("Find a quiet moment, put in your AirPods, and build a picture of your hearing over time. Use the same setup each time for meaningful comparisons.", "Nehmen Sie sich einen ruhigen Moment, setzen Sie Ihre AirPods ein und verfolgen Sie Ihr Hören über die Zeit. Nutzen Sie für Vergleiche immer dieselbe Konfiguration."),
    StartTest => ("Start a listening check", "Hörcheck starten"),
    History => ("Your listening history", "Ihre bisherigen Hörchecks"),
    SavedSession => ("{} saved check", "{} gespeicherter Hörcheck"),
    SavedSessions => ("{} saved checks", "{} gespeicherte Hörchecks"),
    NoTests => ("No tests yet. Your first complete, high-confidence test becomes the baseline for this exact audio setup.", "Noch keine Tests. Ihr erster vollständiger Test mit hoher Zuverlässigkeit wird zur Referenz für genau diese Audiokonfiguration."),
    View => ("View", "Ansehen"),
    LocalData => ("Report identity and local data", "Name im Bericht und lokale Daten"),
    ReportName => ("Optional name on PDF reports:", "Optionaler Name im PDF-Bericht:"),
    DatabasePath => ("Local database: {}", "Lokale Datenbank: {}"),
    Privacy => ("Data stays on this computer and is not encrypted or uploaded.", "Die Daten bleiben auf diesem Computer und werden weder verschlüsselt noch hochgeladen."),
    SetupHeading => ("Let's get you ready.", "Machen wir alles bereit."),
    SetupIntro => ("Four simple steps for a comfortable, repeatable listening check.", "Vier einfache Schritte für einen angenehmen, wiederholbaren Hörcheck."),
    OutputStep => ("Choose your headphones", "Kopfhörer auswählen"),
    NoOutput => ("No output device", "Kein Ausgabegerät"),
    Refresh => ("Refresh", "Aktualisieren"),
    DeviceDetails => ("{} Hz, {} channels, {} backend", "{} Hz, {} Kanäle, Backend: {}"),
    CurrentVolume => ("Current default-output volume: {}% (recorded for comparisons)", "Aktuelle Standard-Ausgabelautstärke: {}% (für Vergleiche gespeichert)"),
    VolumeUnknown => ("Linux output volume could not be read; this test can be viewed but will not qualify for trends.", "Die Linux-Ausgabelautstärke konnte nicht gelesen werden. Dieser Test kann angesehen, aber nicht für Verlaufsvergleiche verwendet werden."),
    ChecklistStep => ("Settle into a quiet space", "Einen ruhigen Platz finden"),
    QuietRoom => ("I am in a quiet room and both AirPods fit securely", "Ich bin in einem ruhigen Raum und beide AirPods sitzen sicher"),
    CancellationOn => ("Noise Cancellation is on (not Adaptive or Transparency)", "Die Geräuschunterdrückung ist eingeschaltet (nicht Adaptiv oder Transparenz)"),
    ProcessingOff => ("Spatial Audio, personalized volume, conversation awareness, headphone accommodations, and hearing assistance are off", "3D-Audio, personalisierte Lautstärke, Konversationserkennung, Kopfhörer-Anpassungen und Hörunterstützung sind ausgeschaltet"),
    ReferenceStep => ("Try a gentle reference sound", "Einen leisen Referenzton testen"),
    ReferenceHelp => ("The test never exceeds this -30 dBFS reference. It should be clearly audible but not loud. Do not turn the volume up if it is uncomfortable or inaudible.", "Der Test überschreitet diesen Referenzpegel von -30 dBFS nie. Der Ton sollte deutlich hörbar, aber nicht laut sein. Erhöhen Sie die Lautstärke nicht, wenn er unangenehm oder unhörbar ist."),
    PlayLeft => ("Play left", "Links abspielen"),
    PlayRight => ("Play right", "Rechts abspielen"),
    ReferenceComfort => ("I heard the correct side and the reference was comfortable, not loud", "Ich habe die richtige Seite gehört und der Referenzton war angenehm, nicht laut"),
    EarOrderStep => ("You're ready to listen", "Bereit zum Hinhören"),
    EarOrderHelp => ("If you know which ear is better, test it first. Otherwise keep Right selected.", "Wenn Sie wissen, welches Ohr besser hört, testen Sie es zuerst. Lassen Sie andernfalls Rechts ausgewählt."),
    RightFirst => ("Right first / unsure", "Rechts zuerst / unsicher"),
    LeftFirst => ("Left first", "Links zuerst"),
    BeginTest => ("Begin test", "Test beginnen"),
    Duration => ("Expected duration: about 12-18 minutes. Press Space or click only when you hear a tone, even if it is very faint.", "Voraussichtliche Dauer: etwa 12-18 Minuten. Drücken Sie die Leertaste oder klicken Sie nur, wenn Sie einen Ton hören, auch wenn er sehr leise ist."),
    SessionMissing => ("This session is no longer available.", "Dieser Test ist nicht mehr verfügbar."),
    ResultsDate => ("Results - {}", "Ergebnisse - {}"),
    ExportPdf => ("Export PDF", "PDF exportieren"),
    PdfReport => ("PDF report", "PDF-Bericht"),
    ReportSaved => ("Report saved to {}", "Bericht gespeichert unter {}"),
    VolumeValue => ("Volume: {}", "Lautstärke: {}"),
    Unavailable => ("unavailable", "nicht verfügbar"),
    UseBaseline => ("Use as baseline for this setup", "Als Referenz für diese Konfiguration verwenden"),
    BaselineUpdated => ("Baseline updated for this exact setup.", "Die Referenz für genau diese Konfiguration wurde aktualisiert."),
    DeleteSession => ("Delete session…", "Test löschen…"),
    DeleteQuestion => ("Delete this session?", "Diesen Test löschen?"),
    DeleteWarning => ("This permanently removes the local session and its raw trials.", "Der lokale Test und seine einzelnen Messversuche werden dauerhaft gelöscht."),
    Cancel => ("Cancel", "Abbrechen"),
    Delete => ("Delete", "Löschen"),
    ListenHeading => ("Take your time. Tune in.", "In Ruhe hinhören."),
    ListenHelp => ("Click the button or press Space whenever you hear one. Some waiting periods contain no tone.", "Klicken Sie auf die Schaltfläche oder drücken Sie die Leertaste, sobald Sie einen Ton hören. Einige Warteintervalle enthalten keinen Ton."),
    Progress => ("{}% complete", "{}% abgeschlossen"),
    HearIt => ("I hear it  [Space]", "Ich höre den Ton  [Leertaste]"),
    RestoreSetup => ("Restore the original output and volume, then resume. The current presentation was discarded.", "Stellen Sie die ursprüngliche Ausgabe und Lautstärke wieder her und setzen Sie den Test fort. Der aktuelle Messversuch wurde verworfen."),
    Resume => ("Check setup and resume", "Konfiguration prüfen und fortsetzen"),
    Pause => ("Pause", "Pausieren"),
    Abandon => ("Abandon test", "Test abbrechen"),
    VolumeStillChanged => ("The Linux output volume still differs from the start of the test.", "Die Linux-Ausgabelautstärke weicht weiterhin vom Wert zu Testbeginn ab."),
    AudioStopped => ("Audio stopped: {}", "Audio gestoppt: {}"),
    VolumeChanged => ("The Linux output volume changed during the test.", "Die Linux-Ausgabelautstärke hat sich während des Tests geändert."),
    PresentationVolumeChanged => ("The Linux output volume changed during the presentation.", "Die Linux-Ausgabelautstärke hat sich während des Messversuchs geändert."),
    NoPending => ("no presentation is pending", "Es steht kein Messversuch aus"),
    VolumeUnverified => ("Linux output volume was unavailable, so setup matching cannot be verified", "Die Linux-Ausgabelautstärke war nicht verfügbar. Daher kann die Übereinstimmung der Konfiguration nicht überprüft werden"),
    ChartCaption => ("Frequency (Hz)  •  lower dB-R means a quieter digital tone was heard", "Frequenz (Hz)  •  niedrigere dB-R bedeuten, dass ein leiserer digitaler Ton gehört wurde"),
    RightLegend => ("○ Right ear", "○ Rechtes Ohr"),
    LeftLegend => ("× Left ear", "× Linkes Ohr"),
    BaselineLegend => ("- - matching baseline", "- - passende Referenz"),
    NoBaseline => ("No earlier high-confidence session with this exact setup is available.", "Es gibt keinen früheren Test mit hoher Zuverlässigkeit für genau diese Konfiguration."),
    Frequency => ("Frequency", "Frequenz"),
    RightThreshold => ("Right dB-R", "Rechts dB-R"),
    LeftThreshold => ("Left dB-R", "Links dB-R"),
    EarDifference => ("Ear difference", "Ohrdifferenz"),
    RelativeScale => ("dB-R is an app-defined digital relative scale: 0 = -90 dBFS and 60 = -30 dBFS. It is not clinical dB HL or sound-pressure level.", "dB-R ist eine von der App definierte relative digitale Skala: 0 = -90 dBFS und 60 = -30 dBFS. Sie entspricht weder klinischen dB HL noch einem Schalldruckpegel."),
    NoResponse => ("NR", "KA"),
    NoResponseHelp => ("NR = no response; -- = unavailable", "KA = keine Antwort; -- = nicht verfügbar"),
    InProgress => ("In progress", "Laufend"),
    Completed => ("Completed", "Abgeschlossen"),
    Abandoned => ("Abandoned", "Abgebrochen"),
    HighConfidence => ("High confidence", "Hohe Zuverlässigkeit"),
    LowConfidence => ("Low confidence", "Geringe Zuverlässigkeit"),
    RepeatDifference => ("The 1000 Hz repeat differed by {} dB", "Die Wiederholungsmessung bei 1000 Hz wich um {} dB ab"),
    FewSilentChecks => ("Too few silent checks were completed", "Es wurden zu wenige stille Kontrollversuche abgeschlossen"),
    SilentResponses => ("Responses occurred on {}% of silent checks", "Bei {}% der stillen Kontrollversuche wurde eine Antwort gegeben"),
    NoDevices => ("no output devices are available", "Es sind keine Ausgabegeräte verfügbar"),
    DeviceMissing => ("the selected output device is no longer available", "Das ausgewählte Ausgabegerät ist nicht mehr verfügbar"),
    NotStereo => ("the selected output must provide at least two channels", "Die ausgewählte Ausgabe muss mindestens zwei Kanäle bieten"),
    UnsupportedFormat => ("unsupported sample format: {}", "Nicht unterstütztes Sample-Format: {}"),
    AudioError => ("audio system error: {}", "Fehler im Audiosystem: {}"),
    NoDataDirectory => ("could not determine the local application-data directory", "Das lokale Verzeichnis für Anwendungsdaten konnte nicht ermittelt werden"),
    StorageUnavailable => ("Local storage is unavailable", "Der lokale Speicher ist nicht verfügbar"),
    FileError => ("file operation failed: {}", "Dateivorgang fehlgeschlagen: {}"),
    DatabaseError => ("database operation failed: {}", "Datenbankvorgang fehlgeschlagen: {}"),
    InvalidData => ("stored session data is invalid: {}", "Gespeicherte Testdaten sind ungültig: {}"),
    InvalidBaseline => ("the selected session cannot be used as this setup's baseline", "Der ausgewählte Test kann nicht als Referenz für diese Konfiguration verwendet werden"),
    ReportError => ("could not write the report: {}", "Der Bericht konnte nicht geschrieben werden: {}"),
    LanguageSaveError => ("The language changed for this launch, but could not be saved: {}", "Die Sprache wurde für diesen Programmstart geändert, konnte aber nicht gespeichert werden: {}"),
    ReportTitle => ("Personal Hearing Trend Report", "Persönlicher Bericht zum Hörverlauf"),
    ReportSubtitle => ("UNCALIBRATED - personal monitoring only; not a clinical audiogram", "UNKALIBRIERT - nur persönliche Verlaufskontrolle; kein klinisches Audiogramm"),
    ReportIdentity => ("Name: {}    Test date: {}    Session: {}", "Name: {}    Testdatum: {}    Test: {}"),
    NotProvided => ("Not provided", "Nicht angegeben"),
    ReportOutput => ("Output: {} | {} Hz | volume {} | {}", "Ausgabe: {} | {} Hz | Lautstärke {} | {}"),
    NoiseCancellation => ("Noise cancellation", "Geräuschunterdrückung"),
    ReportQuality => ("Quality: {}{}", "Qualität: {}{}"),
    ReportBaseline => (" | comparison baseline {}", " | Vergleichsreferenz {}"),
    NotAssessed => ("Not assessed", "Nicht bewertet"),
    ReadReport => ("How to read this report", "So lesen Sie diesen Bericht"),
    ReportLegend => ("Red circles = right ear; blue crosses = left ear; grey dashed = baseline. Lower dB-R means a quieter digital tone was heard.", "Rote Kreise = rechtes Ohr; blaue Kreuze = linkes Ohr; grau gestrichelt = Referenz. Niedrigere dB-R bedeuten, dass ein leiserer digitaler Ton gehört wurde."),
    ReportScale => ("dB-R is an app-defined relative digital scale (0 = -90 dBFS; 60 = -30 dBFS). It is not dB SPL or clinical dB HL.", "dB-R ist eine von der App definierte relative digitale Skala (0 = -90 dBFS; 60 = -30 dBFS). Sie entspricht weder dB SPL noch klinischen dB HL."),
    Limitations => ("Important limitations", "Wichtige Einschränkungen"),
    ReportLimitations => ("AirPods, their fit and processing, system volume, and room noise affect results. The test is unmasked air-conduction screening and cannot diagnose a cause.", "AirPods, ihr Sitz und ihre Signalverarbeitung, die Systemlautstärke und Raumgeräusche beeinflussen die Ergebnisse. Der Test ist ein Luftleitungs-Screening ohne Vertäubung und kann keine Ursache diagnostizieren."),
    RepeatAdvice => ("Repeat an unexpected result under the same conditions and arrange a professional hearing assessment if you remain concerned.", "Wiederholen Sie einen unerwarteten Befund unter denselben Bedingungen. Vereinbaren Sie bei anhaltender Sorge eine professionelle Höruntersuchung."),
    ReportUrgent => ("Urgent: sudden hearing loss, rapid worsening, or hearing loss with earache/discharge needs prompt medical assessment; do not wait on this test.", "Dringend: Plötzlicher Hörverlust, rasche Verschlechterung oder Hörverlust mit Ohrenschmerzen oder Ausfluss erfordern eine umgehende ärztliche Untersuchung. Warten Sie nicht auf diesen Test."),
    ReportProtocol => ("Protocol: pulsed pure tones, 5 dB up / 10 dB down, 250-8000 Hz, maximum -30 dBFS. Generated by Hearing Tracker.", "Protokoll: pulsierende Sinustöne, 5 dB aufwärts / 10 dB abwärts, 250-8000 Hz, maximal -30 dBFS. Erstellt mit Hearing Tracker."),
    FrequencyHz => ("Frequency (Hz)", "Frequenz (Hz)"),
    Thresholds => ("Thresholds", "Hörschwellen"),
    Change => ("change*", "Änderung*"),
    MeanChange => ("*mean dB-R change", "*mittlere Änderung in dB-R"),
    StrongerTone => ("+ means stronger tone needed", "+ bedeutet: stärkerer Ton nötig"),
    Overview => ("Overview", "Übersicht"),
    Prepare => ("Prepare", "Vorbereiten"),
    Listen => ("Listen", "Hinhören"),
    Results => ("Results", "Ergebnisse"),
    PersonalCheck => ("YOUR PERSONAL LISTENING SPACE", "IHR PERSÖNLICHER HÖRCHECK"),
    CheckTime => ("12–18 min", "12–18 Min."),
    CheckTimeLabel => ("A little quiet time", "Ein wenig Ruhe"),
    CheckFrequencies => ("8 frequencies", "8 Frequenzen"),
    CheckFrequenciesLabel => ("Both ears, one check", "Beide Ohren, ein Check"),
    Offline => ("Always offline", "Immer offline"),
    OfflineLabel => ("Saved on this computer", "Auf diesem Computer gespeichert"),
    PersonalOnly => ("Personal tracking only. These checks are uncalibrated and cannot diagnose hearing loss or provide clinical dB HL values.", "Nur zur persönlichen Verlaufskontrolle. Diese Checks sind unkalibriert, können keinen Hörverlust diagnostizieren und liefern keine klinischen dB-HL-Werte."),
    FirstCheck => ("Your story starts here", "Hier beginnt Ihr Verlauf"),
    LatestCheck => ("LATEST CHECK", "LETZTER HÖRCHECK"),
    Headphones => ("Headphones", "Kopfhörer"),
    Room => ("Quiet space", "Ruhiger Platz"),
    Sound => ("Sound check", "Tonprüfung"),
    Ready => ("Ready", "Bereit"),
    StepCount => ("STEP {} OF 4", "SCHRITT {} VON 4"),
    Continue => ("Continue", "Weiter"),
    Back => ("Back", "Zurück"),
    ChooseHeadphonesHelp => ("Choose the stereo output your AirPods use. Keep the same headphones and system volume for every check.", "Wählen Sie die Stereo-Ausgabe Ihrer AirPods. Verwenden Sie für jeden Check dieselben Kopfhörer und dieselbe Systemlautstärke."),
    NoHeadphonesHelp => ("No stereo outputs found. Connect your headphones, then refresh the list.", "Keine Stereo-Ausgabe gefunden. Verbinden Sie Ihre Kopfhörer und aktualisieren Sie die Liste."),
    RoomHelp => ("A few small checks make your sessions easier to compare. Confirm each one to continue.", "Ein paar kurze Prüfungen machen Ihre Checks besser vergleichbar. Bestätigen Sie jeden Punkt, um fortzufahren."),
    FinishSetup => ("Complete this step to continue.", "Schließen Sie diesen Schritt ab, um fortzufahren."),
    ReadySummary => ("Headphones selected · quiet space confirmed · reference sound comfortable", "Kopfhörer ausgewählt · ruhiger Platz bestätigt · Referenzton angenehm"),
    FirstEar => ("Which ear should go first?", "Welches Ohr zuerst?"),
    ListeningCheck => ("LISTENING CHECK", "HÖRCHECK"),
    PausedHeading => ("A moment to pause.", "Ein Moment Pause."),
    PausedHelp => ("Take a breath. Continue when you're ready, keeping your original headphones and volume.", "Atmen Sie durch. Setzen Sie den Check fort, wenn Sie bereit sind, mit denselben Kopfhörern und derselben Lautstärke."),
    ListeningDuration => ("Usually 12–18 minutes · pause whenever you need", "Meist 12–18 Minuten · Pausen jederzeit möglich"),
    HearingChart => ("Your listening picture", "Ihr Hörprofil"),
    DetailedValues => ("Detailed measurements", "Detaillierte Messwerte"),
    SessionActions => ("Manage this check", "Diesen Check verwalten"),
}

impl Language {
    pub fn from_code(code: Option<&str>) -> Self {
        match code {
            Some("de") => Self::German,
            _ => Self::English,
        }
    }
    pub const fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::German => "de",
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::German => "Deutsch",
        }
    }
    pub fn format(self, key: Text, args: &[String]) -> String {
        // Substitute only catalog placeholders, never braces in user-provided values.
        let mut parts = self.text(key).split("{}");
        let mut result = parts.next().unwrap_or_default().to_owned();
        for (index, part) in parts.enumerate() {
            result.push_str(args.get(index).map(String::as_str).unwrap_or("{}"));
            result.push_str(part);
        }
        result
    }
    pub fn date(self, date: Date) -> String {
        match self {
            Self::English => format!(
                "{:04}-{:02}-{:02}",
                date.year(),
                u8::from(date.month()),
                date.day()
            ),
            Self::German => format!(
                "{:02}.{:02}.{:04}",
                date.day(),
                u8::from(date.month()),
                date.year()
            ),
        }
    }
    pub fn number(self, value: f32, decimals: usize) -> String {
        let value = format!("{value:.decimals$}");
        match self {
            Self::English => value,
            Self::German => value.replace('.', ","),
        }
    }
    pub fn status(self, status: SessionStatus) -> &'static str {
        self.text(match status {
            SessionStatus::InProgress => Text::InProgress,
            SessionStatus::Completed => Text::Completed,
            SessionStatus::Abandoned => Text::Abandoned,
        })
    }
    pub fn confidence(self, high: bool) -> &'static str {
        self.text(if high {
            Text::HighConfidence
        } else {
            Text::LowConfidence
        })
    }
    pub fn listening_mode(self, stored: &str) -> &str {
        if stored == "Noise cancellation" {
            self.text(Text::NoiseCancellation)
        } else {
            stored
        }
    }
    pub fn quality_reason(self, reason: &QualityReason) -> String {
        match reason {
            QualityReason::Issue(issue) => self.quality_issue(issue),
            QualityReason::Legacy(value) => match QualityIssue::from_legacy(value) {
                Some(issue) => self.quality_issue(&issue),
                None => value.clone(),
            },
        }
    }
    fn quality_issue(self, issue: &QualityIssue) -> String {
        match issue {
            QualityIssue::RepeatDifference { db } => {
                self.format(Text::RepeatDifference, &[self.number(*db, 0)])
            }
            QualityIssue::TooFewSilentChecks => self.text(Text::FewSilentChecks).into(),
            QualityIssue::SilentResponses { percent } => {
                self.format(Text::SilentResponses, &[self.number(*percent, 0)])
            }
            QualityIssue::VolumeUnavailable => self.text(Text::VolumeUnverified).into(),
        }
    }
}

/// Keep notifications translatable even after they have been opened.
#[derive(Debug, Clone)]
pub struct LocalizedText {
    key: Text,
    args: Vec<String>,
    cause: Option<Box<Self>>,
}
impl LocalizedText {
    pub fn with(key: Text, args: impl IntoIterator<Item = String>) -> Self {
        Self {
            key,
            args: args.into_iter().collect(),
            cause: None,
        }
    }
    pub fn because(key: Text, cause: Self) -> Self {
        Self {
            key,
            args: Vec::new(),
            cause: Some(Box::new(cause)),
        }
    }
    pub fn render(&self, language: Language) -> String {
        if let Some(cause) = &self.cause {
            language.format(self.key, &[cause.render(language)])
        } else {
            language.format(self.key, &self.args)
        }
    }
}
impl From<Text> for LocalizedText {
    fn from(key: Text) -> Self {
        Self {
            key,
            args: Vec::new(),
            cause: None,
        }
    }
}
impl From<AudioError> for LocalizedText {
    fn from(error: AudioError) -> Self {
        match error {
            AudioError::NoDevices => Text::NoDevices.into(),
            AudioError::DeviceMissing => Text::DeviceMissing.into(),
            AudioError::NotStereo => Text::NotStereo.into(),
            AudioError::UnsupportedFormat(format) => {
                Self::with(Text::UnsupportedFormat, [format.to_string()])
            }
            AudioError::Cpal(error) => Self::with(Text::AudioError, [error.to_string()]),
        }
    }
}
impl From<StorageError> for LocalizedText {
    fn from(error: StorageError) -> Self {
        match error {
            StorageError::NoDataDirectory => Text::NoDataDirectory.into(),
            StorageError::InvalidBaseline => Text::InvalidBaseline.into(),
            StorageError::Io(error) => Self::with(Text::FileError, [error.to_string()]),
            StorageError::Sql(error) => Self::with(Text::DatabaseError, [error.to_string()]),
            StorageError::Json(error) => Self::with(Text::InvalidData, [error.to_string()]),
        }
    }
}
impl From<ReportError> for LocalizedText {
    fn from(error: ReportError) -> Self {
        match error {
            ReportError::Io(error) => Self::with(Text::ReportError, [error.to_string()]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;
    #[test]
    fn catalog_is_complete_and_placeholders_match() {
        for key in ALL_TEXT {
            let en = Language::English.text(*key);
            let de = Language::German.text(*key);
            assert!(!en.is_empty() && !de.is_empty(), "{key:?}");
            assert_eq!(
                en.matches("{}").count(),
                de.matches("{}").count(),
                "{key:?}"
            );
        }
    }
    #[test]
    fn defaults_and_german_formats() {
        assert_eq!(Language::from_code(None), Language::English);
        assert_eq!(Language::from_code(Some("unknown")), Language::English);
        for language in [Language::English, Language::German] {
            assert_eq!(Language::from_code(Some(language.code())), language);
        }
        assert_eq!(Language::German.date(date!(2026 - 10 - 10)), "10.10.2026");
        assert_eq!(Language::English.date(date!(2026 - 10 - 10)), "2026-10-10");
        assert_eq!(Language::German.number(12.5, 1), "12,5");
        assert_eq!(Language::English.number(12.5, 1), "12.5");
    }
    #[test]
    fn pending_messages_change_language_and_preserve_user_values() {
        let message = LocalizedText::with(Text::ReportSaved, ["/tmp/{}-ä.pdf".into()]);
        assert_eq!(
            message.render(Language::English),
            "Report saved to /tmp/{}-ä.pdf"
        );
        assert_eq!(
            message.render(Language::German),
            "Bericht gespeichert unter /tmp/{}-ä.pdf"
        );
        assert_eq!(
            LocalizedText::from(AudioError::NotStereo).render(Language::German),
            Language::German.text(Text::NotStereo)
        );
    }
}
