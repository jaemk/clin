use std;
use std::env;
use std::fs;
use std::io::{self, Write, BufRead};
use std::path;
use std::cmp;

use reqwest;
use serde_json;
use tempdir;
use flate2;
use tar;
use errors::*;

pub static BIN_NAME: &'static str = "clin";
pub static CURRENT_VERSION: &'static str = crate_version!();
pub static API_URL: &'static str = "https://api.github.com/repos/jaemk/clin/releases/latest";


macro_rules! print_flush {
    ($literal:expr) => {
        print!($literal);
        io::stdout().flush()?;
    };
    ($literal:expr, $($arg:expr),*) => {
        print!($literal, $($arg),*);
        io::stdout().flush()?;
    }
}


/// Determine the current target triple
/// Since we know what binary releases we make available, we can make some assumptions
/// e.g. only `gnu` available for linux
///
/// Errors:
///     * Unexpected system config
fn get_target() -> Result<String> {
    let arch_config = (cfg!(target_arch = "x86"), cfg!(target_arch = "x86_64"));
    let arch = match arch_config {
        (true, _) => "i686",
        (_, true) => "x86_64",
        _ => bail!(Error::Upgrade, "Unable to determine target-architecture"),
    };

    let os_config = (cfg!(target_os = "macos"), cfg!(target_os = "linux"));
    let os = match os_config {
        (true, _) => "apple-darwin",
        (_, true) => "unknown-linux-gnu",
        _ => bail!(Error::Upgrade, "Unable to determine target-os"),
    };

    Ok(format!("{}-{}", arch, os))
}


#[derive(Debug)]
struct ReleaseAsset {
    download_url: String,
    name: String,
}
impl ReleaseAsset {
    /// Parse a release-asset json object
    ///
    /// Errors:
    ///     * Missing required name & download-url keys
    fn from_asset(asset: &serde_json::Value) -> Result<ReleaseAsset> {
        let download_url = asset["browser_download_url"].as_str()
            .ok_or_else(|| format_err!(Error::Upgrade, "Asset missing `browser_download_url`"))?;
        let name = asset["name"].as_str()
            .ok_or_else(|| format_err!(Error::Upgrade, "Asset missing `name`"))?;
        Ok(ReleaseAsset {
            download_url: download_url.to_owned(),
            name: name.to_owned(),
        })
    }
}


/// Flush a message to stdout and check if they respond `yes`
///
/// Errors:
///     * Io flushing
///     * User entered anything other than Y/y
fn prompt_ok(msg: &str) -> Result<()> {
    print_flush!("{}", msg);
    io::stdout().flush()?;

    let stdin = io::stdin();
    let mut s = String::new();
    stdin.read_line(&mut s)?;
    if s.trim().to_lowercase() != "y" {
        bail!(Error::Upgrade, "Upgrade aborted");
    }
    Ok(())
}


/// Display a download progress bar, returning the size of the
/// bar that needs to be cleared on the next run
///
/// Errors:
///     * Io flushing
fn display_dl_progress(total_size: u64, bytes_read: u64, clear_size: usize) -> Result<usize> {
    let bar_width = 75;
    let ratio = bytes_read as f64 / total_size as f64;
    let percent = (ratio * 100.) as u8;
    let n_complete = (bar_width as f64 * ratio) as usize;
    let mut complete_bars = std::iter::repeat("=").take(n_complete).collect::<String>();
    if ratio != 1. { complete_bars.push('>'); }

    let clear_chars = std::iter::repeat("\x08").take(clear_size).collect::<String>();
    print_flush!("{}\r", clear_chars);
    io::stdout().flush()?;

    let bar = format!("{percent: >3}% [{compl: <full_size$}] {total}kB", percent=percent, compl=complete_bars, full_size=bar_width, total=total_size/1000);
    print_flush!("{}", bar);
    io::stdout().flush()?;

    Ok(bar.len())
}


/// Download the file behind the given `url` into the specified `dest`.
/// Show a sliding progress bar if specified.
/// If the resource doesn't specify a content-length, the progress bar will not be shown
///
/// Errors:
///     * `reqwest` network errors
///     * Unsuccessful response status
///     * Progress-bar errors
///     * Reading from response to `BufReader`-buffer
///     * Writing from `BufReader`-buffer to `File`
fn download_to_file_with_progress<T: io::Write>(url: &str, mut dest: T, mut show_progress: bool) -> Result<()> {
    let resp = reqwest::get(url)?;
    let size = resp.headers()
        .get::<reqwest::header::ContentLength>()
        .map(|ct_len| **ct_len)
        .unwrap_or(0);
    if !resp.status().is_success() { bail!(Error::Upgrade, "Download request failed with status: {:?}", resp.status()) }
    if size == 0 { show_progress = false; }

    let mut bytes_read = 0;
    let mut clear_size = 0;
    let mut src = io::BufReader::new(resp);
    loop {
        if show_progress {
            clear_size = display_dl_progress(size, bytes_read as u64, clear_size)?;
        }
        let n = {
            let mut buf = src.fill_buf()?;
            dest.write_all(&mut buf)?;
            buf.len()
        };
        if n == 0 { break; }
        src.consume(n);
        bytes_read += n;
    }
    if show_progress { println!(" ✓"); }
    Ok(())
}


