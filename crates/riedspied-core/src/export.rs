use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::Utc;
use html_escape::encode_text;
use plotters::coord::Shift;
use plotters::prelude::*;
use serde_json::json;

use crate::bench::{BenchmarkRunRecord, BenchmarkType, SamplePoint};

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

    pub fn label(self) -> &'static str {
        match self {
            ExportFormat::Json => "JSON",
            ExportFormat::Markdown => "Markdown",
            ExportFormat::Html => "HTML",
            ExportFormat::Png => "PNG",
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

    let root = BitMapBackend::new(path, (1600, 1100)).into_drawing_area();
    root.fill(&RGBColor(245, 242, 235))?;

    let mode = png_report_mode(runs);
    let (header_area, content_area) = root.split_vertically(130);
    render_png_header(&header_area, title, runs, mode)?;

    match mode {
        PngReportMode::SingleRun => render_single_run_panels(&content_area, &runs[0])?,
        PngReportMode::Comparison => render_comparison_panels(&content_area, runs)?,
        PngReportMode::Trend => render_trend_panels(&content_area, runs)?,
    }

    root.present().context("failed to finalize PNG export")?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PngReportMode {
    SingleRun,
    Comparison,
    Trend,
}

fn png_report_mode(runs: &[BenchmarkRunRecord]) -> PngReportMode {
    if runs.len() == 1 {
        PngReportMode::SingleRun
    } else if runs.len() >= 3 && same_device_runs(runs) {
        PngReportMode::Trend
    } else {
        PngReportMode::Comparison
    }
}

fn same_device_runs(runs: &[BenchmarkRunRecord]) -> bool {
    runs.first()
        .map(|first| runs.iter().all(|run| run.target.id == first.target.id))
        .unwrap_or(false)
}

fn render_png_header(
    area: &DrawingArea<BitMapBackend<'_>, Shift>,
    title: &str,
    runs: &[BenchmarkRunRecord],
    mode: PngReportMode,
) -> Result<()> {
    area.fill(&RGBColor(245, 242, 235))?;
    area.draw(&Text::new(
        title.to_string(),
        (24, 32),
        ("sans-serif", 28).into_font().color(&BLACK),
    ))?;
    area.draw(&Text::new(
        format!("Exported at {}", Utc::now().format("%Y-%m-%d %H:%M:%S UTC")),
        (24, 62),
        ("sans-serif", 16).into_font().color(&RGBColor(90, 84, 76)),
    ))?;

    let mode_label = match mode {
        PngReportMode::SingleRun => "Single-run detail report",
        PngReportMode::Comparison => "Direct comparison report",
        PngReportMode::Trend => "Same-device trend report",
    };
    area.draw(&Text::new(
        mode_label.to_string(),
        (24, 90),
        ("sans-serif", 16).into_font().color(&RGBColor(118, 74, 30)),
    ))?;

    for (index, run) in runs.iter().take(4).enumerate() {
        area.draw(&Text::new(
            run.series_label(),
            (420, 32 + (index as i32 * 24)),
            ("sans-serif", 15).into_font().color(&RGBColor(58, 55, 51)),
        ))?;
    }

    if runs.len() > 4 {
        area.draw(&Text::new(
            format!("... and {} more run(s)", runs.len() - 4),
            (420, 32 + (4 * 24)),
            ("sans-serif", 15).into_font().color(&RGBColor(58, 55, 51)),
        ))?;
    }

    Ok(())
}

fn render_single_run_panels(
    area: &DrawingArea<BitMapBackend<'_>, Shift>,
    run: &BenchmarkRunRecord,
) -> Result<()> {
    let panels = area.split_evenly((2, 2));
    for (panel, benchmark) in panels.into_iter().zip(BenchmarkType::ALL) {
        render_single_panel(panel, run, benchmark)?;
    }
    Ok(())
}

fn render_comparison_panels(
    area: &DrawingArea<BitMapBackend<'_>, Shift>,
    runs: &[BenchmarkRunRecord],
) -> Result<()> {
    let panels = area.split_evenly((2, 2));
    for (panel, benchmark) in panels.into_iter().zip(BenchmarkType::ALL) {
        render_comparison_panel(panel, runs, benchmark)?;
    }
    Ok(())
}

fn render_trend_panels(
    area: &DrawingArea<BitMapBackend<'_>, Shift>,
    runs: &[BenchmarkRunRecord],
) -> Result<()> {
    let panels = area.split_evenly((2, 2));
    for (panel, benchmark) in panels.into_iter().zip(BenchmarkType::ALL) {
        render_trend_panel(panel, runs, benchmark)?;
    }
    Ok(())
}

fn render_single_panel<'a>(
    area: DrawingArea<BitMapBackend<'a>, Shift>,
    run: &BenchmarkRunRecord,
    benchmark: BenchmarkType,
) -> Result<()> {
    let (metrics_area, chart_area) = prepare_panel(area, benchmark)?;
    let Some(result) = run.result_for(benchmark) else {
        metrics_area.draw(&Text::new(
            "No data captured".to_string(),
            (12, 24),
            ("sans-serif", 14)
                .into_font()
                .color(&RGBColor(120, 114, 105)),
        ))?;
        return Ok(());
    };

    draw_metric_lines(
        &metrics_area,
        &[
            format!(
                "Avg {:.1} MiB/s   Peak {:.1}   Min {:.1}",
                result.average_mbps, result.peak_mbps, result.minimum_mbps
            ),
            format_optional_metrics(result.iops, result.latency_ms_p95),
        ],
    )?;

    let points = time_series_points(result);
    let max_x = points
        .last()
        .map(|(seconds, _)| *seconds)
        .unwrap_or(1.0)
        .max(1.0);
    let max_y = points
        .iter()
        .map(|(_, throughput)| *throughput)
        .fold(result.peak_mbps.max(result.average_mbps), f64::max)
        .max(1.0);

    let mut chart = ChartBuilder::on(&chart_area)
        .margin(12)
        .x_label_area_size(30)
        .y_label_area_size(45)
        .build_cartesian_2d(0.0_f64..max_x, 0.0_f64..(max_y * 1.15))?;
    chart
        .configure_mesh()
        .x_desc("Seconds")
        .y_desc("MiB/s")
        .light_line_style(WHITE.mix(0.25))
        .draw()?;
    chart.draw_series(LineSeries::new(points.clone(), &RGBColor(16, 110, 109)))?;
    chart.draw_series(
        points
            .into_iter()
            .step_by(12)
            .map(|point| Circle::new(point, 3, RGBColor(16, 110, 109).filled())),
    )?;
    Ok(())
}

