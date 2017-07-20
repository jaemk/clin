use std::env;
use std::net;

use serde_json;
use env_logger;
use clap::ArgMatches;
use chrono::Local;

use errors::*;
use super::{ApiNote, Note, DEFAULT_PORT_STR};


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


/// Listen on the given address for incoming `ApiNote` messages
/// and generate local notifications
///
/// Errors:
///     * Binding to a <host:port>
///     * Reading from opened stream
///     * Deserializing incoming `ApiNote`s
///     * Communication to the system notification-server
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
        info!("[{}]: {}", note.title, note.msg);
        Note::with_msg(&note.msg)
            .title(&note.title)
            .timeout(note.timeout)
            .push()?;
    }
    Ok(())
}


/// Pull out listener parameters and spin up a listerner
///
/// Errors:
///     * Parsing argument integers
///     * Initializing the listener
pub fn start_listener(matches: &ArgMatches) -> Result<()> {
    init_logger(matches.is_present("log"));
    let host = if matches.is_present("public") { "0.0.0.0" } else { "127.0.0.1" };
    let fallback_port = env::var("CLIN_LISTEN_PORT").unwrap_or_else(|_| DEFAULT_PORT_STR.to_string());
    let port = matches.value_of("port")
        .unwrap_or(&fallback_port)
        .parse::<u32>()?;
    let addr = format!("{}:{}", host, port);
    return listen(&addr);
}

