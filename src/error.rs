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
    Canceled(::futures::Canceled),
    Utf8(::std::str::Utf8Error),
    ConnectionGone(::futures::unsync::mpsc::SendError<crate::packet::Packet>),
    DecodeError(DecodeError),
    OutOfMemory,
    InvalidState,
    InvalidPacket,
    InvalidTopic,
    SpawnError,
    Other(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;