fn render_comparison_panel<'a>(
    area: DrawingArea<BitMapBackend<'a>, Shift>,
    runs: &[BenchmarkRunRecord],
    benchmark: BenchmarkType,
) -> Result<()> {
    let (metrics_area, chart_area) = prepare_panel(area, benchmark)?;
    let palette = [
        RGBColor(190, 83, 28),
        RGBColor(16, 110, 109),
        RGBColor(36, 74, 127),
        RGBColor(134, 93, 47),
    ];

    let mut metric_lines = Vec::new();
    let mut max_x = 1.0_f64;
    let mut max_y = 1.0_f64;
    let mut series = Vec::new();

    for (index, run) in runs.iter().enumerate() {
        if let Some(result) = run.result_for(benchmark) {
            let points = time_series_points(result);
            max_x = max_x.max(points.last().map(|(seconds, _)| *seconds).unwrap_or(1.0));
            max_y = max_y.max(
                points
                    .iter()
                    .map(|(_, throughput)| *throughput)
                    .fold(result.peak_mbps.max(result.average_mbps), f64::max),
            );
            metric_lines.push(format!(
                "{}  avg {:.1} MiB/s",
                run.series_label(),
                result.average_mbps
            ));
            series.push((run.series_label(), palette[index % palette.len()], points));
        }
    }

    if metric_lines.is_empty() {
        draw_metric_lines(&metrics_area, &["No data captured".to_string()])?;
        return Ok(());
    }

    draw_metric_lines(&metrics_area, &metric_lines)?;

    let mut chart = ChartBuilder::on(&chart_area)
        .margin(12)
        .x_label_area_size(30)
        .y_label_area_size(45)
        .build_cartesian_2d(0.0_f64..max_x.max(1.0), 0.0_f64..(max_y * 1.15))?;
    chart
        .configure_mesh()
        .x_desc("Seconds")
        .y_desc("MiB/s")
        .light_line_style(WHITE.mix(0.25))
        .draw()?;

    for (label, color, points) in series {
        chart
            .draw_series(LineSeries::new(points.clone(), color.stroke_width(3)))?
            .label(label)
            .legend(move |(x, y)| {
                PathElement::new(vec![(x, y), (x + 18, y)], color.stroke_width(3))
            });
        chart.draw_series(
            points
                .into_iter()
                .step_by(14)
                .map(|point| Circle::new(point, 3, color.filled())),
        )?;
    }
    chart
        .configure_series_labels()
        .background_style(WHITE.mix(0.85))
        .border_style(BLACK)
        .draw()?;
    Ok(())
}

