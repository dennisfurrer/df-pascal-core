//! Websocket clients for native programs, using tokio-tungstenite: [`Feed`] runs a [`Session`]
//! on the market data socket and returns [`Event`]s, and [`OrderSocket`] sends signed requests on
//! the order entry socket.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use tokio::{
    net::TcpStream,
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::{Instant, sleep_until},
};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

use crate::client::OFFLINE as OFFLINE_WRITES;
use crate::client::native::clock;
use crate::client::{Refusal, Reply, Signed};
use crate::live::orders::{self, OrderAnswer, OrderSocketError, PING_EVERY_MS};
use crate::live::{Channel, Event, FeedError, Session, Tick};
use crate::trade::cancel::{CancelBody, MassCancelBody, MassCancelled};
use crate::trade::order::{Order, Placed};
use crate::trade::orders::PlaceBody;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

enum Command {
    Subscribe(Channel, oneshot::Sender<Result<(), FeedError>>),
    Unsubscribe(Channel),
}

/// A market data websocket connection. It reconnects when the connection drops or goes silent,
/// and sends subscriptions made while disconnected on the next connection. Dropping it closes the
/// connection.
pub struct Feed {
    commands: mpsc::UnboundedSender<Command>,
    events: mpsc::UnboundedReceiver<Event>,
    task: JoinHandle<()>,
}

impl Feed {
    /// Connects to the session's URL on the current tokio runtime.
    #[must_use]
    pub fn start(session: Session) -> Feed {
        let (commands, inbox) = mpsc::unbounded_channel();
        let (outbox, events) = mpsc::unbounded_channel();
        let task = tokio::spawn(run(session, inbox, outbox));
        Feed { commands, events, task }
    }

    /// Subscribes to `channel`, including after reconnects.
    pub async fn subscribe(&self, channel: Channel) -> Result<(), FeedError> {
        let (reply, answer) = oneshot::channel();
        self.commands.send(Command::Subscribe(channel, reply)).map_err(|_| FeedError::Stopped)?;
        answer.await.map_err(|_| FeedError::Stopped)?
    }

    pub fn unsubscribe(&self, channel: Channel) -> Result<(), FeedError> {
        self.commands.send(Command::Unsubscribe(channel)).map_err(|_| FeedError::Stopped)
    }

    /// The next event; `None` once the feed has stopped.
    pub async fn next(&mut self) -> Option<Event> {
        self.events.recv().await
    }

    pub fn close(self) {
        drop(self);
    }
}

impl Drop for Feed {
    fn drop(&mut self) {
        self.task.abort();
    }
}

enum Outcome {
    Lost(String),
    Stop,
}

async fn run(
    mut session: Session,
    mut commands: mpsc::UnboundedReceiver<Command>,
    events: mpsc::UnboundedSender<Event>,
) {
    loop {
        let url = session.url().to_owned();
        let dial = connect_async(url.as_str());
        tokio::pin!(dial);
        let dialled = loop {
            tokio::select! {
                dialled = &mut dial => break dialled,
                command = commands.recv() => match command {
                    Some(command) => handle(&mut session, command),
                    None => return,
                },
            }
        };
        let reason = match dialled {
            Ok((mut socket, _)) => match open(&mut socket, &mut session, &mut commands, &events).await {
                Outcome::Lost(reason) => reason,
                Outcome::Stop => return,
            },
            Err(error) => error.to_string(),
        };
        let retry = session.closed(clock(), random());
        let until = Instant::now() + Duration::from_millis(retry.after_ms);
        if events.send(Event::Closed { reason, retry }).is_err() {
            return;
        }
        loop {
            tokio::select! {
                () = sleep_until(until) => break,
                command = commands.recv() => match command {
                    Some(command) => handle(&mut session, command),
                    None => return,
                },
            }
        }
    }
}

