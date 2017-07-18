#[macro_use] extern crate clap;
extern crate notify_rust;
extern crate libc;
#[macro_use] extern crate log;
extern crate env_logger;
extern crate chrono;
#[macro_use] extern crate serde_derive;
extern crate serde;
extern crate serde_json;

#[macro_use] mod errors;

use clap::{App, Arg, SubCommand, ArgMatches, AppSettings};
use notify_rust::{Notification, Timeout};
use chrono::Local;

use std::io;
use std::env;
use std::ffi;
use std::process;
use std::net;

use errors::*;


pub static DEFAULT_TITLE:       &'static str = "CLIN:";
pub static DEFAULT_ICON:        &'static str = "terminal";
pub static DEFAULT_HOST:        &'static str = "127.0.0.1";
pub static DEFAULT_PORT_STR:    &'static str = "6445";
pub static DEFAULT_PORT:        usize        = 6445;
pub static DEFAULT_TIMEOUT_STR: &'static str = "10000";
pub static DEFAULT_TIMEOUT:     u32          = 10000;
pub static DEFAULT_TIMEOUT_SECONDS_STR: &'static str = "10";


fn main() {
    let matches = App::new("CLIN")
        .setting(AppSettings::TrailingVarArg)
        .version(crate_version!())
        .author("James K. <james.kominick@gmail.com>")
        .about("\
Command line notification tool
Supports local and networked notifications

examples:
clin -- ./some-build-script.sh --flag --arg1 'some arg'
clin -c \"./some-build-script.sh --flag --arg1 'some arg'\"")
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
             .help("Send notification to a clin-listener, also enabled by `CLIN_SEND`")
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
        use io::Write;
        let mut stderr = io::stderr();
        writeln!(stderr, "[ERROR] {}", e)
            .expect("Failed writing to stderr");
        process::exit(1);
    }
}


