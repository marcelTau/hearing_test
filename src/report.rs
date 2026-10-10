use crate::domain::{Ear, FREQUENCIES_HZ, TestSession, ThresholdOutcome, compare_sessions};
use crate::localization::{Language, Text};
use printpdf::{
    BuiltinFont, Color, Line, LineDashPattern, LinePoint, Mm, Op, PaintMode, PdfDocument,
    PdfFontHandle, PdfPage, PdfSaveOptions, Point, Polygon, PolygonRing, Pt, Rgb, TextItem,
    WindingOrder,
};
use std::fs;
use std::path::Path;
use thiserror::Error;

const PAGE_WIDTH_MM: f32 = 297.0;
const PAGE_HEIGHT_MM: f32 = 210.0;
const PLOT_LEFT: f32 = 56.0;
const PLOT_RIGHT: f32 = 545.0;
const PLOT_TOP: f32 = 430.0;
const PLOT_BOTTOM: f32 = 205.0;

#[derive(Debug, Error)]
pub enum ReportError {
    #[error("could not write the report: {0}")]
    Io(#[from] std::io::Error),
}

pub fn save_pdf(
    path: &Path,
    session: &TestSession,
    baseline: Option<&TestSession>,
    display_name: Option<&str>,
    language: Language,
) -> Result<(), ReportError> {
    fs::write(
        path,
        generate_pdf(session, baseline, display_name, language),
    )?;
    Ok(())
}

pub fn generate_pdf(
    session: &TestSession,
    baseline: Option<&TestSession>,
    display_name: Option<&str>,
    language: Language,
) -> Vec<u8> {
    let baseline = baseline.filter(|candidate| session.is_comparable_with(candidate));
    let mut ops = Vec::new();
    text(
        &mut ops,
        42.0,
        555.0,
        19.0,
        language.text(Text::ReportTitle),
    );
    text(
        &mut ops,
        42.0,
        535.0,
        10.0,
        language.text(Text::ReportSubtitle),
    );
    let date = session.completed_at.unwrap_or(session.started_at).date();
    let identity = language.format(
        Text::ReportIdentity,
        &[
            display_name
                .filter(|name| !name.trim().is_empty())
                .unwrap_or(language.text(Text::NotProvided))
                .to_owned(),
            language.date(date),
            session.id.to_string(),
        ],
    );
    fitted_text(&mut ops, 42.0, 514.0, 9.0, 758.0, &identity);
    let output = language.format(
        Text::ReportOutput,
        &[
            session.setup.device_name.clone(),
            session.setup.sample_rate.to_string(),
            session
                .setup
                .sink_volume_percent
                .map(|value| format!("{}%", language.number(value, 0)))
                .unwrap_or_else(|| language.text(Text::Unavailable).into()),
            language
                .listening_mode(&session.setup.listening_mode)
                .to_owned(),
        ],
    );
    fitted_text(&mut ops, 42.0, 500.0, 8.0, 758.0, &output);
    let quality = language.format(
        Text::ReportQuality,
        &[
            session
                .quality
                .as_ref()
                .map(|quality| language.confidence(quality.high_confidence))
                .unwrap_or(language.text(Text::NotAssessed))
                .to_owned(),
            baseline
                .map(|base| language.format(Text::ReportBaseline, &[base.id.to_string()]))
                .unwrap_or_default(),
        ],
    );
    fitted_text(&mut ops, 42.0, 486.0, 8.0, 758.0, &quality);

    draw_grid(&mut ops, language);
    if let Some(base) = baseline {
        draw_session_curves(&mut ops, base, true);
    }
    draw_session_curves(&mut ops, session, false);
    draw_table(&mut ops, session, baseline, language);

    // Flow explanatory paragraphs instead of assuming English fits on one line.
    let mut y = 178.0;
    for (key, size, gap) in [
        (Text::ReadReport, 9.0, 4.0),
        (Text::ReportLegend, 8.0, 2.0),
        (Text::ReportScale, 8.0, 6.0),
        (Text::Limitations, 9.0, 4.0),
        (Text::ReportLimitations, 8.0, 2.0),
        (Text::RepeatAdvice, 8.0, 6.0),
        (Text::ReportUrgent, 8.0, 6.0),
        (Text::ReportProtocol, 7.0, 0.0),
    ] {
        y = paragraph(&mut ops, 42.0, y, size, 758.0, language.text(key)) - gap;
    }

    let page = PdfPage::new(Mm(PAGE_WIDTH_MM), Mm(PAGE_HEIGHT_MM), ops);
    let mut document = PdfDocument::new(language.text(Text::ReportTitle));
    let mut warnings = Vec::new();
    document
        .with_pages(vec![page])
        .save(&PdfSaveOptions::default(), &mut warnings)
}

fn draw_grid(ops: &mut Vec<Op>, language: Language) {
    let grey = rgb(0.78, 0.80, 0.83);
    ops.push(Op::SetOutlineColor { col: grey });
    ops.push(Op::SetOutlineThickness { pt: Pt(0.55) });
    for (index, frequency) in FREQUENCIES_HZ.into_iter().enumerate() {
        let x = frequency_x(index);
        line(ops, x, PLOT_BOTTOM, x, PLOT_TOP);
        text(
            ops,
            x - 9.0,
            PLOT_TOP + 9.0,
            7.0,
            &frequency_label(frequency),
        );
    }
    for level in (0..=60).step_by(10) {
        let y = level_y(level as f32);
        line(ops, PLOT_LEFT, y, PLOT_RIGHT, y);
        text(ops, PLOT_LEFT - 24.0, y - 2.5, 7.0, &level.to_string());
    }
    text(ops, PLOT_LEFT - 35.0, PLOT_TOP + 25.0, 7.5, "dB-R");
    text(
        ops,
        PLOT_LEFT + 185.0,
        PLOT_TOP + 25.0,
        7.5,
        language.text(Text::FrequencyHz),
    );
}

fn draw_session_curves(ops: &mut Vec<Op>, session: &TestSession, baseline: bool) {
    if baseline {
        ops.push(Op::SetLineDashPattern {
            dash: LineDashPattern::new(0.0, &[5.0, 4.0]),
        });
    } else {
        ops.push(Op::SetLineDashPattern {
            dash: LineDashPattern::default(),
        });
    }
    for ear in [Ear::Right, Ear::Left] {
        let colour = if baseline {
            rgb(0.50, 0.52, 0.55)
        } else if ear == Ear::Right {
            rgb(0.82, 0.12, 0.16)
        } else {
            rgb(0.08, 0.32, 0.78)
        };
        ops.push(Op::SetOutlineColor { col: colour });
        ops.push(Op::SetOutlineThickness {
            pt: Pt(if baseline { 1.0 } else { 1.7 }),
        });
        let mut previous = None;
        for (index, frequency) in FREQUENCIES_HZ.into_iter().enumerate() {
            let threshold = session.threshold(ear, frequency);
            let point = threshold
                .and_then(|threshold| threshold.relative_db())
                .map(|level| (frequency_x(index), level_y(level)));
            if let (Some((x1, y1)), Some((x2, y2))) = (previous, point) {
                line(ops, x1, y1, x2, y2);
            }
            previous = point;
            if let Some((x, y)) = point {
                if !baseline {
                    draw_symbol(ops, ear, x, y);
                }
            } else if let Some(threshold) = threshold {
                let x = frequency_x(index);
                match threshold.outcome {
                    ThresholdOutcome::BelowFloor => {
                        let x = x + if ear == Ear::Right { -5.0 } else { 5.0 };
                        let y = PLOT_TOP - 1.0;
                        line(ops, x, y - 15.0, x, y);
                        line(ops, x, y, x - 4.0, y - 5.0);
                        line(ops, x, y, x + 4.0, y - 5.0);
                    }
                    ThresholdOutcome::NoResponse if !baseline => {
                        draw_no_response(ops, ear, x, PLOT_BOTTOM + 3.0);
                    }
                    _ => {}
                }
            }
        }
    }
    ops.push(Op::SetLineDashPattern {
        dash: LineDashPattern::default(),
    });
}

fn draw_table(
    ops: &mut Vec<Op>,
    session: &TestSession,
    baseline: Option<&TestSession>,
    language: Language,
) {
    let x = 575.0;
    text(ops, x, 448.0, 9.0, language.text(Text::Thresholds));
    for (offset, label) in [
        (0.0, "Hz"),
        (47.0, "R"),
        (84.0, "L"),
        (121.0, language.text(Text::Change)),
    ] {
        text(ops, x + offset, 432.0, 7.0, label);
    }
    let deltas = baseline.and_then(|base| compare_sessions(session, base));
    for (row, frequency) in FREQUENCIES_HZ.into_iter().enumerate() {
        let y = 415.0 - row as f32 * 22.0;
        let right = threshold_text(session, Ear::Right, frequency, language);
        let left = threshold_text(session, Ear::Left, frequency, language);
        let change = deltas
            .as_ref()
            .map(|values| {
                let values: Vec<_> = values
                    .iter()
                    .filter(|value| value.frequency_hz == frequency)
                    .filter_map(|value| value.change_db)
                    .collect();
                if values.is_empty() {
                    "--".to_owned()
                } else {
                    format!("{:+.0}", values.iter().sum::<f32>() / values.len() as f32)
                }
            })
            .unwrap_or_else(|| "--".to_owned());
        for (offset, value) in [
            (0.0, frequency.to_string()),
            (47.0, right),
            (84.0, left),
            (121.0, change),
        ] {
            text(ops, x + offset, y, 8.0, &value);
        }
    }
    text(ops, x, 227.0, 6.5, language.text(Text::MeanChange));
    text(ops, x, 216.0, 6.5, language.text(Text::StrongerTone));
    paragraph(
        ops,
        x,
        201.0,
        6.5,
        220.0,
        language.text(Text::ReportRangeHelp),
    );
}

fn threshold_text(session: &TestSession, ear: Ear, frequency: u32, language: Language) -> String {
    match session.threshold(ear, frequency) {
        Some(threshold) if threshold.outcome == ThresholdOutcome::BelowFloor => "<=0".into(),
        Some(threshold) if threshold.outcome == ThresholdOutcome::NoResponse => {
            language.text(Text::NoResponse).into()
        }
        Some(threshold) => threshold
            .relative_db()
            .map(|value| format!("{value:.0}"))
            .unwrap_or_else(|| "--".into()),
        None => "--".into(),
    }
}

fn draw_symbol(ops: &mut Vec<Op>, ear: Ear, x: f32, y: f32) {
    match ear {
        Ear::Right => {
            let points = (0..16)
                .map(|index| {
                    let angle = std::f32::consts::TAU * index as f32 / 16.0;
                    LinePoint {
                        p: Point {
                            x: Pt(x + angle.cos() * 4.2),
                            y: Pt(y + angle.sin() * 4.2),
                        },
                        bezier: false,
                    }
                })
                .collect();
            ops.push(Op::DrawPolygon {
                polygon: Polygon {
                    rings: vec![PolygonRing { points }],
                    mode: PaintMode::Stroke,
                    winding_order: WindingOrder::NonZero,
                },
            });
        }
        Ear::Left => {
            line(ops, x - 4.0, y - 4.0, x + 4.0, y + 4.0);
            line(ops, x - 4.0, y + 4.0, x + 4.0, y - 4.0);
        }
    }
}

fn draw_no_response(ops: &mut Vec<Op>, ear: Ear, x: f32, y: f32) {
    draw_symbol(ops, ear, x, y + 7.0);
    line(ops, x, y + 2.0, x, y - 4.0);
    line(ops, x, y - 4.0, x - 3.0, y - 1.0);
    line(ops, x, y - 4.0, x + 3.0, y - 1.0);
}

fn text_width(value: &str, size: f32) -> f32 {
    static FONT: std::sync::OnceLock<printpdf::ParsedFont> = std::sync::OnceLock::new();
    let font = FONT.get_or_init(|| {
        BuiltinFont::Helvetica
            .get_parsed_font()
            .expect("bundled Helvetica font")
    });
    value
        .chars()
        .map(|character| {
            font.lookup_glyph_index(character as u32)
                .and_then(|glyph| font.get_glyph_width(glyph))
                .map(|width| width as f32 / font.units_per_em as f32)
                .unwrap_or(1.0)
                * size
        })
        .sum()
}

fn fitted_text(ops: &mut Vec<Op>, x: f32, y: f32, size: f32, width: f32, value: &str) {
    let actual_width = text_width(value, size);
    let size = if actual_width > width {
        size * width / actual_width
    } else {
        size
    };
    text(ops, x, y, size, value);
}

fn wrapped_lines(value: &str, size: f32, width: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in value.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_owned()
        } else {
            format!("{current} {word}")
        };
        if !current.is_empty() && text_width(&candidate, size) > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn paragraph(ops: &mut Vec<Op>, x: f32, mut y: f32, size: f32, width: f32, value: &str) -> f32 {
    for line in wrapped_lines(value, size, width) {
        fitted_text(ops, x, y, size, width, &line);
        y -= size * 1.4;
    }
    y
}

fn text(ops: &mut Vec<Op>, x: f32, y: f32, size: f32, value: &str) {
    ops.extend([
        Op::StartTextSection,
        Op::SetTextCursor {
            pos: Point { x: Pt(x), y: Pt(y) },
        },
        Op::SetFont {
            font: PdfFontHandle::Builtin(BuiltinFont::Helvetica),
            size: Pt(size),
        },
        Op::ShowText {
            items: vec![TextItem::Text(value.to_owned())],
        },
        Op::EndTextSection,
    ]);
}

fn line(ops: &mut Vec<Op>, x1: f32, y1: f32, x2: f32, y2: f32) {
    ops.push(Op::DrawLine {
        line: Line {
            points: vec![
                LinePoint {
                    p: Point {
                        x: Pt(x1),
                        y: Pt(y1),
                    },
                    bezier: false,
                },
                LinePoint {
                    p: Point {
                        x: Pt(x2),
                        y: Pt(y2),
                    },
                    bezier: false,
                },
            ],
            is_closed: false,
        },
    });
}

fn rgb(r: f32, g: f32, b: f32) -> Color {
    Color::Rgb(Rgb {
        r,
        g,
        b,
        icc_profile: None,
    })
}

fn frequency_x(index: usize) -> f32 {
    PLOT_LEFT + index as f32 * (PLOT_RIGHT - PLOT_LEFT) / 7.0
}

fn level_y(level: f32) -> f32 {
    PLOT_TOP - level.clamp(0.0, 60.0) / 60.0 * (PLOT_TOP - PLOT_BOTTOM)
}

fn frequency_label(frequency: u32) -> String {
    if frequency >= 1_000 {
        format!("{}k", frequency / 1_000)
    } else {
        frequency.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{
        AudioSetup, CEILING_DBFS, PROTOCOL_VERSION, QualityAssessment, SessionStatus,
    };

    fn sample_session() -> TestSession {
        use crate::domain::Threshold;
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
        session.started_at = time::macros::datetime!(2026-10-10 12:00 UTC);
        session.status = SessionStatus::Completed;
        session.completed_at = Some(session.started_at);
        session.quality = Some(QualityAssessment {
            high_confidence: true,
            catch_trials: 6,
            false_positive_catches: 0,
            max_retest_difference_db: 0.0,
            reasons: vec![],
        });
        for ear in [Ear::Right, Ear::Left] {
            for (index, frequency_hz) in FREQUENCIES_HZ.into_iter().enumerate() {
                let no_response = ear == Ear::Left && index == 7;
                session.thresholds.push(Threshold {
                    ear,
                    frequency_hz,
                    dbfs: if no_response {
                        None
                    } else {
                        Some(-65.0 + index as f32 * 5.0)
                    },
                    outcome: if no_response {
                        ThresholdOutcome::NoResponse
                    } else {
                        ThresholdOutcome::Measured
                    },
                });
            }
        }
        session
    }

    #[test]
    fn bilingual_reports_preserve_results_and_german_characters() {
        let session = sample_session();
        let mut baseline = session.clone();
        baseline.id = uuid::Uuid::now_v7();
        for threshold in &mut baseline.thresholds {
            threshold.dbfs = threshold.dbfs.map(|value| value - 5.0);
        }
        let original = serde_json::to_string(&session).unwrap();
        for language in [Language::English, Language::German] {
            let bytes = generate_pdf(&session, Some(&baseline), Some("Jörg Weiß"), language);
            assert!(bytes.starts_with(b"%PDF-"));
            let parsed = PdfDocument::parse(
                &bytes,
                &printpdf::PdfParseOptions::default(),
                &mut Vec::new(),
            )
            .unwrap();
            let text = parsed
                .extract_text()
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            assert!(text.contains(language.text(Text::ReportTitle)), "{text}");
            assert!(text.contains("Jörg Weiß"), "{text}");
            assert!(
                text.contains(&language.date(session.started_at.date())),
                "{text}"
            );
            assert!(
                text.contains(language.text(Text::NoiseCancellation)),
                "{text}"
            );
            assert!(text.contains(language.text(Text::HighConfidence)), "{text}");
            assert!(text.contains(language.text(Text::NoResponse)), "{text}");
            if language == Language::German {
                assert!(!text.contains("Noise cancellation"));
                assert!(!text.contains("High confidence"));
                assert!(text.contains("ärztliche"), "{text}");
            }
            assert_eq!(serde_json::to_string(&session).unwrap(), original);
            if let Ok(directory) = std::env::var("HEARING_TEST_QA_DIR") {
                std::fs::create_dir_all(&directory).unwrap();
                std::fs::write(
                    Path::new(&directory).join(format!("report-{}.pdf", language.code())),
                    &bytes,
                )
                .unwrap();
            }
        }
    }

    #[test]
    fn translated_report_text_stays_inside_page() {
        for language in [Language::English, Language::German] {
            let bytes = generate_pdf(&sample_session(), None, None, language);
            let parsed = PdfDocument::parse(
                &bytes,
                &printpdf::PdfParseOptions::default(),
                &mut Vec::new(),
            )
            .unwrap();
            let (mut x, mut y, mut size) = (0.0, 0.0, 0.0);
            for op in &parsed.pages[0].ops {
                match op {
                    Op::SetTextCursor { pos } => {
                        x = pos.x.0;
                        y = pos.y.0;
                    }
                    Op::SetFont {
                        size: font_size, ..
                    } => size = font_size.0,
                    Op::ShowText { items } => {
                        for item in items {
                            if let TextItem::Text(value) = item {
                                assert!(y > 20.0 && y < 575.0, "{value}: y={y}");
                                assert!(
                                    x + text_width(value, size) <= 810.0,
                                    "{value}: x={x}, size={size}"
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    #[test]
    fn reports_label_limits_and_omit_their_numeric_changes() {
        let mut session = sample_session();
        for threshold in &mut session.thresholds {
            if threshold.frequency_hz == 250 {
                threshold.outcome = ThresholdOutcome::BelowFloor;
                threshold.dbfs = Some(crate::domain::FLOOR_DBFS);
            }
        }
        let mut baseline = sample_session();
        baseline.id = uuid::Uuid::now_v7();
        for language in [Language::English, Language::German] {
            assert_eq!(threshold_text(&session, Ear::Right, 250, language), "<=0");
            assert_eq!(threshold_text(&session, Ear::Left, 250, language), "<=0");
            let deltas = compare_sessions(&session, &baseline).unwrap();
            assert!(
                deltas
                    .iter()
                    .filter(|delta| delta.frequency_hz == 250)
                    .all(|delta| delta.change_db.is_none())
            );
            let bytes = generate_pdf(&session, Some(&baseline), None, language);
            let parsed = PdfDocument::parse(
                &bytes,
                &printpdf::PdfParseOptions::default(),
                &mut Vec::new(),
            )
            .unwrap();
            let text = parsed
                .extract_text()
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ");
            assert!(text.contains("<=0"), "{text}");
            assert!(text.contains("--"), "{text}");
            if let Ok(directory) = std::env::var("HEARING_TEST_QA_DIR") {
                std::fs::create_dir_all(&directory).unwrap();
                std::fs::write(
                    Path::new(&directory).join(format!("report-limits-{}.pdf", language.code())),
                    &bytes,
                )
                .unwrap();
            }
        }
    }
}
