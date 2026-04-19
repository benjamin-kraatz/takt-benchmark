use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use html_escape::encode_text;
use plotters::prelude::*;
use serde_json::json;

use crate::bench::{BenchmarkRunRecord, BenchmarkType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    Markdown,
    Html,
    Png,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::Markdown => "md",
            ExportFormat::Html => "html",
            ExportFormat::Png => "png",
        }
    }
}

pub fn export_runs_to_string(
    format: ExportFormat,
    title: &str,
    runs: &[BenchmarkRunRecord],
) -> Result<String> {
    match format {
        ExportFormat::Json => render_json(title, runs),
        ExportFormat::Markdown => render_markdown(title, runs),
        ExportFormat::Html => render_html(title, runs),
        ExportFormat::Png => bail!("PNG export must be written to a file path"),
    }
}

pub fn export_runs_to_path(
    format: ExportFormat,
    title: &str,
    runs: &[BenchmarkRunRecord],
    path: &Path,
) -> Result<()> {
    match format {
        ExportFormat::Json | ExportFormat::Markdown | ExportFormat::Html => {
            let content = export_runs_to_string(format, title, runs)?;
            fs::write(path, content)
                .with_context(|| format!("failed to write export file {}", path.display()))?;
        }
        ExportFormat::Png => render_png_chart(title, runs, path)?,
    }

    Ok(())
}

fn render_json(title: &str, runs: &[BenchmarkRunRecord]) -> Result<String> {
    serde_json::to_string_pretty(&json!({
        "title": title,
        "exported_at": Utc::now(),
        "run_count": runs.len(),
        "runs": runs,
    }))
    .context("failed to serialize JSON export")
}

fn render_markdown(title: &str, runs: &[BenchmarkRunRecord]) -> Result<String> {
    let mut markdown = String::new();
    markdown.push_str(&format!("# {title}\n\n"));
    markdown.push_str(&format!(
        "Exported at {}\n\n",
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
    ));
    markdown.push_str(&format!("Contains {} benchmark run(s).\n\n", runs.len()));

    for run in runs {
        markdown.push_str(&format!("## {}\n\n", run.display_name(),));
        markdown.push_str(&format!(
            "- Run ID: `{}`\n- Started: {}\n- Profile: {}\n- Target: {}\n- Filesystem: {}\n",
            run.run_id,
            run.started_at.format("%Y-%m-%d %H:%M:%S"),
            run.profile.preset,
            run.target.mount_point.display(),
            run.target.filesystem,
        ));
        if !run.tags.is_empty() {
            markdown.push_str(&format!("- Tags: {}\n", run.tags.join(", ")));
        }
        if let Some(notes) = &run.notes {
            markdown.push_str(&format!("- Notes: {}\n", notes));
        }
        markdown.push_str("\n| Benchmark | Avg MiB/s | Peak MiB/s | Min MiB/s | IOPS | P95 ms |\n");
        markdown.push_str("| --- | ---: | ---: | ---: | ---: | ---: |\n");
        for result in &run.results {
            markdown.push_str(&format!(
                "| {} | {:.1} | {:.1} | {:.1} | {} | {} |\n",
                result.benchmark.label(),
                result.average_mbps,
                result.peak_mbps,
                result.minimum_mbps,
                result
                    .iops
                    .map(|value| format!("{value:.0}"))
                    .unwrap_or_else(|| "-".to_string()),
                result
                    .latency_ms_p95
                    .map(|value| format!("{value:.2}"))
                    .unwrap_or_else(|| "-".to_string()),
            ));
        }
        markdown.push('\n');
    }

    Ok(markdown)
}

