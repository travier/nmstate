#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ErrorKind {
    InvalidArgument,
    PluginFailure,
    Bug,
    VerificationError,
    NotImplementedError,
    KernelIntegerRoundedError,
}

impl std::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::fmt::Display for NmstateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.msg)
    }
}

#[derive(Debug)]
pub struct NmstateError {
    kind: ErrorKind,
    msg: String,
}

impl NmstateError {
    pub fn new(kind: ErrorKind, msg: String) -> Self {
        Self { kind, msg }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn msg(&self) -> &str {
        self.msg.as_str()
    }
}

impl From<serde_json::Error> for NmstateError {
    fn from(e: serde_json::Error) -> Self {
        NmstateError::new(ErrorKind::Bug, format!("serde_json::Error: {}", e))
    }
}

impl From<std::net::AddrParseError> for NmstateError {
    fn from(e: std::net::AddrParseError) -> Self {
        NmstateError::new(
            ErrorKind::InvalidArgument,
            format!("Invalid IP address : {}", e),
        )
    }
}
