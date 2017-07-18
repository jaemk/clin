/*!
Error type, conversions, and macros

*/
use std;
use notify_rust;
use serde_json;

pub type Result<T> = std::result::Result<T, Error>;


#[derive(Debug)]
pub enum Error {
    Msg(String),
    Network(String),
    Command(i32),
    Io(std::io::Error),
    Nul(std::ffi::NulError),
    ParseInt(std::num::ParseIntError),
    Notify(notify_rust::Error),
    Json(serde_json::Error),
}


impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        use Error::*;
        match *self {
            Msg(ref s)      => write!(f, "{}", s),
            Network(ref s)  => write!(f, "NetworkError: {}", s),
            Command(n)      => write!(f, "CommandError-StatusCode: {}", n),
            Io(ref e)       => write!(f, "IoError: {}", e),
            Nul(ref e)      => write!(f, "NulError: {}", e),
            ParseInt(ref e) => write!(f, "ParseIntError: {}", e),
            Notify(ref e)   => write!(f, "NotifyError: {}", e),
            Json(ref e)     => write!(f, "JsonError: {}", e),
        }
    }
}


impl std::error::Error for Error {
    fn description(&self) -> &str {
        "CLIN Error"
    }

    fn cause(&self) -> Option<&std::error::Error> {
        use Error::*;
        Some(match *self {
            Io(ref e)           => e,
            Nul(ref e)          => e,
            ParseInt(ref e)     => e,
            Notify(ref e)       => e,
            Json(ref e)         => e,
            _ => return None,
        })
    }
}


impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Error {
        Error::Io(e)
    }
}

impl From<std::ffi::NulError> for Error {
    fn from(e: std::ffi::NulError) -> Error {
        Error::Nul(e)
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(e: std::num::ParseIntError) -> Error {
        Error::ParseInt(e)
    }
}

impl From<notify_rust::Error> for Error {
    fn from(e: notify_rust::Error) -> Error {
        Error::Notify(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Error {
        Error::Json(e)
    }
}


macro_rules! format_err {
    ($e_type:expr, $literal:expr) => {
        $e_type(format!($literal))
    };
    ($e_type:expr, $literal:expr, $($arg:expr),*) => {
        $e_type(format!($literal, $($arg),*))
    };
}

macro_rules! bail {
    ($e_type:expr, $literal:expr) => {
        return Err(format_err!($e_type, $literal))
    };
    ($e_type:expr, $literal:expr, $($arg:expr),*) => {
        return Err(format_err!($e_type, $literal, $($arg),*))
    };
}
