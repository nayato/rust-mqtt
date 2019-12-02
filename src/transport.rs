use crate::codec::Codec;
use crate::error::*;
use crate::packet::{Connect, ConnectReturnCode, Packet};
use crate::proto::{Protocol, QoS};
use bytes::Bytes;
use futures::channel::{mpsc, oneshot};
use futures::{future, Future, FutureExt, SinkExt, StreamExt, TryStreamExt};
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use string::String;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Decoder, Framed};

type InnerRef = Rc<RefCell<ConnectionInner>>;
type DeliveryResponse = ();
type DeliveryPromise = oneshot::Sender<Result<DeliveryResponse, Error>>;

pub struct Connection {
    inner: InnerRef,
}

pub struct ConnectionInner {
    write_sender: mpsc::UnboundedSender<Packet>,
    pending_ack: VecDeque<(u16, DeliveryPromise)>,
    pending_sub_acks: VecDeque<(u16, DeliveryPromise)>,
    next_packet_id: u16,
}

pub enum Delivery {
    Resolved(Result<DeliveryResponse, Error>),
    Pending(oneshot::Receiver<Result<DeliveryResponse, Error>>),
    Gone,
}

impl Connection {
    pub async fn open<T: AsyncRead + AsyncWrite + Send + Unpin + 'static>(
        client_id: String<Bytes>,
        username: Option<String<Bytes>>,
        password: Option<Bytes>,
        io: T,
    ) -> Result<(Connection, impl Future<Output = Result<(), Error>>), Error> {
        let mut io = Codec::new().framed(io);
        let connect = Connect {
            protocol: Protocol::default(),
            clean_session: false,
            keep_alive: 0,
            last_will: None,
            client_id: client_id,
            username,
            password,
        };
        io.send(Packet::Connect {
            connect: Box::new(connect),
        })
        .await?;
        if let Some(packet_result) = io.next().await {
            if let Packet::ConnectAck { return_code, .. } = packet_result? {
                if let ConnectReturnCode::ConnectionAccepted = return_code {
                    // todo: surface session_present
                    Ok(Connection::new(io))
                } else {
                    Err(return_code.reason().into())
                }
            } else {
                Err("Protocol violation: expected CONNACK".into())
            }
        } else {
            Err("Connection is closed.".into())
        }
    }

    fn new<T: AsyncRead + AsyncWrite + 'static>(
        io: Framed<T, Codec>,
    ) -> (Connection, impl Future<Output = Result<(), Error>>) {
        let (mut writer, reader) = io.split();
        let (tx, rx) = mpsc::unbounded();
        let connection = Rc::new(RefCell::new(ConnectionInner::new(tx)));
        let reader_conn = connection.clone();
        let drive_fut = async move {
            let mut rx = rx.map(|i| Ok(i));
            let read_handling = reader.try_for_each(move |packet| {
                reader_conn.borrow_mut().handle_packet(packet);
                future::ok(())
            });
            futures::select! {
                read = read_handling.fuse() => read,
                send = writer.send_all(&mut rx).fuse() => send,
            }
        };
        (Connection { inner: connection }, drive_fut)
    }

    pub fn send(&self, qos: QoS, topic: String<Bytes>, payload: Bytes) -> Delivery {
        if qos == QoS::ExactlyOnce {
            return Delivery::Resolved(Err("QoS 2 is not supported at this time.".into()));
        }
        self.inner.borrow_mut().send(qos, topic, payload)
    }

    pub fn subscribe(&mut self, topic_filters: Vec<(String<Bytes>, QoS)>) -> Delivery {
        self.inner.borrow_mut().subscribe(topic_filters)
    }

    pub fn close(self) -> impl Future<Output = Result<(), Error>> {
        future::ok(())
    }
}

impl ConnectionInner {
    pub fn new(sender: mpsc::UnboundedSender<Packet>) -> ConnectionInner {
        ConnectionInner {
            write_sender: sender,
            pending_ack: VecDeque::new(),
            pending_sub_acks: VecDeque::new(),
            next_packet_id: 1,
        }
    }

    pub fn post_packet(
        &self,
        packet: Packet,
    ) -> std::result::Result<(), mpsc::TrySendError<Packet>> {
        self.write_sender.unbounded_send(packet)
    }

    pub fn handle_packet(&mut self, packet: Packet) {
        match packet {
            Packet::PublishAck { packet_id } => {
                if let Some(pending) = self.pending_ack.pop_front() {
                    if pending.0 != packet_id {
                        println!("protocol violation");
                        // todo: handle protocol violation
                    }
                    pending.1.send(Ok(()));
                } else {
                    // todo: handle protocol violation
                }
            }
            Packet::SubscribeAck { packet_id, status } => {
                if let Some(pending) = self.pending_sub_acks.pop_front() {
                    if pending.0 != packet_id {
                        // todo: handle protocol violation
                    }
                    pending.1.send(Ok(())); // todo: surface subscribe outcome
                } else {
                    // todo: handle protocol violation
                }
            }
            Packet::UnsubscribeAck { packet_id } => {} // todo
            Packet::PingResponse => {}                 // todo
            _ => {
                // todo: handle protocol violation
            }
        }
    }

    pub fn send(&mut self, qos: QoS, topic: String<Bytes>, payload: Bytes) -> Delivery {
        if qos == QoS::AtMostOnce {
            let publish = Packet::Publish {
                dup: false,
                retain: false,
                qos,
                topic,
                packet_id: None,
                payload,
            };
            if let Err(e) = self.post_packet(publish) {
                return Delivery::Resolved(Err(e.into()));
            }
            Delivery::Resolved(Ok(())) // todo: delay until handed out to network successfully
        } else {
            let packet_id = self.next_packet_id();
            let publish = Packet::Publish {
                dup: false,
                retain: false,
                qos,
                topic,
                packet_id: Some(packet_id),
                payload,
            };
            let (delivery_tx, delivery_rx) = oneshot::channel();
            if let Err(e) = self.post_packet(publish) {
                return Delivery::Resolved(Err(e.into()));
            }
            self.pending_ack.push_back((packet_id, delivery_tx));
            Delivery::Pending(delivery_rx)
        }
    }

    pub fn subscribe(&mut self, topic_filters: Vec<(String<Bytes>, QoS)>) -> Delivery {
        let packet_id = self.next_packet_id();
        let subscribe = Packet::Subscribe {
            packet_id,
            topic_filters,
        };
        self.post_packet(subscribe);
        let (delivery_tx, delivery_rx) = oneshot::channel();
        self.pending_sub_acks.push_back((packet_id, delivery_tx));
        Delivery::Pending(delivery_rx)
    }

    fn next_packet_id(&mut self) -> u16 {
        // todo: simple wrapping may not work for everything, think about honoring known in-flights
        let packet_id = self.next_packet_id;
        if packet_id == ::std::u16::MAX {
            self.next_packet_id = 1;
        } else {
            self.next_packet_id = packet_id + 1;
        }
        packet_id
    }
}

impl Future for Delivery {
    type Output = Result<DeliveryResponse, Error>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        unsafe {
            let this = self.get_unchecked_mut();
            if let Delivery::Pending(ref mut receiver) = *this {
                return Poll::Ready(
                    futures::ready!(Pin::new_unchecked(receiver).poll(cx))?.map(|state| state),
                );
            }

            let old_v = ::std::mem::replace(this, Delivery::Gone);
            if let Delivery::Resolved(r) = old_v {
                return match r {
                    Ok(state) => Poll::Ready(Ok(state)),
                    Err(e) => Poll::Ready(Err(e)),
                };
            }
        }
        panic!("Delivery was polled already.");
    }
}
