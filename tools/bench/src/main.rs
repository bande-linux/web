mod compare;
mod load;
mod server;
mod stages;

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: bench stages [--pages a,b] [DIR] | bench compare [--pages a,b] [--browsers bande,firefox,chromium] [--rounds N] [DIR] | bench load URL WARM_ROUNDS";
pub const WIDTH: u16 = 1280;
pub const HEIGHT: u16 = 800;
pub const SCROLL_FRAMES: usize = 120;
pub const SCROLL_STEP: f32 = 200.0;

pub struct Options {
    pub directory: PathBuf,
    pub pages: Vec<String>,
    pub browsers: Vec<String>,
    pub rounds: usize,
}

fn options(arguments: &[String]) -> Option<Options> {
    let mut options = Options {
        directory: PathBuf::from(".bench-pages"),
        pages: Vec::new(),
        browsers: vec!["bande".into(), "firefox".into(), "chromium".into()],
        rounds: 5,
    };
    let split = |value: &str| value.split(',').map(str::to_owned).collect::<Vec<String>>();
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--pages" => options.pages = split(rest.next()?),
            "--browsers" => options.browsers = split(rest.next()?),
            "--rounds" => options.rounds = rest.next()?.parse().ok()?,
            directory => options.directory = PathBuf::from(directory),
        }
    }
    if options.pages.is_empty() {
        options.pages = all_pages(&options.directory);
    }
    Some(options)
}

fn all_pages(directory: &std::path::Path) -> Vec<String> {
    let mut pages: Vec<String> = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().join("index.html").exists())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    pages.sort();
    pages
}

pub fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or(f64::NAN)
}

pub fn percentile(values: &mut [f64], fraction: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    let last = values.len().saturating_sub(1);
    let index = (last as f64 * fraction).round() as usize;
    values.get(index).copied().unwrap_or(f64::NAN)
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let result = match arguments.split_first() {
        Some((command, rest)) if command == "load" => load::child(rest),
        Some((command, rest)) => match (command.as_str(), options(rest)) {
            ("stages", Some(options)) => stages::run(&options),
            ("compare", Some(options)) => compare::run(&options),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(2);
            }
        },
        None => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bench: {error}");
            ExitCode::FAILURE
        }
    }
}
