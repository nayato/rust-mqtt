use futures::channel::oneshot::Canceled;
use derive_more::From;

#[derive(Debug)]
pub enum DecodeError {
    InvalidProtocol,
    InvalidLength,
    UnsupportedProtocolLevel,
    ConnectReservedFlagSet,
    ConnAckReservedFlagSet,
    InvalidClientId,
    UnsupportedPacketType,
}

#[derive(Debug, From)]
pub enum Error {
    Fmt(::std::fmt::Error),
    Io(::std::io::Error),
    Canceled(Canceled),
    Utf8(::std::str::Utf8Error),
    ConnectionGone(::futures::channel::mpsc::TrySendError<crate::packet::Packet>),
    DecodeError(DecodeError),
    OutOfMemory,
    InvalidState,
    InvalidPacket,
    InvalidTopic,
    SpawnError,
    Other(String),
}

impl<'a> From<&'a str> for Error {
    fn from(v: &'a str) -> Error {
        Error::Other(v.to_owned())
    }
}