fn render_html(title: &str, runs: &[BenchmarkRunRecord]) -> Result<String> {
    let mut body = String::new();
    for run in runs {
        body.push_str(&format!(
            "<section class=\"run\"><h2>{}</h2><p><strong>Run ID:</strong> {}<br><strong>Started:</strong> {}<br><strong>Profile:</strong> {}<br><strong>Target:</strong> {}<br><strong>Filesystem:</strong> {}</p>",
            encode_text(&run.display_name()),
            encode_text(&run.run_id),
            run.started_at.format("%Y-%m-%d %H:%M:%S"),
            encode_text(&run.profile.preset.to_string()),
            encode_text(&run.target.mount_point.display().to_string()),
            encode_text(&run.target.filesystem),
        ));
        if !run.tags.is_empty() {
            body.push_str(&format!(
                "<p><strong>Tags:</strong> {}</p>",
                encode_text(&run.tags.join(", "))
            ));
        }
        if let Some(notes) = &run.notes {
            body.push_str(&format!(
                "<p><strong>Notes:</strong> {}</p>",
                encode_text(notes)
            ));
        }
        body.push_str("<table><thead><tr><th>Benchmark</th><th>Avg MiB/s</th><th>Peak MiB/s</th><th>Min MiB/s</th><th>IOPS</th><th>P95 ms</th></tr></thead><tbody>");
        for result in &run.results {
            body.push_str(&format!(
                "<tr><td>{}</td><td>{:.1}</td><td>{:.1}</td><td>{:.1}</td><td>{}</td><td>{}</td></tr>",
                encode_text(result.benchmark.label()),
                result.average_mbps,
                result.peak_mbps,
                result.minimum_mbps,
                result
                    .iops
                    .map(|value| format!("{value:.0}"))
                    .unwrap_or_else(|| "-".to_string()),
                result
                    .latency_ms_p95
                    .map(|value| format!("{value:.2}"))
                    .unwrap_or_else(|| "-".to_string()),
            ));
        }
        body.push_str("</tbody></table></section>");
    }

    Ok(format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title><style>body{{font-family:-apple-system,BlinkMacSystemFont,Segoe UI,sans-serif;margin:32px;background:#f5f2eb;color:#1e1c18;}}h1,h2{{font-weight:700;}}section.run{{background:white;border-radius:14px;padding:20px;margin-bottom:20px;box-shadow:0 8px 24px rgba(0,0,0,0.06);}}table{{width:100%;border-collapse:collapse;margin-top:12px;}}th,td{{padding:10px 12px;border-bottom:1px solid #e8e1d4;text-align:left;}}th{{background:#f4ede1;}}</style></head><body><h1>{}</h1><p>Exported at {}. Contains {} benchmark run(s).</p>{}</body></html>",
        encode_text(title),
        encode_text(title),
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        runs.len(),
        body
    ))
}

fn render_png_chart(title: &str, runs: &[BenchmarkRunRecord], path: &Path) -> Result<()> {
    if runs.is_empty() {
        bail!("cannot export PNG without any benchmark runs");
    }

    let root = BitMapBackend::new(path, (1280, 720)).into_drawing_area();
    root.fill(&RGBColor(245, 242, 235))?;

    let benchmarks = BenchmarkType::ALL;
    let max_y = runs
        .iter()
        .flat_map(|run| {
            run.results
                .iter()
                .map(|result| result.peak_mbps.max(result.average_mbps))
        })
        .fold(0.0_f64, f64::max)
        .max(1.0);

    let mut chart = ChartBuilder::on(&root)
        .caption(title, ("sans-serif", 32).into_font())
        .margin(20)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0_i32..(benchmarks.len() as i32 - 1), 0.0_f64..(max_y * 1.2))?;

    chart
        .configure_mesh()
        .x_labels(benchmarks.len())
        .x_label_formatter(&|index| {
            let idx = (*index).clamp(0, benchmarks.len() as i32 - 1) as usize;
            benchmarks[idx].label().to_string()
        })
        .y_desc("MiB/s")
        .x_desc("Benchmark")
        .light_line_style(WHITE.mix(0.3))
        .draw()?;

    let palette = [
        RGBColor(190, 83, 28),
        RGBColor(16, 110, 109),
        RGBColor(36, 74, 127),
        RGBColor(134, 93, 47),
        RGBColor(143, 41, 53),
    ];

    for (run_index, run) in runs.iter().enumerate() {
        let color = palette[run_index % palette.len()];
        let series_points = benchmarks
            .iter()
            .enumerate()
            .filter_map(|(index, benchmark)| {
                run.results
                    .iter()
                    .find(|result| result.benchmark == *benchmark)
                    .map(|result| (index as i32, result.average_mbps))
            })
            .collect::<Vec<_>>();

        chart
            .draw_series(LineSeries::new(
                series_points.clone(),
                color.stroke_width(3),
            ))?
            .label(run.series_label())
            .legend(move |(x, y)| {
                PathElement::new(vec![(x, y), (x + 20, y)], color.stroke_width(3))
            });
        chart.draw_series(
            series_points
                .into_iter()
                .map(|point| Circle::new(point, 5, color.filled())),
        )?;
    }

    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.85))
        .border_style(BLACK)
        .draw()?;

    root.present().context("failed to finalize PNG export")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{ExportFormat, export_runs_to_path, export_runs_to_string};
    use crate::bench::{
        BenchmarkProfile, BenchmarkResult, BenchmarkRunRecord, BenchmarkType, SamplePoint,
    };
    use crate::device::{DeviceKind, DeviceMetadata, DeviceTarget};

    fn sample_run() -> BenchmarkRunRecord {
        BenchmarkRunRecord {
            run_id: "run-123".to_string(),
            started_at: chrono::Utc::now(),
            finished_at: chrono::Utc::now(),
            target: DeviceTarget {
                id: "/tmp".to_string(),
                name: "tmp".to_string(),
                mount_point: "/tmp".into(),
                source: "/dev/disk1s1".to_string(),
                filesystem: "apfs".to_string(),
                kind: DeviceKind::BuiltIn,
                total_bytes: 1024,
                available_bytes: 512,
                metadata: DeviceMetadata::default(),
            },
            profile: BenchmarkProfile::balanced(),
            tags: vec!["baseline".to_string()],
            notes: Some("note".to_string()),
            results: vec![BenchmarkResult {
                benchmark: BenchmarkType::SequentialWrite,
                bytes_processed: 1024,
                duration_secs: 1.0,
                average_mbps: 8.0,
                peak_mbps: 9.0,
                minimum_mbps: 7.0,
                iops: None,
                latency_ms_p50: None,
                latency_ms_p95: None,
                samples: vec![SamplePoint {
                    seconds: 1.0,
                    throughput_mbps: 8.0,
                }],
            }],
        }
    }

    #[test]
    fn renders_textual_exports() {
        let runs = vec![sample_run()];
        let json = export_runs_to_string(ExportFormat::Json, "export", &runs).expect("json export");
        let markdown = export_runs_to_string(ExportFormat::Markdown, "export", &runs)
            .expect("markdown export");
        let html = export_runs_to_string(ExportFormat::Html, "export", &runs).expect("html export");

        assert!(json.contains("run-123"));
        assert!(markdown.contains("# export"));
        assert!(html.contains("<!doctype html>"));
    }

    #[test]
    fn renders_png_export() {
        let temp_dir = tempdir().expect("tempdir");
        let output = temp_dir.path().join("report.png");

        export_runs_to_path(ExportFormat::Png, "chart", &[sample_run()], &output)
            .expect("png export");

        assert!(output.exists());
    }
}