async fn open(
    socket: &mut Socket,
    session: &mut Session,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    events: &mpsc::UnboundedSender<Event>,
) -> Outcome {
    if let Err(reason) = send(socket, session.opened(clock())).await {
        return Outcome::Lost(reason);
    }
    if events.send(Event::Opened).is_err() {
        return Outcome::Stop;
    }
    loop {
        let wake = session.due().map(|at| Instant::now() + Duration::from_millis(at.saturating_sub(clock())));
        tokio::select! {
            message = socket.next() => match message {
                Some(Ok(Message::Text(text))) => {
                    let event = match session.received(clock(), &text) {
                        Ok(frame) => Event::Frame(Box::new(frame)),
                        Err(bad) => Event::Unreadable(bad),
                    };
                    if events.send(event).is_err() {
                        return Outcome::Stop;
                    }
                }
                Some(Ok(Message::Close(frame))) => {
                    return Outcome::Lost(frame.map_or_else(|| "Pascal closed the socket".to_owned(), |frame| format!("{} {}", u16::from(frame.code), frame.reason)));
                }
                Some(Ok(_)) => session.heard(clock()),
                Some(Err(error)) => return Outcome::Lost(error.to_string()),
                None => return Outcome::Lost("the socket ended".to_owned()),
            },
            command = commands.recv() => match command {
                Some(command) => {
                    handle(session, command);
                    if let Err(reason) = send(socket, session.owed(clock())).await {
                        return Outcome::Lost(reason);
                    }
                }
                None => {
                    let _ = socket.close(None).await;
                    return Outcome::Stop;
                }
            },
            () = until(wake) => match session.tick(clock()) {
                Tick::Wait => {}
                Tick::Send => {
                    if let Err(reason) = send(socket, session.owed(clock())).await {
                        return Outcome::Lost(reason);
                    }
                }
                Tick::Reconnect => {
                    let _ = socket.close(None).await;
                    return Outcome::Lost("nothing arrived for too long".to_owned());
                }
            },
        }
    }
}

async fn send(socket: &mut Socket, frames: Vec<String>) -> Result<(), String> {
    for frame in frames {
        socket.send(Message::text(frame)).await.map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn handle(session: &mut Session, command: Command) {
    match command {
        Command::Subscribe(channel, reply) => {
            let _ = reply.send(session.subscribe(channel).map_err(FeedError::from));
        }
        Command::Unsubscribe(channel) => session.unsubscribe(&channel),
    }
}

async fn until(wake: Option<Instant>) {
    match wake {
        Some(at) => sleep_until(at).await,
        None => std::future::pending().await,
    }
}

fn random() -> u64 {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |since| u64::from(since.subsec_nanos()));
    nanos.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ clock().rotate_left(29)
}

type Pending = oneshot::Sender<Result<OrderAnswer, OrderSocketError>>;

struct Outgoing {
    frame: String,
    reply: Pending,
    id: String,
}

/// The order entry websocket. It connects on first use and again after a disconnect, and matches
/// responses to requests by request id. Requests from many tasks share one connection and are
/// sent in the order they are made. Requests in flight when the connection drops return
/// [`OrderSocketError::Lost`] and may have been executed.
pub struct OrderSocket {
    outgoing: mpsc::UnboundedSender<Outgoing>,
    task: JoinHandle<()>,
    next_id: std::sync::atomic::AtomicU64,
}

impl OrderSocket {
    /// Creates the socket. It connects to `url` (the deployment's `trade_ws_url`) on first use.
    #[must_use]
    pub fn start(url: impl Into<String>) -> OrderSocket {
        let (outgoing, inbox) = mpsc::unbounded_channel();
        let task = tokio::spawn(run_orders(url.into(), inbox));
        OrderSocket { outgoing, task, next_id: std::sync::atomic::AtomicU64::new(1) }
    }

    async fn request(&self, frame: impl FnOnce(&str) -> String) -> Result<OrderAnswer, OrderSocketError> {
        if OFFLINE_WRITES {
            return Err(OrderSocketError::Offline);
        }
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed).to_string();
        let frame = frame(&id);
        if frame.len() > orders::MAX_FRAME_BYTES {
            return Err(OrderSocketError::Invalid("a frame is at most 64 KiB".to_owned()));
        }
        let (reply, answer) = oneshot::channel();
        self.outgoing.send(Outgoing { frame, reply, id }).map_err(|_| OrderSocketError::Stopped)?;
        answer.await.map_err(|_| OrderSocketError::Stopped)?
    }

    /// Places up to 50 signed orders and returns one result per order, in order.
    pub async fn place(
        &self,
        orders: &[Signed<PlaceBody>],
    ) -> Result<Vec<Result<Reply<Placed>, Refusal>>, OrderSocketError> {
        check(orders.len(), 50)?;
        match self.request(|id| orders::place_frame(id, orders)).await? {
            OrderAnswer::Place { results, .. } => Ok(results),
            other => Err(unexpected(other)),
        }
    }

    pub async fn cancel(
        &self,
        cancels: &[Signed<CancelBody>],
    ) -> Result<Vec<Result<Reply<Order>, Refusal>>, OrderSocketError> {
        check(cancels.len(), 50)?;
        match self.request(|id| orders::cancel_frame(id, cancels)).await? {
            OrderAnswer::Cancel { results, .. } => Ok(results),
            other => Err(unexpected(other)),
        }
    }

    pub async fn mass_cancel(
        &self,
        mass_cancels: &[Signed<MassCancelBody>],
    ) -> Result<Vec<Result<Reply<MassCancelled>, Refusal>>, OrderSocketError> {
        check(mass_cancels.len(), 10)?;
        match self.request(|id| orders::mass_cancel_frame(id, mass_cancels)).await? {
            OrderAnswer::MassCancel { results, .. } => Ok(results),
            other => Err(unexpected(other)),
        }
    }
}