fn render_trend_panel<'a>(
    area: DrawingArea<BitMapBackend<'a>, Shift>,
    runs: &[BenchmarkRunRecord],
    benchmark: BenchmarkType,
) -> Result<()> {
    let (metrics_area, chart_area) = prepare_panel(area, benchmark)?;
    let mut points = Vec::new();
    for (index, run) in runs.iter().enumerate() {
        if let Some(result) = run.result_for(benchmark) {
            points.push((index as i32, result.average_mbps));
        }
    }

    if points.is_empty() {
        draw_metric_lines(&metrics_area, &["No data captured".to_string()])?;
        return Ok(());
    }

    let latest = points.last().map(|(_, value)| *value).unwrap_or_default();
    draw_metric_lines(
        &metrics_area,
        &[
            format!("{} run(s) for {}", runs.len(), runs[0].target.name),
            format!("Latest average {:.1} MiB/s", latest),
        ],
    )?;

    let max_y = points
        .iter()
        .map(|(_, value)| *value)
        .fold(1.0_f64, f64::max);
    let mut chart = ChartBuilder::on(&chart_area)
        .margin(12)
        .x_label_area_size(36)
        .y_label_area_size(45)
        .build_cartesian_2d(
            0_i32..((runs.len() as i32).max(1) - 1),
            0.0_f64..(max_y * 1.15),
        )?;
    chart
        .configure_mesh()
        .x_desc("Run order")
        .y_desc("Avg MiB/s")
        .x_labels(runs.len().min(6))
        .x_label_formatter(&|index| {
            runs.get((*index).max(0) as usize)
                .map(|run| run.started_at.format("%m-%d").to_string())
                .unwrap_or_default()
        })
        .light_line_style(WHITE.mix(0.25))
        .draw()?;
    chart.draw_series(LineSeries::new(points.clone(), &RGBColor(36, 74, 127)))?;
    chart.draw_series(
        points
            .into_iter()
            .map(|point| Circle::new(point, 4, RGBColor(36, 74, 127).filled())),
    )?;
    Ok(())
}

fn prepare_panel<'a>(
    area: DrawingArea<BitMapBackend<'a>, Shift>,
    benchmark: BenchmarkType,
) -> Result<(
    DrawingArea<BitMapBackend<'a>, Shift>,
    DrawingArea<BitMapBackend<'a>, Shift>,
)> {
    area.fill(&WHITE)?;
    let (title_area, chart_area) = area.split_vertically(62);
    title_area.draw(&Text::new(
        benchmark.label().to_string(),
        (12, 22),
        ("sans-serif", 20).into_font().color(&BLACK),
    ))?;
    Ok((title_area, chart_area))
}

fn draw_metric_lines(area: &DrawingArea<BitMapBackend<'_>, Shift>, lines: &[String]) -> Result<()> {
    for (index, line) in lines.iter().enumerate() {
        area.draw(&Text::new(
            line.to_string(),
            (12, 44 + (index as i32 * 16)),
            ("sans-serif", 13).into_font().color(&RGBColor(90, 84, 76)),
        ))?;
    }
    Ok(())
}