/// Extract contents of a tar.gz file to a specified directory, returning the
/// temp path to our new executable
///
/// Errors:
///     * Io - opening files
///     * Io - gzip decoding
///     * Io - archive unpacking
fn extract_tarball(tarball: &path::Path, dir: &path::Path) -> Result<path::PathBuf> {
    let tarball = fs::File::open(tarball)?;
    let tar = flate2::read::GzDecoder::new(tarball)?;
    let mut archive = tar::Archive::new(tar);
    archive.unpack(dir)?;
    Ok(dir.join(BIN_NAME))
}


/// Copy existing executable to a temp directory and try putting our new one in its place.
/// If something goes wrong, copy the original executable back
///
/// Errors:
///     * Io - copying / renaming
fn replace_exe(current_exe: &path::Path, new_exe: &path::Path, tmp_file: &path::Path) -> Result<()> {
    fs::copy(current_exe, tmp_file)?;
    match fs::rename(new_exe, current_exe) {
        Err(_) => {
            fs::copy(tmp_file, current_exe)?;
        }
        Ok(_) => (),
    };
    Ok(())
}


/// Upgrade the current binary to the latest release
pub fn run(show_progress: bool) -> Result<()> {
    let current_exe = env::current_exe()?;

    print_flush!("Checking target-arch... ");
    io::stdout().flush()?;
    let target = get_target()?;
    println!("{}", target);

    println!("Checking current version... v{}", CURRENT_VERSION);

    print_flush!("Checking latest released version... ");
    io::stdout().flush()?;
    let mut resp = reqwest::get(API_URL)?;
    if !resp.status().is_success() { bail!(Error::Upgrade, "api request failed with status: {:?}", resp.status()) }
    let latest: serde_json::Value = resp.json()?;
    let latest_tag = latest["tag_name"].as_str()
        .ok_or_else(|| format_err!(Error::Upgrade, "No tag_name found for latest release"))?
        .trim_left_matches("v");
    println!("v{}", latest_tag);

    if latest_tag.cmp(CURRENT_VERSION) != cmp::Ordering::Greater {
        println!("Already up to date! -- v{}", CURRENT_VERSION);
        return Ok(())
    }

    println!("New release found! v{} --> v{}", CURRENT_VERSION, latest_tag);

    let latest_assets = latest["assets"].as_array().ok_or_else(|| format_err!(Error::Upgrade, "No release assets found!"))?;
    let target_asset = latest_assets.iter().map(ReleaseAsset::from_asset).collect::<Result<Vec<ReleaseAsset>>>();
    let target_asset = target_asset?.into_iter()
        .filter(|ra| ra.name.contains(&target))
        .nth(0)
        .ok_or_else(|| format_err!(Error::Upgrade, "No release asset found for current target: `{}`", target))?;

    println!("\nclin release status:");
    println!("  * Current exe: {:?}", current_exe);
    println!("  * New exe tarball: {:?}", target_asset.name);
    println!("  * New exe download url: {:?}", target_asset.download_url);
    println!("\nThe new release will be downloaded/extracted and the existing binary will be replaced.");
    prompt_ok("Do you want to continue? [Y/n] ")?;

    let tmp_dir = tempdir::TempDir::new(&format!("{}-download", BIN_NAME))?;
    let tmp_tarball_path = tmp_dir.path().join(&target_asset.name);
    let mut tmp_tarball = fs::File::create(&tmp_tarball_path)?;

    println!("Downloading...");
    download_to_file_with_progress(&target_asset.download_url, &mut tmp_tarball, show_progress)?;

    print_flush!("Extracting tarball... ");
    io::stdout().flush()?;
    let new_exe = extract_tarball(&tmp_tarball_path, &tmp_dir.path())?;
    println!("✓");

    print_flush!("Replacing binary file... ");
    io::stdout().flush()?;
    let tmp_file = tmp_dir.path().join(&format!("__{}_backup", BIN_NAME));
    replace_exe(&current_exe, &new_exe, &tmp_file)?;
    println!("✓");

    println!("Complete!");
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn can_determine_target_arch() {
        let target = get_target();
        assert!(target.is_ok(), "{:?}", target);
    }
}

