#[macro_use]
extern crate serde_derive;

#[macro_use]
mod errors;
mod listen;

use clap::{crate_version, App, AppSettings, Arg, ArgMatches, SubCommand};
use notify_rust::{Notification, Timeout};

use std::env;
use std::ffi;
use std::io;
use std::net;
use std::process;

use errors::*;

pub static APP_VERSION: &'static str = crate_version!();
pub static DEFAULT_TITLE: &'static str = "CLIN:";
pub static DEFAULT_MESSAGE: &'static str = "clin!";
pub static DEFAULT_ICON: &'static str = "terminal";
pub static DEFAULT_HOST: &'static str = "127.0.0.1";
pub static DEFAULT_PORT_STR: &'static str = "6445";
pub static DEFAULT_PORT: u32 = 6445;
pub static DEFAULT_TIMEOUT_STR: &'static str = "10000";
pub static DEFAULT_TIMEOUT: u32 = 10000;
pub static DEFAULT_TIMEOUT_SECONDS_STR: &'static str = "10";

/// Notification information to send over the wire from a remote client
/// to a local listening server
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiNote {
    title: String,
    msg: String,
    timeout: u32,
}
impl ApiNote {
    /// Create a new api-note with a message and default values
    fn with_msg(msg: &str) -> ApiNote {
        ApiNote {
            title: DEFAULT_TITLE.to_owned(),
            msg: msg.to_owned(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    /// Set a title, overriding the default
    fn title(mut self, title: &str) -> ApiNote {
        self.title = title.to_owned();
        self
    }

    /// Set a timeout in milliseconds, overriding the default
    fn timeout(mut self, millis: u32) -> ApiNote {
        self.timeout = millis;
        self
    }
}

/// Notification builder
pub struct Note {
    pub title: String,
    pub msg: String,
    pub timeout: u32,
    pub send: bool,
    pub host: String,
    pub port: u32,
}
impl Note {
    /// Create a new notification with a given message and default values
    pub fn with_msg(msg: &str) -> Note {
        Note {
            title: DEFAULT_TITLE.to_owned(),
            msg: msg.to_owned(),
            timeout: DEFAULT_TIMEOUT,
            send: false,
            host: DEFAULT_HOST.to_owned(),
            port: DEFAULT_PORT,
        }
    }

    pub fn msg(mut self, msg: &str) -> Note {
        self.msg = msg.to_owned();
        self
    }

    /// Set a title, overriding the default
    pub fn title(mut self, title: &str) -> Note {
        self.title = title.to_owned();
        self
    }

    /// Set timeout in milliseconds, overriding the default
    pub fn timeout(mut self, millis: u32) -> Note {
        self.timeout = millis;
        self
    }

    /// Set whether the notification should send itself to a listener
    pub fn send(mut self, send: bool) -> Note {
        self.send = send;
        self
    }

    /// Set the receiving host (for sending), replacing the default
    pub fn host(mut self, host: &str) -> Note {
        self.host = host.to_owned();
        self
    }

    /// Set the receiving port (for sending), replacing the default
    pub fn port(mut self, port: u32) -> Note {
        self.port = port;
        self
    }

    fn from_matches(matches: &ArgMatches) -> Result<Note> {
        // Capture default and overridden notification arguments
        let send = matches.is_present("send")
            || env::var("CLIN_SEND")
                .ok()
                .and_then(|s| if s == "1" { Some(()) } else { None })
                .is_some();
        let fallback_host = env::var("CLIN_SEND_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
        let host = matches.value_of("host").unwrap_or(&fallback_host);
        let fallback_port =
            env::var("CLIN_SEND_PORT").unwrap_or_else(|_| DEFAULT_PORT_STR.to_string());
        let port = matches
            .value_of("port")
            .unwrap_or(&fallback_port)
            .parse::<u32>()?;
        let fallback_timeout =
            env::var("CLIN_TIMEOUT").unwrap_or_else(|_| DEFAULT_TIMEOUT_STR.to_string());
        let timeout = matches
            .value_of("timeout")
            .unwrap_or(&fallback_timeout)
            .parse::<u32>()?;
        let note = Note::with_msg(DEFAULT_MESSAGE)
            .timeout(timeout)
            .send(send)
            .host(host)
            .port(port);
        Ok(note)
    }

    /// Create the notification locally, or send it over the wire
    ///
    /// Errors:
    ///     * Serializing `ApiNote`
    ///     * Connecting to a listener
    ///     * Writing to listener stream
    ///     * Communicating to the system notification-server
    pub fn push(self) -> Result<()> {
        if self.send {
            use io::Write;
            let addr = format!("{}:{}", self.host, self.port);
            let note = ApiNote::with_msg(&self.msg)
                .title(&self.title)
                .timeout(self.timeout);
            let note = serde_json::to_string(&note)?;
            let mut stream = net::TcpStream::connect(&addr)?;
            stream.write(note.as_bytes())?;
        } else {
            Notification::new()
                .icon(DEFAULT_ICON)
                .summary(&self.title)
                .timeout(Timeout::Milliseconds(self.timeout))
                .body(&self.msg)
                .show()?;
        }
        Ok(())
    }
}

/// Check if we can connect to the specified receiver
///
/// Errors:
///     * Connecting to listener
///     * Writing to listener stream
fn can_connect(host: &str, port: u32) -> Result<()> {
    use io::Write;
    let addr = format!("{}:{}", host, port);
    let mut stream = net::TcpStream::connect(&addr)?;
    stream.write("ping".as_bytes())?;
    Ok(())
}

/// Run a command in foreground
///
/// Errors:
///     * Converting `cmd` to `Cstring`
///     * `cmd` exited with non-zero status-code
fn run_command(cmd: &str) -> Result<()> {
    let c_str = ffi::CString::new(cmd)?;
    let ret = unsafe {
        let ret = libc::system(c_str.as_ptr());
        // convert child status code to a normal code 0-255
        libc::WEXITSTATUS(ret)
    };
    if ret != 0 {
        return Err(Error::Command(ret));
    }
    Ok(())
}

/// Collect all default and overridden notification parameters from argument matches,
/// returning a captured command-string and a constructed `Note`
///
/// Errors:
///     * Unable to parse input integers
///     * Unable to connect to specified listener with provided `host`/`port`
///     * No `command-string` provided
fn collect_cmd_note(matches: &ArgMatches) -> Result<(String, Note)> {
    let note = Note::from_matches(&matches)?;

    // If sending, make sure specified connection works
    if note.send && can_connect(&note.host, note.port).is_err() {
        bail!(
            Error::Network,
            "Unable to connect to clin-listener at `{}:{}`",
            note.host,
            note.port
        )
    }

    // Capture command contents or bail out if nothing was provided
    let cmd = match (
        matches.value_of("command_string"),
        matches.is_present("cmd"),
    ) {
        (Some(c), _) => c.to_owned(),
        (_, true) => {
            // Pull out the full trailing args list...
            // The built in parsing will strip out any '--' from the list
            let args = env::args().collect::<Vec<_>>();
            let ind = match args.iter().position(|item| item == "--") {
                None => bail!(Error::Msg, "Error parsing command, no `--` delimiter found"),
                Some(i) => i,
            };
            let (_, args) = args.split_at(ind + 1);
            args.join(" ")
        }
        _ => bail_help!(),
    };
    let note = note.msg(&cmd);
    Ok((cmd, note))
}

#[cfg(feature = "update")]
fn update(matches: &ArgMatches) -> Result<()> {
    let mut builder = self_update::backends::github::Update::configure();

    builder
        .repo_owner("jaemk")
        .repo_name("clin")
        .target(self_update::get_target())
        .bin_name("clin")
        .show_download_progress(true)
        .no_confirm(matches.is_present("no_confirm"))
        .current_version(APP_VERSION);

    if matches.is_present("quiet") {
        builder.show_output(false).show_download_progress(false);
    }

    let status = builder.build()?.update()?;
    match status {
        self_update::Status::UpToDate(v) => {
            println!("Already up to date [v{}]!", v);
        }
        self_update::Status::Updated(v) => {
            println!("Updated to {}!", v);
        }
    }
    return Ok(());
}

#[cfg(not(feature = "update"))]
fn update(_: &ArgMatches) -> Result<()> {
    bail!(Error::Msg, "This executable was not compiled with `self_update` features enabled via `--features update`");
}

/// Dispatch over arguments
fn run(matches: ArgMatches) -> Result<()> {
    if let Some(self_matches) = matches.subcommand_matches("self") {
        if let Some(update_matches) = self_matches.subcommand_matches("update") {
            return update(update_matches);
        }
        bail_help!()
    }

    if let Some(listen_matches) = matches.subcommand_matches("listen") {
        return listen::start_listener(listen_matches);
    }

    if let Some(msg) = matches.value_of("message_string") {
        Note::from_matches(&matches)?
            .msg(msg)
            .title("clin")
            .push()?;
        return Ok(());
    }

    let (cmd, note) = collect_cmd_note(&matches)?;
    eprintln!("clin: `{}`", cmd);

    let title = match run_command(&cmd) {
        Err(Error::Command(ret)) => format!("Error ✗ -- exit status: {}", ret),
        Err(e) => return Err(e),
        Ok(_) => "Complete ✓".to_string(),
    };
    note.title(&title).push()
}

fn main() {
    let matches = App::new("clin")
        .setting(AppSettings::TrailingVarArg)
        .version(APP_VERSION)
        .author("James K. <james@kominick.com>")
        .about("\
Command line notification tool
Supports local and networked notifications

examples:
clin -- ./some-build-script.sh --flag --arg1 'some arg'
clin -c \"./some-build-script.sh --flag --arg1 'some arg'\"
clin -m \"just post this message\"")
        .subcommand(SubCommand::with_name("self")
                    .about("Self referential things")
                    .subcommand(SubCommand::with_name("update")
                        .about("Update to the latest binary release, replacing this binary")
                        .arg(Arg::with_name("no_confirm")
                             .help("Skip download/update confirmation")
                             .long("no-confirm")
                             .short("y")
                             .required(false)
                             .takes_value(false))
                        .arg(Arg::with_name("quiet")
                             .help("Suppress unnecessary download output (progress bar)")
                             .long("quiet")
                             .short("q")
                             .required(false)
                             .takes_value(false))))
        .subcommand(SubCommand::with_name("listen")
                    .about("Listen for network notifications")
            .arg(Arg::with_name("log")
                 .help("Turn on server logging. Shortcut for `LOG=info clin listen`")
                 .long("log")
                 .required(false)
                 .takes_value(false))
            .arg(Arg::with_name("port")
                 .help(&format!("Port to listen on, defaults to `{}`, overrides `CLIN_LISTEN_PORT`", DEFAULT_PORT))
                 .long("port")
                 .short("p")
                 .required(false)
                 .takes_value(true))
            .arg(Arg::with_name("public")
                 .help("Listen publicly on 0.0.0.0, instead of 127.0.0.1")
                 .long("public")
                 .required(false)
                 .takes_value(false)))
        .arg(Arg::with_name("send")
             .help("Send notification to a clin-listener, also enabled by `CLIN_SEND=1`")
             .long("send")
             .short("s")
             .required(false)
             .takes_value(false))
        .arg(Arg::with_name("host")
             .help(&format!("Host to send notification to, defaults to `{}`, overrides `CLIN_SEND_HOST`", DEFAULT_HOST))
             .long("host")
             .required(false)
             .takes_value(true))
        .arg(Arg::with_name("port")
             .help(&format!("Port to send notification over, defaults to `{}`, overrides `CLIN_SEND_PORT`", DEFAULT_PORT))
             .long("port")
             .short("p")
             .required(false)
             .takes_value(true))
        .arg(Arg::with_name("timeout")
             .help(&format!("Notification timeout in milliseconds, defaults to `{}s`, overrides `CLIN_TIMEOUT`", DEFAULT_TIMEOUT_SECONDS_STR))
             .long("timeout")
             .short("t")
             .required(false)
             .takes_value(true))
        .arg(Arg::with_name("message_string")
             .help("Message to post as a notification, overrides cmd string and trailing args")
             .long("message")
             .short("m")
             .required(false)
             .takes_value(true))
        .arg(Arg::with_name("command_string")
             .help("Specify command to run as a string, overrides trailing args")
             .long("command")
             .short("c")
             .required(false)
             .takes_value(true))
        .arg(Arg::with_name("cmd")
             .help("Specify a command as arguments trailing an initial `--`")
             .multiple(true)
             .required(false)
             .last(true))
        .get_matches();

    if let Err(e) = run(matches) {
        eprintln!("[ERROR] {}", e);
        process::exit(1);
    }
}
