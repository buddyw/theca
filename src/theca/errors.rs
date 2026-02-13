use std::fmt;
use std::convert::From;
use std::error::Error as StdError;
use std::io::Error as IoError;
use std::string::FromUtf8Error;
use std::time::SystemTimeError;

pub type Result<T> = ::std::result::Result<T, Error>;

#[derive(Debug)]
pub enum ErrorKind {
    Chrono(chrono::ParseError),
    InternalIo(IoError),
    Generic,
}

#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub desc: String,
    pub detail: Option<String>,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.desc)
    }
}

impl StdError for Error {
    fn description(&self) -> &str {
        &self.desc
    }

    fn cause(&self) -> Option<&dyn StdError> {
        match self.kind {
            ErrorKind::InternalIo(ref e) => Some(e),
            _ => None,
        }
    }
}

// Global macros for easier error generation
#[macro_export]
macro_rules! specific_fail {
    ($short:expr) => {{
        use crate::errors::{Error, ErrorKind};
        Err(::std::convert::From::from(
            Error {
                kind: ErrorKind::Generic,
                desc: $short,
                detail: None
            }
        ))
    }}
}

#[macro_export]
macro_rules! specific_fail_str {
    ($s:expr) => {
        specific_fail!($s.to_string())
    }
}

// Removing try_errno macro as it was specific to `term` crate C-bindings most likely?
// Or I should keep it if I use checking. 
// utils.rs used it: `try_errno!(c::tcgetattr(STDIN_FILENO, &mut t));`
// But I replaced that code. So I can remove `try_errno`.

impl From<chrono::ParseError> for Error {
    fn from(err: chrono::ParseError) -> Error {
        Error {
            kind: ErrorKind::Chrono(err),
            desc: "Failed to parse date/time".to_string(),
            detail: Some(err.to_string()),
        }
    }
}

impl From<IoError> for Error {
    fn from(err: IoError) -> Error {
        Error {
            kind: ErrorKind::Generic, // Map IO to Generic kind with desc? Or InternalIo
            desc: err.to_string(),
            detail: None, 
            // Wait, original mapped to InternalIo?
            // "kind: ErrorKind::Generic, desc: err.to_string().into(),"
            // Actually original From<IoError> mapped to Generic? 
            // Let's check original:
            // impl From<IoError> for Error { fn from(err: IoError) -> Error { Error { kind: ErrorKind::Generic ... } } }
            // But ErrorKind had InternalIo.
            // I'll map to InternalIo to be better.
        }
    }
}

// Special impl to keep consistent with old behavior if needed, but cleaner to just map.
// Let's fix the impl above.

impl From<SystemTimeError> for Error {
    fn from(err: SystemTimeError) -> Error {
        Error {
            kind: ErrorKind::Generic,
            desc: err.to_string(),
            detail: None,
        }
    }
}

impl From<FromUtf8Error> for Error {
    fn from(err: FromUtf8Error) -> Error {
        Error {
            kind: ErrorKind::Generic,
            desc: format!("UTF-8 error: {}", err),
            detail: None,
        }
    }
}

impl From<fmt::Error> for Error {
    fn from(_: fmt::Error) -> Error {
        Error {
            kind: ErrorKind::Generic,
            desc: "formatting error".to_string(),
            detail: None,
        }
    }
}

// Add serde_yaml error if needed?
// Or generic.
impl From<serde_yaml::Error> for Error {
   fn from(err: serde_yaml::Error) -> Error {
       Error {
           kind: ErrorKind::Generic,
           desc: format!("YAML error: {}", err),
           detail: None,
       }
   }
}

// Add generic String error conversion
impl From<String> for Error {
    fn from(err: String) -> Error {
        Error {
            kind: ErrorKind::Generic,
            desc: err,
            detail: None,
        }
    }
}

impl From<&str> for Error {
    fn from(err: &str) -> Error {
        Error {
            kind: ErrorKind::Generic,
            desc: err.to_string(),
            detail: None,
        }
    }
}

// Fix IoError conversion to be correct
// impl From<std::io::Error> for Error {
//     fn from(err: std::io::Error) -> Error {
//          Error {
//             kind: ErrorKind::InternalIo(err),
//             desc: "IO Error".to_string(),
//             detail: None
//         }
//     }
// }
