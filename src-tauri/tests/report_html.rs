use std::path::PathBuf;

use pulse::report::{generate_html_report, generate_markdown_report};

fn html() -> String {
    generate_html_report(Some(30), None)
}

fn sample_html_path() -> PathBuf {
    std::env::temp_dir().join("pulse-report-sample.html")
}

#[test]
fn writes_rendered_sample_for_visual_review() {
    let report = html();
    let path = sample_html_path();
    std::fs::write(&path, &report).expect("write sample report");
    assert!(
        path.exists(),
        "sample report should be written for /browser review"
    );
    eprintln!("pulse sample report written to {}", path.display());
}

#[test]
fn html_report_has_no_remote_font_or_asset_references() {
    let report = html();
    assert!(
        !report.contains("fonts.googleapis.com"),
        "report must not pull Google Fonts (offline regression guard)"
    );
    assert!(
        !report.contains("fonts.gstatic.com"),
        "report must not pull gstatic font binaries"
    );
    assert!(
        !report.contains("@import url("),
        "report must not @import any remote stylesheet"
    );
    assert!(
        !report.contains("http://"),
        "report must not reference any http:// asset"
    );
    for occurrence in report.match_indices("https://") {
        let (idx, _) = occurrence;
        let tail = &report[idx..(idx + 120).min(report.len())];
        let is_allowed_namespace =
            tail.starts_with("https://www.w3.org") || tail.starts_with("https://github.com");
        assert!(
            is_allowed_namespace,
            "unexpected https reference in report (must be offline-safe): {tail}"
        );
    }
}

#[test]
fn html_report_is_well_formed_with_style_and_charts() {
    let report = html();
    assert!(!report.trim().is_empty(), "report must be non-empty");
    assert!(
        report.starts_with("<!DOCTYPE html>"),
        "report must open with doctype"
    );
    assert_eq!(
        report.matches("<html").count(),
        1,
        "exactly one <html> open tag"
    );
    assert_eq!(
        report.matches("</html>").count(),
        1,
        "exactly one </html> close tag"
    );
    assert!(
        report.contains("<style>") && report.contains("</style>"),
        "report must inline its CSS"
    );
    assert!(report.contains("</body>"), "report must close its body");
    assert!(
        report.contains("<svg"),
        "report must render inline SVG charts"
    );
    assert!(
        report.contains(r#"aria-label="Token composition""#),
        "token composition chart must be present"
    );
}

#[test]
fn html_report_contains_brand_kpi_and_sections() {
    let report = html();
    assert!(report.contains("Pulse"), "brand kicker present");
    assert!(report.contains("Analytics Report"), "report title present");
    assert!(
        report.contains(r#"class="summary-grid""#),
        "KPI strip present"
    );
    assert!(
        report.contains(r#"class="summary-card""#),
        "KPI cards present"
    );
    for anchor in [
        r#"id="cache""#,
        r#"id="routing""#,
        r#"id="inflections""#,
        r#"id="sessions""#,
        r#"id="tools""#,
        r#"id="topology""#,
        r#"id="prompts""#,
        r#"id="recommendations""#,
    ] {
        assert!(
            report.contains(anchor),
            "missing analyzer section: {anchor}"
        );
    }
    assert!(
        report.contains("Speed Split"),
        "speed split surfacing present"
    );
    assert!(
        report.contains(r#"class="speed-split""#),
        "speed split layout present"
    );
}

#[test]
fn html_report_uses_offline_system_font_stack() {
    let report = html();
    assert!(
        report.contains("-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Inter, sans-serif"),
        "body must use the self-contained system UI font stack"
    );
    assert!(
        report.contains("'JetBrains Mono', 'SF Mono', 'Cascadia Code', Consolas, monospace"),
        "monospace must use a self-contained stack"
    );
}

#[test]
fn markdown_report_is_valid_non_empty_gfm_with_sections() {
    let md = generate_markdown_report(Some(30), None);
    assert!(!md.trim().is_empty(), "markdown report must be non-empty");
    assert!(
        md.contains("# Pulse Analytics Report"),
        "top-level heading present"
    );
    for heading in [
        "## Executive Summary",
        "## Cache",
        "## Routing",
        "### Speed Split",
        "## Inflections",
        "## Sessions",
        "## Tools",
        "## Telemetry Topology",
        "## Prompts",
    ] {
        assert!(md.contains(heading), "missing markdown section: {heading}");
    }
    assert!(
        md.contains("| Tier | Sessions | Cost | Share |"),
        "speed split table header present"
    );
    assert!(
        !md.contains("fonts.googleapis.com"),
        "markdown must not embed remote font references"
    );
}
