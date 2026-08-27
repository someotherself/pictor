pub mod codecs;
pub mod samples;

pub type PictorResult<T> = Result<T, PictorError>;

#[non_exhaustive]
pub enum PictorError {
    InvalidArgument {
        msg: &'static str,
    },
    IoError {
        err: std::io::Error,
    },
    IntegerError {
        err: std::num::TryFromIntError,
    },
    // checked_mul error
    MulOverflow {
        op: &'static str, // "frames * channels"
    },
    FileSizeExceeded,
    InvalidFormat {
        msg: String,
    },
}

impl std::fmt::Debug for PictorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidArgument { msg } => write!(f, "Invalid Argument: {}", msg),
            Self::IoError { err } => write!(f, "I/O Error: {err}"),
            Self::IntegerError { err } => write!(f, "Integer Error: {err}"),
            Self::MulOverflow { op } => write!(f, "{op}"),
            Self::FileSizeExceeded => write!(f, "Maximum file size exceeded"),
            Self::InvalidFormat { msg } => write!(f, "Does not match expected format: {msg}"),
        }
    }
}

impl From<std::io::Error> for PictorError {
    fn from(value: std::io::Error) -> Self {
        PictorError::IoError { err: value }
    }
}

impl From<std::num::TryFromIntError> for PictorError {
    fn from(value: std::num::TryFromIntError) -> Self {
        PictorError::IntegerError { err: value }
    }
}
