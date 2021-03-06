extern crate docopt;
extern crate env_logger;
#[macro_use]
extern crate log;

#[macro_use]
extern crate serde_derive;

extern crate html5ever;
extern crate url;

extern crate cargo_edit;
extern crate num_cpus;
extern crate rayon;
extern crate reqwest;
extern crate walkdir;

mod check;
mod parse;

use std::path::{Path, PathBuf};
use std::process;

use log::LogLevelFilter;
use env_logger::LogBuilder;
use docopt::Docopt;

use cargo_edit::Manifest;
use rayon::ThreadPoolBuilder;
use walkdir::{DirEntry, WalkDir};

use check::check_urls;
use parse::parse_html_file;

const MAIN_USAGE: &'static str = "
Check your package's documentation for dead links.

Usage:
    cargo deadlinks [--dir <directory>] [options]

Options:
    -h --help               Print this message
    --dir                   Specify a directory to check (default is target/doc/<package>)
    --debug                 Use debug output
    -v --verbose            Use verbose output
    -V --version            Print version info and exit.
";

#[derive(Debug, Deserialize)]
struct MainArgs {
    arg_directory: Option<String>,
    flag_verbose: bool,
    flag_debug: bool,
}

fn main() {
    let args: MainArgs = Docopt::new(MAIN_USAGE)
        .and_then(|d| {
            d.version(Some(env!("CARGO_PKG_VERSION").to_owned()))
                .deserialize()
        })
        .unwrap_or_else(|e| e.exit());

    init_logger(&args);

    let dir = args.arg_directory
        .map_or_else(determine_dir, |dir| PathBuf::from(dir));
    let dir = dir.canonicalize().unwrap();
    if !walk_dir(&dir) {
        process::exit(1);
    }
}

/// Initalizes the logger according to the provided config flags.
fn init_logger(args: &MainArgs) {
    let mut builder = LogBuilder::new();
    builder.format(|record| format!("{}", record.args()));
    match (args.flag_debug, args.flag_verbose) {
        (true, _) => {
            builder.filter(Some("cargo_deadlinks"), LogLevelFilter::Debug);
        }
        (false, true) => {
            builder.filter(Some("cargo_deadlinks"), LogLevelFilter::Info);
        }
        (false, false) => {
            builder.filter(Some("cargo_deadlinks"), LogLevelFilter::Error);
        }
    }
    builder.init().unwrap();
}

/// Returns the directory to use as root of the documentation.
///
/// If an directory has been provided as CLI argument that one is used.
/// Otherwise we try to find the `Cargo.toml` and construct the documentation path
/// from the package name found there.
///
/// All *.html files under the root directory will be checked.
fn determine_dir() -> PathBuf {
    match Manifest::open(&None) {
        Ok(manifest) => {
            let package_name = manifest
                .data
                .get("package")
                .unwrap()
                .as_table()
                .unwrap()
                .get("name")
                .unwrap()
                .as_str()
                .unwrap();
            let package_name = package_name.replace("-", "_");

            Path::new("target").join("doc").join(package_name)
        }
        Err(err) => {
            debug!("Error: {}", err);
            error!("Could not find a Cargo.toml.");
            ::std::process::exit(1);
        }
    }
}

fn is_html_file(entry: &DirEntry) -> bool {
    match entry.path().extension() {
        Some(e) => e.to_str().map(|ext| ext == "html").unwrap_or(false),
        None => false,
    }
}

/// Traverses a given path recursively, checking all *.html files found.
fn walk_dir(dir_path: &Path) -> bool {
    let pool = ThreadPoolBuilder::new()
        .num_threads(num_cpus::get())
        .build()
        .unwrap();
    let mut result = true;

    for entry in WalkDir::new(dir_path).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && is_html_file(&entry) {
            let urls = parse_html_file(entry.path());
            let success = pool.install(|| check_urls(&urls));
            result &= success;
        }
    }

    result
}