impl Drop for OrderSocket {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn check(count: usize, most: usize) -> Result<(), OrderSocketError> {
    match count {
        0 => Err(OrderSocketError::Invalid("nothing to send".to_owned())),
        count if count > most => Err(OrderSocketError::Invalid(format!("at most {most} a frame"))),
        _ => Ok(()),
    }
}

fn unexpected(answer: OrderAnswer) -> OrderSocketError {
    match answer {
        OrderAnswer::Rejected { code, message, .. } => OrderSocketError::Rejected { code, message },
        other => OrderSocketError::Invalid(format!("an answer of another kind: {other:?}")),
    }
}

async fn run_orders(url: String, mut inbox: mpsc::UnboundedReceiver<Outgoing>) {
    let mut waiting: Option<Outgoing> = None;
    loop {
        // Connect only when there is a request to send.
        let first = match waiting.take() {
            Some(first) => first,
            None => match inbox.recv().await {
                Some(first) => first,
                None => return,
            },
        };
        let mut socket = match connect_async(url.as_str()).await {
            Ok((socket, _)) => socket,
            Err(error) => {
                let _ = first.reply.send(Err(OrderSocketError::Unreachable(error.to_string())));
                continue;
            }
        };
        let mut pending: HashMap<String, Pending> = HashMap::new();
        let mut queue = std::collections::VecDeque::from([first]);
        let mut last_sent = Instant::now();
        let lost = loop {
            while pending.len() < orders::MAX_IN_FLIGHT {
                let Some(next) = queue.pop_front() else { break };
                if let Err(error) = socket.send(Message::text(next.frame)).await {
                    // The frame was not sent, so it fails as unreachable rather than lost.
                    let _ = next.reply.send(Err(OrderSocketError::Unreachable(error.to_string())));
                    break;
                }
                last_sent = Instant::now();
                pending.insert(next.id, next.reply);
            }
            let ping_at = last_sent + Duration::from_millis(PING_EVERY_MS);
            tokio::select! {
                message = socket.next() => match message {
                    Some(Ok(Message::Text(text))) => match orders::decode_answer(&text) {
                        Ok(OrderAnswer::Pong) => {}
                        Ok(answer) => {
                            if let Some(reply) = answer.id().and_then(|id| pending.remove(id)) {
                                let _ = reply.send(Ok(answer));
                            }
                        }
                        Err(_) => {}
                    },
                    Some(Ok(Message::Close(frame))) => break frame.map_or_else(|| "closed".to_owned(), |frame| format!("{} {}", u16::from(frame.code), frame.reason)),
                    Some(Ok(_)) => {}
                    Some(Err(error)) => break error.to_string(),
                    None => break "the socket ended".to_owned(),
                },
                next = inbox.recv() => match next {
                    Some(next) => queue.push_back(next),
                    None => {
                        let _ = socket.close(None).await;
                        return;
                    }
                },
                () = sleep_until(ping_at) => {
                    if let Err(error) = socket.send(Message::text(orders::PING)).await {
                        break error.to_string();
                    }
                    last_sent = Instant::now();
                },
            }
        };
        for (_, reply) in pending.drain() {
            let _ = reply.send(Err(OrderSocketError::Lost(lost.clone())));
        }
        // The first unsent request is kept for the next connection; the rest fail as unreachable.
        waiting = queue.pop_front();
        for rest in queue {
            let _ = rest.reply.send(Err(OrderSocketError::Unreachable(lost.clone())));
        }
    }
}
