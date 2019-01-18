#[macro_use]
extern crate log;
#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate error_chain;

mod error;
#[macro_use]
mod topic;
#[macro_use]
mod proto;
mod packet;
mod codec;
mod transport;

pub use crate::proto::{QoS, Protocol};
pub use crate::topic::{Level, Topic, TopicTree, MatchTopic};
pub use crate::packet::{Packet, LastWill, Connect, ConnectReturnCode, SubscribeReturnCode};
pub use crate::codec::Codec;
pub use crate::error::*;
pub use crate::transport::{Connection, Delivery};

// http://www.iana.org/assignments/service-names-port-numbers/service-names-port-numbers.xhtml
pub const TCP_PORT: u16 = 1883;
pub const SSL_PORT: u16 = 8883;