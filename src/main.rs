use std::error::Error;

use backtrace::Backtrace;

#[derive(Debug)]
// One can check equality of Kinds only if all their constituents can be
// checked for equality.
#[derive(Eq, PartialEq)]
enum OurErrorKind {
    // Raised when a devicemapper context is not initialized
    ContextInitError,
    // Raised when any method receives an argument it can not handle
    InvalidArgument { description: String },
    // below should be Box<DeviceInfo>
    // ioctl failure
    IoctlError { device_info: String },
    // ioctl result is too large
    IoctlResultTooLarge,
    // Failed to get metadata
    MetadataIoError { path: std::path::PathBuf },
}

impl std::fmt::Display for OurErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            OurErrorKind::ContextInitError => write!(f, "DM context not initialized"),
            OurErrorKind::InvalidArgument { description } => {
                write!(f, "invalid argument: {}", description)
            }
            OurErrorKind::IoctlError { device_info, .. } => {
                write!(f, "ioctl error, device info: {}", device_info)
            }
            OurErrorKind::IoctlResultTooLarge => write!(
                f,
                "ioctl result too large for maximum buffer size 4294967295 bytes"
            ),
            OurErrorKind::MetadataIoError { path } => write!(
                f,
                "failed to stat metadata for device at {}",
                path.to_string_lossy()
            ),
        }
    }
}

#[derive(Debug)]
/// What relation the component error has to its parent
enum Suberror {
    /// The error occurred before the parent error
    Previous(Box<std::error::Error>),
    /// The error is further explained or extended by the parent
    Constituent(Box<std::error::Error>),
}

#[derive(Debug)]
struct OurError {
    // The source of the error, which may be an error for
    // which this error is a further explanation, i.e., a
    // constituent error, or it may simply be an error that occurred
    // previously, and which presumably caused the current code to
    // be run and encounter its own, novel error.
    source_impl: Option<Suberror>,

    // The backtrace at the site the error is returned
    backtrace: Backtrace,

    // Distinguish among different errors with an ErrorKind
    specifics: OurErrorKind,
}

impl OurError {
    fn new(kind: OurErrorKind) -> OurError {
        OurError {
            backtrace: Backtrace::new(),
            source_impl: None,
            specifics: kind,
        }
    }

    /// Return the optional backtrace associated with this error.
    // Note that the function name is our_backtrace, so that it does not
    // conflict with a future possible backtrace function in the Error trait.
    pub fn our_backtrace(&self) -> Option<&Backtrace> {
        Some(&self.backtrace)
    }

    /// Set extension as the extension on this error.
    /// Return the head of the chain, now subsequent.
    pub fn set_extension(self, mut extension: OurError) -> OurError {
        extension.source_impl = Some(Suberror::Constituent(Box::new(self)));
        extension
    }

    /// Set subsequent as the subsequent error for this error.
    /// Return the head of the chain, now subsequent.
    pub fn set_subsequent(self, mut subsequent: OurError) -> OurError {
        subsequent.source_impl = Some(Suberror::Previous(Box::new(self)));
        subsequent
    }

    /// Set constituent as the constituent of this error.
    pub fn set_constituent(&mut self, constituent: Box<std::error::Error>) {
        self.source_impl = Some(Suberror::Constituent(constituent));
    }

    /// Set previous as the previous error.
    pub fn set_previous(&mut self, previous: Box<std::error::Error>) {
        self.source_impl = Some(Suberror::Previous(previous));
    }

    /// Obtain the immediate previous error, if there is one
    pub fn previous(&self) -> Option<&(std::error::Error + 'static)> {
        match self.source_impl.as_ref() {
            Some(Suberror::Previous(c)) => Some(&**c),
            _ => None,
        }
    }

    /// Obtain the immediate constituent error, if there is one
    pub fn constituent(&self) -> Option<&(std::error::Error + 'static)> {
        match self.source_impl.as_ref() {
            Some(Suberror::Constituent(c)) => Some(&**c),
            _ => None,
        }
    }
}

impl std::error::Error for OurError {
    fn source(&self) -> Option<&(std::error::Error + 'static)> {
        self.source_impl.as_ref().map(|c| match c {
            Suberror::Previous(c) => &**c,
            Suberror::Constituent(c) => &**c,
        })
    }

    // deprecated in 1.33.0
    // identical to source()
    fn cause(&self) -> Option<&std::error::Error> {
        self.source_impl.as_ref().map(|c| match c {
            Suberror::Previous(c) => &**c,
            Suberror::Constituent(c) => &**c,
        })
    }
}

// Display only the message associated w/ the specifics.
// Consider the rest to be management baggage.
impl std::fmt::Display for OurError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.specifics)
    }
}

fn b() -> Result<(), OurError> {
    let err = std::io::Error::new(std::io::ErrorKind::Other, "oh no!");
    let mut ours = OurError::new(OurErrorKind::ContextInitError);
    ours.set_constituent(Box::new(err));
    Err(ours)
}

fn c() -> Result<(), OurError> {
    b()
}

fn d() -> Result<(), OurError> {
    Err(c()
        .expect_err("")
        .set_extension(OurError::new(OurErrorKind::InvalidArgument {
            description: "32".into(),
        }))
        .set_subsequent(OurError::new(OurErrorKind::IoctlResultTooLarge)))
}

fn main() {
    let err = d().expect_err("");

    // We can downcast the source to OurError, since we know it is.
    assert_eq!(
        err.source()
            .expect("")
            .downcast_ref::<OurError>()
            .expect("")
            .specifics,
        OurErrorKind::InvalidArgument {
            description: "32".into()
        }
    );

    // Downcasting to std::io::Error will result in None
    assert!(err
        .source()
        .expect("")
        .downcast_ref::<std::io::Error>()
        .is_none());

    // The source is a previous error
    assert_eq!(
        err.previous()
            .expect("")
            .downcast_ref::<OurError>()
            .expect("")
            .specifics,
        err.source()
            .expect("")
            .downcast_ref::<OurError>()
            .expect("")
            .specifics
    );

    // Since the source is a previous error, it can not be a constituent
    // error.
    assert!(err.constituent().is_none());

    println!("The error's debug representation: {:?}", err);
    println!("");
    println!("Just the error: {}", err);
    println!("");
    print!(
        "Just this error's backtrace: {:?}",
        err.our_backtrace().expect("")
    );
}
