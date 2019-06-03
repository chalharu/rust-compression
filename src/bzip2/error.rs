use core::fmt;
use error::CompressionError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BZip2Error {
    DataError,
    DataErrorMagicFirst,
    DataErrorMagic,
    UnexpectedEof,
    Unexpected,
}

impl fmt::Display for BZip2Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.description_in())
    }
}

#[cfg(feature = "std")]
impl ::std::error::Error for BZip2Error {
    fn description(&self) -> &str {
        self.description_in()
    }

    fn cause(&self) -> Option<&dyn (::std::error::Error)> {
        None
    }
}

impl BZip2Error {
    fn description_in(&self) -> &str {
        match *self {
            BZip2Error::DataError => "data integrity (CRC) error in data",
            BZip2Error::DataErrorMagicFirst => {
                "bad magic number (file not created by bzip2)"
            }
            BZip2Error::DataErrorMagic => "trailing garbage after EOF ignored",
            BZip2Error::UnexpectedEof => "file ends unexpectedly",
            BZip2Error::Unexpected => "unexpected error",
        }
    }
}

impl From<BZip2Error> for CompressionError {
    fn from(error: BZip2Error) -> Self {
        match error {
            BZip2Error::UnexpectedEof => CompressionError::UnexpectedEof,
            BZip2Error::Unexpected => CompressionError::Unexpected,
            _ => CompressionError::DataError,
        }
    }
}