fn format_optional_metrics(iops: Option<f64>, latency_ms_p95: Option<f64>) -> String {
    let mut parts = Vec::new();
    if let Some(iops) = iops {
        parts.push(format!("IOPS {:.0}", iops));
    }
    if let Some(latency_ms_p95) = latency_ms_p95 {
        parts.push(format!("P95 {:.2} ms", latency_ms_p95));
    }
    if parts.is_empty() {
        "No latency or IOPS metrics".to_string()
    } else {
        parts.join("   ")
    }
}

fn time_series_points(result: &crate::bench::BenchmarkResult) -> Vec<(f64, f64)> {
    if result.samples.is_empty() {
        let duration = result.duration_secs.max(1.0);
        return vec![(0.0, result.average_mbps), (duration, result.average_mbps)];
    }

    downsample_samples(&result.samples, 180)
        .into_iter()
        .map(|sample| (sample.seconds, sample.throughput_mbps))
        .collect()
}

fn downsample_samples(samples: &[SamplePoint], max_points: usize) -> Vec<SamplePoint> {
    if samples.len() <= max_points {
        return samples.to_vec();
    }

    let bucket_size = samples.len().div_ceil(max_points);
    samples
        .chunks(bucket_size)
        .map(|chunk| SamplePoint {
            seconds: chunk.iter().map(|sample| sample.seconds).sum::<f64>() / chunk.len() as f64,
            throughput_mbps: chunk
                .iter()
                .map(|sample| sample.throughput_mbps)
                .sum::<f64>()
                / chunk.len() as f64,
        })
        .collect()
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
            results: BenchmarkType::ALL
                .iter()
                .enumerate()
                .map(|(index, benchmark)| BenchmarkResult {
                    benchmark: *benchmark,
                    bytes_processed: 1024,
                    duration_secs: 4.0,
                    average_mbps: 8.0 + index as f64,
                    peak_mbps: 9.0 + index as f64,
                    minimum_mbps: 7.0 + index as f64,
                    iops: (*benchmark == BenchmarkType::RandomIops).then_some(1500.0),
                    latency_ms_p50: (*benchmark == BenchmarkType::RandomIops).then_some(1.1),
                    latency_ms_p95: (*benchmark == BenchmarkType::RandomIops).then_some(2.2),
                    samples: vec![
                        SamplePoint {
                            seconds: 1.0,
                            throughput_mbps: 8.0 + index as f64,
                        },
                        SamplePoint {
                            seconds: 2.0,
                            throughput_mbps: 9.0 + index as f64,
                        },
                        SamplePoint {
                            seconds: 3.0,
                            throughput_mbps: 7.5 + index as f64,
                        },
                    ],
                })
                .collect(),
        }
    }

    fn sample_run_with_index(index: usize) -> BenchmarkRunRecord {
        let mut run = sample_run();
        run.run_id = format!("run-{index}");
        run.started_at = chrono::Utc::now() + chrono::TimeDelta::seconds(index as i64 * 60);
        run.tags = vec![format!("tag-{index}")];
        for result in &mut run.results {
            result.average_mbps += index as f64 * 3.0;
            result.peak_mbps += index as f64 * 3.0;
            result.minimum_mbps += index as f64 * 2.0;
            for sample in &mut result.samples {
                sample.throughput_mbps += index as f64 * 2.0;
            }
        }
        run
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

    #[test]
    fn renders_comparison_png_export() {
        let temp_dir = tempdir().expect("tempdir");
        let output = temp_dir.path().join("comparison.png");

        export_runs_to_path(
            ExportFormat::Png,
            "comparison",
            &[sample_run_with_index(0), sample_run_with_index(1)],
            &output,
        )
        .expect("comparison png export");

        assert!(output.exists());
    }

    #[test]
    fn renders_trend_png_export() {
        let temp_dir = tempdir().expect("tempdir");
        let output = temp_dir.path().join("trend.png");

        export_runs_to_path(
            ExportFormat::Png,
            "trend",
            &[
                sample_run_with_index(0),
                sample_run_with_index(1),
                sample_run_with_index(2),
            ],
            &output,
        )
        .expect("trend png export");

        assert!(output.exists());
    }
}
