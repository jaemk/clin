use std;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path;

use reqwest;
use serde_json;
use tempdir;
use flate2;
use tar;
use errors::*;

pub static CURRENT_VERSION: &'static str = ""; //crate_version!();
pub static API_URL: &'static str = "https://api.github.com/repos/jaemk/clin/releases/latest";


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


fn prompt(msg: &str) -> Result<()> {
    use std::io::Write;

    print!("{}", msg);
    let mut stdout = io::stdout();
    stdout.flush()?;

    let stdin = io::stdin();
    let mut s = String::new();
    stdin.read_line(&mut s)?;
    if s.trim().to_lowercase() != "y" {
        bail!(Error::Upgrade, "Upgrade aborted");
    }
    Ok(())
}


fn display_dl_progress(total_size: u64, bytes_read: u64, clear_size: usize) -> Result<usize> {
    let bar_width = 75;
    let ratio = bytes_read as f64 / total_size as f64;
    let percent = (ratio * 100.) as u8;
    let complete = bar_width as f64 * ratio;
    let complete = complete as usize;
    let mut complete = std::iter::repeat("=").take(complete).collect::<String>();
    if ratio != 1. { complete.push('>'); }

    //let mut stdout = io::stdout();
    let clear = std::iter::repeat("\x08").take(clear_size).collect::<String>();
    print!("{}\r", clear);
    io::stdout().flush()?;

    let bar = format!("{percent: >3}% [{compl: <full_size$}] {total}kB", percent=percent, compl=complete, full_size=bar_width, total=total_size/1000);
    print!("{}", bar);
    io::stdout().flush()?;

    Ok(bar.len())
}


fn download_to_file_with_progress<T: io::Read, U: io::Write>(mut src: T, mut dest: U, size: u64, show_progress: bool) -> Result<()> {
    let mut buf = vec![0; 4096];
    let mut bytes_read = 0;
    let mut clear_size = 0;
    loop {
        buf.resize(4096, 0);  // make sure buf is full size before reading
        if show_progress {
            clear_size = display_dl_progress(size, bytes_read as u64, clear_size)?;
        }
        let n = src.read(&mut buf)?;
        if n == 0 { break; }
        bytes_read += n;
        buf.truncate(n);     // read doesn't always fill the entire buf, truncate before writing
        dest.write_all(&mut buf)?;
    }
    if show_progress { println!(" ✓"); }
    Ok(())
}


fn extract_tarball(tarball: &path::Path, dir: &path::Path) -> Result<path::PathBuf> {
    let tarball = fs::File::open(tarball)?;
    let tar = flate2::read::GzDecoder::new(tarball)?;
    let mut archive = tar::Archive::new(tar);
    archive.unpack(dir)?;
    Ok(dir.join("clin"))
}


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
    let target = get_target()?;

    let mut resp = reqwest::get(API_URL)?;
    if !resp.status().is_success() { bail!(Error::Upgrade, "api request failed with status: {:?}", resp.status()) }

    let latest: serde_json::Value = resp.json()?;

    let latest_tag = latest["tag_name"].as_str()
        .ok_or_else(|| format_err!(Error::Upgrade, "No tag_name found for latest release"))?
        .trim_left_matches("v");
    if CURRENT_VERSION == latest_tag {
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
    println!("  * Current executable: {:?}", current_exe);
    println!("  * New executable tarball: {:?}", target_asset.name);
    println!("  * New executable download url: {:?}", target_asset.download_url);
    println!("\nThe following operations will be executed:");
    println!("  - Download/extract new release");
    println!("  - Overwrite current executable with new release");
    prompt("Do you want to continue? [Y/n] ")?;

    let tmp_dir = tempdir::TempDir::new("clin-download")?;
    let tmp_tarball_path = tmp_dir.path().join(&target_asset.name);
    let mut tmp_tarball = fs::File::create(&tmp_tarball_path)?;

    println!("Downloading...");
    let mut resp = reqwest::get(&target_asset.download_url)?;
    let content_length = resp.headers()
        .get::<reqwest::header::ContentLength>()
        .map(|ct_len| **ct_len)
        .unwrap_or(0);
    if !resp.status().is_success() { bail!(Error::Upgrade, "Download request failed with status: {:?}", resp.status()) }
    download_to_file_with_progress(&mut resp, &mut tmp_tarball, content_length, show_progress)?;

    print!("Extracting tarball to temp-dir...");
    io::stdout().flush()?;
    let new_exe = extract_tarball(&tmp_tarball_path, &tmp_dir.path())?;
    println!(" ✓");

    print!("Replacing binary file...");
    io::stdout().flush()?;
    let tmp_file = tmp_dir.path().join("__clin_backup");
    replace_exe(&current_exe, &new_exe, &tmp_file)?;
    println!(" ✓");

    println!("Complete!");
    Ok(())
}

