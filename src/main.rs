#[macro_use] extern crate clap;
extern crate notify_rust;
extern crate libc;

#[macro_use] extern crate serde_derive;
extern crate serde;
extern crate serde_json;

#[macro_use] mod errors;

use clap::{App, Arg, SubCommand, ArgMatches, AppSettings};
use notify_rust::{Notification, Timeout};

use std::io;
use std::process;
use std::ffi;
use std::net;

use errors::*;


pub static DEFAULT_TITLE:       &'static str = "CLIN:";
pub static DEFAULT_ICON:        &'static str = "terminal";
pub static DEFAULT_HOST:        &'static str = "127.0.0.1";
pub static DEFAULT_PORT_STR:    &'static str = "6445";
pub static DEFAULT_PORT:        usize        = 6445;
pub static DEFAULT_TIMEOUT_STR: &'static str = "10000";
pub static DEFAULT_TIMEOUT:     u32          = 10000;


fn main() {
    let matches = App::new("CLIN")
        .setting(AppSettings::TrailingVarArg)
        .version(crate_version!())
        .author("James K. <james.kominick@gmail.com>")
        .about("\
Command line notification tool
Supports local and networked notifications")
        .subcommand(SubCommand::with_name("listen")
                    .about("Listen for network notifications")
            .arg(Arg::with_name("port")
                 .help(&format!("Port to listen on, defaults to {}", DEFAULT_PORT))
                 .long("port")
                 .short("p")
                 .required(false)
                 .takes_value(true)))
        .arg(Arg::with_name("send")
             .help("Send notification to a clin-listener on the default or specified port")
             .long("send")
             .short("s")
             .required(false)
             .takes_value(false))
        .arg(Arg::with_name("port")
             .help("Port to send notification on, defaults to 6445")
             .long("port")
             .short("p")
             .required(false)
             .takes_value(true))
        .arg(Arg::with_name("timeout")
             .help("Notification timeout in milliseconds, defaults to 10s")
             .long("timeout")
             .short("t")
             .required(false)
             .takes_value(true))
        .arg(Arg::with_name("cmd")
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
        use std::io::Read;

        let port = listen_matches.value_of("port")
            .unwrap_or(DEFAULT_PORT_STR)
            .parse::<usize>()?;

        let addr = format!("{}:{}", DEFAULT_HOST, port);
        println!("** Listening on {} **", addr);
        let listener = net::TcpListener::bind(&addr)?;
        for stream in listener.incoming() {
            let mut stream = stream?;
            println!("new connection");
            let mut s = String::new();
            stream.read_to_string(&mut s)?;
            let note: ApiNote = serde_json::from_str(&s)?;
            println!("Incoming! <- {:?}", note);
            Note::with_msg(&note.msg)
                .timeout(note.timeout)
                .port(port)
                .push()?;
        }

        return Ok(())
    }

    let send = matches.is_present("send");
    let port = matches.value_of("port")
        .unwrap_or(DEFAULT_PORT_STR)
        .parse::<usize>()?;
    let timeout = matches.value_of("timeout")
        .unwrap_or(DEFAULT_TIMEOUT_STR)
        .parse::<u32>()?;
    if let Some(cmd) = matches.values_of("cmd").map(|stuff| stuff.collect::<Vec<&str>>()) {
        let cmd = cmd.into_iter().map(String::from).collect::<Vec<String>>();
        let cmd = cmd.join(" ");
        println!("clin: `{}`", cmd);

        let add_msg = match run_command(&cmd) {
            Err(Error::Command(e)) => {
                format!("{}", e)
            }
            Err(e) => return Err(e),
            Ok(_) => "Complete ✓".to_string(),
        };
        let msg = format!("`{}`\n{}", cmd, add_msg);
        Note::with_msg(&msg)
            .timeout(timeout)
            .send(send)
            .port(port)
            .push()?;
        return Ok(())
    }

    println!("clin: see `--help`");
    Ok(())
}


fn run_command(command: &str) -> Result<()> {
    let c_str = ffi::CString::new(command)?;
    let ret = unsafe { libc::system(c_str.as_ptr()) };
    if ret != 0 { bail!(Error::Command, "Command `{}` failed with status: `{}`", command, ret) }
    Ok(())
}


#[derive(Debug, Serialize, Deserialize)]
struct ApiNote {
    msg: String,
    timeout: u32,
}
impl ApiNote {
    fn with_msg(msg: &str) -> ApiNote {
        ApiNote { msg: msg.to_owned(), timeout: DEFAULT_TIMEOUT }
    }
    fn timeout(mut self, millis: u32) -> ApiNote {
        self.timeout = millis;
        self
    }
}


struct Note {
    msg: String,
    timeout: u32,
    send: bool,
    port: usize,
}
impl Note {
    fn with_msg(msg: &str) -> Note {
        Note { msg: msg.to_owned(), timeout: DEFAULT_TIMEOUT, send: false, port: DEFAULT_PORT }
    }
    fn timeout(mut self, millis: u32) -> Note {
        self.timeout = millis;
        self
    }
    fn send(mut self, send: bool) -> Note {
        self.send = send;
        self
    }
    fn port(mut self, port: usize) -> Note {
        self.port = port;
        self
    }
    fn push(self) -> Result<()> {
        if self.send {
            use std::io::Write;
            let addr = format!("{}:{}", DEFAULT_HOST, self.port);
            let note = ApiNote::with_msg(&self.msg).timeout(self.timeout);
            let note = serde_json::to_string(&note)?;
            let mut stream = net::TcpStream::connect(&addr)?;
            stream.write(note.as_bytes())?;
        } else {
            Notification::new()
                .icon(DEFAULT_ICON)
                .summary(DEFAULT_TITLE)
                .timeout(Timeout::Milliseconds(self.timeout))
                .body(&self.msg)
                .show()?;
        }
        Ok(())
    }
}
