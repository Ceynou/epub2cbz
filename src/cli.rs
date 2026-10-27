use epub2cbz::settings::Settings;
use clap::Parser;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

#[derive(Parser)]
#[command(name = "epub2cbz", about = "Convert EPUB manga/comics to CBZ")]
pub struct Cli {
    /// Input EPUB file(s) or directory
    #[arg(required = true)]
    pub input: Vec<PathBuf>,

    /// Output directory (defaults to input directory)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Process directories recursively
    #[arg(short, long)]
    pub recursive: bool,

    /// Simple extraction (no processing)
    #[arg(short, long)]
    pub simple: bool,

    /// Split double-page spreads
    #[arg(long)]
    pub split_spreads: Option<bool>,

    /// Crop image borders
    #[arg(long)]
    pub crop: Option<bool>,

    /// Resize images to WxH (e.g. 1264x1680)
    #[arg(long)]
    pub resize: Option<String>,

    /// Generate ComicInfo.xml
    #[arg(long)]
    pub comic_info: Option<bool>,

    /// Page spread alignment
    #[arg(long)]
    pub page_spread: Option<bool>,

    /// Output format: jpg, png, webp
    #[arg(long)]
    pub format: Option<String>,

    /// Parallel jobs (default: CPU count)
    #[arg(short, long)]
    pub jobs: Option<usize>,

    /// Quiet mode (no progress output)
    #[arg(short, long)]
    pub quiet: bool,
}

pub fn run_cli() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut settings = Settings::load();

    if cli.simple {
        settings.simple_extraction = true;
    }
    if let Some(split) = cli.split_spreads {
        settings.split_page_spread = split;
    }
    if let Some(crop) = cli.crop {
        settings.crop_images = crop;
    }
    if let Some(ref resize) = cli.resize {
        if let Some((w, h)) = parse_dimensions(resize) {
            settings.resize_images = true;
            settings.resize_width = w;
            settings.resize_height = h;
        }
    }
    if let Some(ci) = cli.comic_info {
        settings.comic_info = ci;
    }
    if let Some(ps) = cli.page_spread {
        settings.page_spread = ps;
    }
    if let Some(ref fmt) = cli.format {
        match fmt.to_lowercase().as_str() {
            "jpg" | "jpeg" => {
                settings.output_format_jpg = true;
                settings.output_format_webp = false;
            }
            "png" => {
                settings.output_format_jpg = false;
                settings.output_format_webp = false;
            }
            "webp" => {
                settings.output_format_webp = true;
                settings.output_format_jpg = false;
            }
            _ => {}
        }
    }
    if let Some(jobs) = cli.jobs {
        settings.parallelism = jobs;
    }

    let mut epub_files: Vec<PathBuf> = Vec::new();
    for input in &cli.input {
        epub_files.extend(epub2cbz::find_epub_files(input, cli.recursive));
    }

    if epub_files.is_empty() {
        anyhow::bail!("No EPUB files found");
    }

    let total = epub_files.len();
    let success_count = AtomicUsize::new(0);
    let fail_count = AtomicUsize::new(0);
    let start = Instant::now();

    let mp = MultiProgress::new();
    let overall = if !cli.quiet && total > 1 {
        let pb = mp.add(ProgressBar::new(total as u64));
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/dim}] {pos}/{len} files ({eta})")
                .unwrap()
                .progress_chars("=> "),
        );
        Some(pb)
    } else {
        None
    };

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(settings.max_parallelism())
        .build()?;

    let success_ref = &success_count;
    let fail_ref = &fail_count;
    let mp_ref = &mp;

    pool.scope(|s| {
        for epub_path in &epub_files {
            let settings = &settings;
            let quiet = cli.quiet;
            let overall = overall.as_ref();
            let output_dir = cli
                .output
                .clone()
                .unwrap_or_else(|| {
                    epub_path
                        .parent()
                        .unwrap_or(std::path::Path::new("."))
                        .to_path_buf()
                });

            s.spawn(move |_| {
                let fname = epub_path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();

                let pb = if !quiet {
                    let pb = mp_ref.add(ProgressBar::new_spinner());
                    pb.set_style(
                        ProgressStyle::default_spinner()
                            .template("  {spinner:.green} {msg}")
                            .unwrap(),
                    );
                    pb.set_message(fname.clone());
                    pb.enable_steady_tick(std::time::Duration::from_millis(80));
                    Some(pb)
                } else {
                    None
                };

                let log_fn = |_msg: &str| {};

                match epub2cbz::process_epub(epub_path, &output_dir, settings, &log_fn) {
                    Ok(result) => {
                        if result.success {
                            success_ref.fetch_add(1, Ordering::Relaxed);
                            if let Some(pb) = &pb {
                                pb.finish_with_message(format!("{fname} ✓"));
                            }
                        } else {
                            fail_ref.fetch_add(1, Ordering::Relaxed);
                            if let Some(pb) = &pb {
                                pb.finish_with_message(format!(
                                    "{fname} ✗ {}",
                                    result.message
                                ));
                            }
                        }
                    }
                    Err(e) => {
                        fail_ref.fetch_add(1, Ordering::Relaxed);
                        if let Some(pb) = &pb {
                            pb.finish_with_message(format!("{fname} ✗ {e}"));
                        }
                    }
                }

                if let Some(overall) = overall {
                    overall.inc(1);
                }
            });
        }
    });

    if let Some(overall) = &overall {
        overall.finish_and_clear();
    }

    let elapsed = start.elapsed();
    let ok = success_count.load(Ordering::Relaxed);
    let fail = fail_count.load(Ordering::Relaxed);
    if !cli.quiet {
        eprintln!(
            "\n{ok} succeeded, {fail} failed out of {total} in {:.1}s",
            elapsed.as_secs_f64()
        );
    }

    if fail > 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn parse_dimensions(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() == 2 {
        let w = parts[0].parse().ok()?;
        let h = parts[1].parse().ok()?;
        Some((w, h))
    } else {
        None
    }
}
