//! rust-compression
//!
//! # Licensing
//! This Source Code is subject to the terms of the Mozilla Public License
//! version 2.0 (the "License"). You can obtain a copy of the License at
//! <http://mozilla.org/MPL/2.0/>.

use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionError {
    DataError,
    UnexpectedEof,
    Unexpected,
}

impl fmt::Display for CompressionError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description_in())
    }
}

#[cfg(feature = "std")]
impl ::std::error::Error for CompressionError {
    fn description(&self) -> &str {
        self.description_in()
    }

    fn cause(&self) -> Option<&dyn (::std::error::Error)> {
        None
    }
}

impl CompressionError {
    fn description_in(&self) -> &str {
        match *self {
            CompressionError::DataError => "data integrity error in data",
            CompressionError::UnexpectedEof => "file ends unexpectedly",
            CompressionError::Unexpected => "unexpected error",
        }
    }
}