fn run(matches: ArgMatches) -> Result<()> {
    if let Some(listen_matches) = matches.subcommand_matches("listen") {
        init_logger(listen_matches.is_present("log"));
        let host = if listen_matches.is_present("public") { "0.0.0.0" } else { "127.0.0.1" };
        let fallback_port = env::var("CLIN_LISTEN_PORT").unwrap_or_else(|_| DEFAULT_PORT_STR.to_string());
        let port = listen_matches.value_of("port")
            .unwrap_or(&fallback_port)
            .parse::<usize>()?;
        let addr = format!("{}:{}", host, port);
        return listen(&addr);
    }

    let send = matches.is_present("send") ||
        env::var("CLIN_SEND").ok()
            .and_then(|s| if s == "1" { Some(()) } else { None })
            .is_some();
    let fallback_host = env::var("CLIN_SEND_HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());
    let host = matches.value_of("host")
        .unwrap_or(&fallback_host);
    let fallback_port = env::var("CLIN_SEND_PORT").unwrap_or_else(|_| DEFAULT_PORT_STR.to_string());
    let port = matches.value_of("port")
        .unwrap_or(&fallback_port)
        .parse::<usize>()?;
    let fallback_timeout = env::var("CLIN_TIMEOUT").unwrap_or_else(|_| DEFAULT_TIMEOUT_STR.to_string());
    let timeout = matches.value_of("timeout")
        .unwrap_or(&fallback_timeout)
        .parse::<u32>()?;

    if send && can_connect(&host, port).is_err() {
        bail!(Error::Network, "Unable to connect to clin-listener at `{}:{}`", host, port)
    }

    let cmd = match (matches.value_of("command_string"), matches.is_present("cmd")) {
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
        _ => {
            println!("clin: see `--help`");
            return Ok(())
        }
    };

    println!("clin: `{}`", cmd);

    let title = match run_command(&cmd) {
        Err(Error::Command(ret)) => {
            format!("Error ✗ -- exit status: {}", ret)
        }
        Err(e) => return Err(e),
        Ok(_) => "Complete ✓".to_string(),
    };

    Note::with_msg(&cmd)
        .title(&title)
        .timeout(timeout)
        .send(send)
        .host(host)
        .port(port)
        .push()?;
    Ok(())
}


/// Initialize loggers for the listening server
fn init_logger(log: bool) {
    if log {
        env::set_var("LOG", "info")
    }

    // Set a custom logging format & change the env-var to "LOG"
    // e.g. LOG=info clin listen
    env_logger::LogBuilder::new()
        .format(|record| {
            format!("{} [{}] - [{}] -> {}",
                Local::now().format("%Y-%m-%d_%H:%M:%S"),
                record.level(),
                record.location().module_path(),
                record.args()
                )
            })
        .parse(&env::var("LOG").unwrap_or_default())
        .init()
        .expect("failed to initialize logger");
}


/// Check if we can connect to the specified receiver
fn can_connect(host: &str, port: u32) -> Result<()> {
    use io::Write;
    let addr = format!("{}:{}", host, port);
    let mut stream = net::TcpStream::connect(&addr)?;
    stream.write("ping".as_bytes())?;
    Ok(())
}


/// Listen on the given address for incoming `ApiNote` messages
/// and generate local notifications
fn listen(addr: &str) -> Result<()> {
    use io::Read;
    info!("** Listening on {} **", addr);

    let listener = net::TcpListener::bind(&addr)?;
    for stream in listener.incoming() {
        let mut stream = stream?;
        let mut s = String::new();
        stream.read_to_string(&mut s)?;
        if s == "ping" { continue; }
        let note: ApiNote = serde_json::from_str(&s)?;
        info!("{:?}", note);
        Note::with_msg(&note.msg)
            .title(&note.title)
            .timeout(note.timeout)
            .push()?;
    }
    Ok(())
}


/// Run a command in foreground
fn run_command(cmd: &str) -> Result<()> {
    let c_str = ffi::CString::new(cmd)?;
    let ret = unsafe {
        let ret = libc::system(c_str.as_ptr());
        // convert child status code to a normal code 0-255
        libc::WEXITSTATUS(ret)
    };
    if ret != 0 { return Err(Error::Command(ret)) }
    Ok(())
}


/// Notification information to send over the wire from a remote client
/// to a local listening server
#[derive(Debug, Serialize, Deserialize)]
struct ApiNote {
    title: String,
    msg: String,
    timeout: u32,
}
impl ApiNote {
    fn with_msg(msg: &str) -> ApiNote {
        ApiNote {
            title: DEFAULT_TITLE.to_owned(),
            msg: msg.to_owned(),
            timeout: DEFAULT_TIMEOUT,
        }
    }
    fn title(mut self, title: &str) -> ApiNote {
        self.title = title.to_owned();
        self
    }
    fn timeout(mut self, millis: u32) -> ApiNote {
        self.timeout = millis;
        self
    }
}


/// Notification builder
struct Note {
    title: String,
    msg: String,
    timeout: u32,
    send: bool,
    host: String,
    port: usize,
}
impl Note {
    fn with_msg(msg: &str) -> Note {
        Note {
            title: DEFAULT_TITLE.to_owned(),
            msg: msg.to_owned(),
            timeout: DEFAULT_TIMEOUT,
            send: false,
            host: DEFAULT_HOST.to_owned(),
            port: DEFAULT_PORT }
    }
    fn title(mut self, title: &str) -> Note {
        self.title = title.to_owned();
        self
    }
    fn timeout(mut self, millis: u32) -> Note {
        self.timeout = millis;
        self
    }
    fn send(mut self, send: bool) -> Note {
        self.send = send;
        self
    }
    fn host(mut self, host: &str) -> Note {
        self.host = host.to_owned();
        self
    }
    fn port(mut self, port: usize) -> Note {
        self.port = port;
        self
    }
    fn push(self) -> Result<()> {
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
