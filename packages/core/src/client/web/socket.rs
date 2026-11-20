//! Websocket clients for browsers and Workers, using the global `WebSocket`: [`Feed`] runs a
//! [`Session`] on the market data socket and returns [`Event`]s, and [`OrderSocket`] sends signed
//! requests on the order entry socket.
//!
//! Everything runs on the single JS thread: socket events and timers update a shared driver, and
//! the futures here poll it.

use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    future::poll_fn,
    rc::{Rc, Weak},
    task::{Poll, Waker},
};

use js_sys::{Array, Function, Math, Reflect};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_sys::{CloseEvent, MessageEvent, WebSocket};

use crate::client::web::clock;
use crate::client::{OFFLINE, Refusal, Reply, Signed};
use crate::live::orders::{self, OrderAnswer, OrderSocketError, PING_EVERY_MS};
use crate::live::{Channel, Event, FeedError, Retry, Session, Tick};
use crate::trade::cancel::{CancelBody, MassCancelBody, MassCancelled};
use crate::trade::order::{Order, Placed};
use crate::trade::orders::PlaceBody;

/// Maximum delay `setTimeout` accepts, in ms.
const LONGEST_DELAY: u32 = 0x7fff_ffff;

/// Opens a socket to `url` with the global `WebSocket`.
fn connect(url: &str) -> Result<WebSocket, String> {
    let global: JsValue = js_sys::global().into();
    let class = function(&global, "WebSocket").ok_or_else(|| "this runtime has no WebSocket".to_owned())?;
    let socket = Reflect::construct(&class, &Array::of1(&JsValue::from_str(url)))
        .map_err(|error| said(&error).unwrap_or_else(|| "the socket could not be made".to_owned()))?;
    Ok(socket.unchecked_into())
}

fn function(global: &JsValue, name: &str) -> Option<Function> {
    Reflect::get(global, &JsValue::from_str(name)).ok()?.dyn_into().ok()
}

fn said(error: &JsValue) -> Option<String> {
    let message = Reflect::get(error, &JsValue::from_str("message")).ok()?.as_string()?;
    (!message.is_empty()).then_some(message)
}

fn random() -> u64 {
    let bits = Math::random().to_bits().wrapping_mul(0x9e37_79b9_7f4a_7c15);
    bits ^ (bits >> 29) ^ clock().rotate_left(29)
}

/// `setTimeout` and `clearTimeout`, looked up once.
struct Timers {
    global: JsValue,
    set: Function,
    clear: Function,
    pending: Option<JsValue>,
}

impl Timers {
    fn new() -> Option<Timers> {
        let global: JsValue = js_sys::global().into();
        let set = function(&global, "setTimeout")?;
        let clear = function(&global, "clearTimeout")?;
        Some(Timers { global, set, clear, pending: None })
    }

    fn arm(&mut self, ms: u64, callback: &Closure<dyn FnMut()>) -> bool {
        self.disarm();
        let delay = JsValue::from(u32::try_from(ms).map_or(LONGEST_DELAY, |ms| ms.min(LONGEST_DELAY)));
        match self.set.call2(&self.global, callback.as_ref(), &delay) {
            Ok(timer) => {
                self.pending = Some(timer);
                true
            }
            Err(_) => false,
        }
    }

    fn disarm(&mut self) {
        if let Some(timer) = self.pending.take() {
            let _ = self.clear.call1(&self.global, &timer);
        }
    }
}

/// Event handler closures for a socket and a timer. Each holds a weak reference to its driver.
struct Calls {
    open: Closure<dyn FnMut()>,
    message: Closure<dyn FnMut(MessageEvent)>,
    close: Closure<dyn FnMut(CloseEvent)>,
    error: Closure<dyn FnMut(JsValue)>,
    wake: Closure<dyn FnMut()>,
}

trait Driven: 'static {
    fn opened(&mut self);
    fn received(&mut self, text: String);
    fn lost(&mut self, reason: String);
    fn woke(&mut self);
    /// Wakers to call after the driver borrow is released.
    fn wakeable(&mut self) -> Vec<Waker>;
}

fn on<D: Driven>(driver: &Weak<RefCell<D>>, act: impl FnOnce(&mut D)) {
    let Some(driver) = driver.upgrade() else { return };
    let wakers = {
        let Ok(mut driver) = driver.try_borrow_mut() else { return };
        act(&mut driver);
        driver.wakeable()
    };
    for waker in wakers {
        waker.wake();
    }
}

impl Calls {
    fn new<D: Driven>(driver: &Weak<RefCell<D>>) -> Calls {
        let (open, message, close, error, wake) =
            (driver.clone(), driver.clone(), driver.clone(), driver.clone(), driver.clone());
        Calls {
            open: Closure::new(move || on(&open, D::opened)),
            message: Closure::new(move |event: MessageEvent| {
                if let Some(text) = event.data().as_string() {
                    on(&message, |driver| driver.received(text));
                }
            }),
            close: Closure::new(move |event: CloseEvent| {
                let reason = match event.reason() {
                    reason if reason.is_empty() => format!("the socket closed ({})", event.code()),
                    reason => format!("{} {reason}", event.code()),
                };
                on(&close, |driver| driver.lost(reason));
            }),
            error: Closure::new(move |event: JsValue| {
                let reason = said(&event).unwrap_or_else(|| "the socket failed".to_owned());
                on(&error, |driver| driver.lost(reason));
            }),
            wake: Closure::new(move || on(&wake, D::woke)),
        }
    }

    fn attach(&self, socket: &WebSocket) {
        socket.set_onopen(Some(self.open.as_ref().unchecked_ref()));
        socket.set_onmessage(Some(self.message.as_ref().unchecked_ref()));
        socket.set_onclose(Some(self.close.as_ref().unchecked_ref()));
        socket.set_onerror(Some(self.error.as_ref().unchecked_ref()));
    }
}

/// Removes `socket`'s handlers and closes it, so no queued events are delivered.
fn hang_up(socket: Option<WebSocket>) {
    if let Some(socket) = socket {
        socket.set_onopen(None);
        socket.set_onmessage(None);
        socket.set_onclose(None);
        socket.set_onerror(None);
        let _ = socket.close();
    }
}

/// A market data websocket connection that reconnects after a disconnect. Dropping it closes the
/// connection.
pub struct Feed {
    driver: Rc<RefCell<FeedDriver>>,
}

struct FeedDriver {
    session: Session,
    timers: Option<Timers>,
    socket: Option<WebSocket>,
    calls: Option<Calls>,
    events: VecDeque<Event>,
    waker: Option<Waker>,
    stopped: bool,
}

impl Feed {
    #[must_use]
    pub fn start(session: Session) -> Feed {
        let driver = Rc::new(RefCell::new(FeedDriver {
            session,
            timers: Timers::new(),
            socket: None,
            calls: None,
            events: VecDeque::new(),
            waker: None,
            stopped: false,
        }));
        let calls = Calls::new(&Rc::downgrade(&driver));
        if let Ok(mut held) = driver.try_borrow_mut() {
            held.calls = Some(calls);
            match held.timers.is_some() {
                true => held.dial(),
                false => held.stop(),
            }
        }
        Feed { driver }
    }

    /// Subscribes to `channel`, including after reconnects.
    pub async fn subscribe(&self, channel: Channel) -> Result<(), FeedError> {
        self.with(|driver| {
            driver.session.subscribe(channel)?;
            driver.flush();
            Ok(())
        })
    }

    pub fn unsubscribe(&self, channel: Channel) -> Result<(), FeedError> {
        self.with(|driver| {
            driver.session.unsubscribe(&channel);
            driver.flush();
            Ok(())
        })
    }

    /// The next event; `None` once the feed has stopped.
    pub async fn next(&mut self) -> Option<Event> {
        poll_fn(|cx| {
            let Ok(mut driver) = self.driver.try_borrow_mut() else {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            };
            if let Some(event) = driver.events.pop_front() {
                return Poll::Ready(Some(event));
            }
            if driver.stopped {
                return Poll::Ready(None);
            }
            driver.waker = Some(cx.waker().clone());
            Poll::Pending
        })
        .await
    }

    pub fn close(self) {
        drop(self);
    }

    /// Returns a handle that stops the feed from elsewhere, such as a component's cleanup, while a
    /// task waits in [`Feed::next`], which then returns `None`.
    #[must_use]
    pub fn stopper(&self) -> Stopper {
        Stopper { driver: Rc::downgrade(&self.driver) }
    }

    fn with<T>(&self, act: impl FnOnce(&mut FeedDriver) -> Result<T, FeedError>) -> Result<T, FeedError> {
        let mut driver = self.driver.try_borrow_mut().map_err(|_| FeedError::Stopped)?;
        if driver.stopped {
            return Err(FeedError::Stopped);
        }
        act(&mut driver)
    }
}

/// Stops a [`Feed`] from outside the task that reads it.
#[derive(Clone)]
pub struct Stopper {
    driver: Weak<RefCell<FeedDriver>>,
}

impl Stopper {
    pub fn stop(&self) {
        on(&self.driver, FeedDriver::stop);
    }
}

impl Drop for Feed {
    fn drop(&mut self) {
        if let Ok(mut driver) = self.driver.try_borrow_mut() {
            driver.stop();
        }
    }
}

impl FeedDriver {
    fn dial(&mut self) {
        match connect(self.session.url()) {
            Ok(socket) => {
                if let Some(calls) = &self.calls {
                    calls.attach(&socket);
                }
                self.socket = Some(socket);
            }
            Err(reason) => self.lost(reason),
        }
    }

    fn send(&self, frame: &str) -> Result<(), String> {
        let Some(socket) = &self.socket else { return Err("no socket to send on".to_owned()) };
        socket.send_with_str(frame).map_err(|error| said(&error).unwrap_or_else(|| "the send failed".to_owned()))
    }

    fn flush(&mut self) {
        for frame in self.session.owed(clock()) {
            if let Err(reason) = self.send(&frame) {
                return self.lost(reason);
            }
        }
        self.arm_tick();
    }

    fn arm_tick(&mut self) {
        if let Some(at) = self.session.due() {
            self.arm(at.saturating_sub(clock()));
        }
    }

    fn arm(&mut self, ms: u64) {
        let armed = match (&mut self.timers, &self.calls) {
            (Some(timers), Some(calls)) => timers.arm(ms, &calls.wake),
            _ => false,
        };
        if !armed {
            self.stop();
        }
    }

    fn reconnect(&mut self, reason: String, retry: Retry) {
        let after = retry.after_ms;
        self.events.push_back(Event::Closed { reason, retry });
        self.arm(after);
    }

    fn stop(&mut self) {
        hang_up(self.socket.take());
        if let Some(timers) = &mut self.timers {
            timers.disarm();
        }
        self.stopped = true;
    }
}

impl Driven for FeedDriver {
    fn opened(&mut self) {
        for frame in self.session.opened(clock()) {
            if let Err(reason) = self.send(&frame) {
                return self.lost(reason);
            }
        }
        self.events.push_back(Event::Opened);
        self.arm_tick();
    }

    fn received(&mut self, text: String) {
        let event = match self.session.received(clock(), &text) {
            Ok(frame) => Event::Frame(Box::new(frame)),
            Err(bad) => Event::Unreadable(bad),
        };
        self.events.push_back(event);
    }

    fn lost(&mut self, reason: String) {
        hang_up(self.socket.take());
        let retry = self.session.closed(clock(), random());
        self.reconnect(reason, retry);
    }

    fn woke(&mut self) {
        if self.stopped {
            return;
        }
        if self.session.is_open() {
            match self.session.tick(clock()) {
                Tick::Wait => self.arm_tick(),
                Tick::Send => self.flush(),
                Tick::Reconnect => self.lost("nothing arrived for too long".to_owned()),
            }
        } else if self.socket.is_none() {
            self.dial();
        }
    }

    fn wakeable(&mut self) -> Vec<Waker> {
        match self.events.is_empty() && !self.stopped {
            true => Vec::new(),
            false => self.waker.take().into_iter().collect(),
        }
    }
}

type Slot = Rc<RefCell<(Option<Result<OrderAnswer, OrderSocketError>>, Option<Waker>)>>;

/// The order entry websocket. It connects on first use and again after a disconnect, and matches
/// responses to requests by id.
pub struct OrderSocket {
    driver: Rc<RefCell<OrderDriver>>,
}

struct OrderDriver {
    url: String,
    timers: Option<Timers>,
    socket: Option<WebSocket>,
    open: bool,
    calls: Option<Calls>,
    next_id: u64,
    /// Frames waiting for the socket to open.
    queued: VecDeque<(String, String)>,
    pending: HashMap<String, Slot>,
    ready: Vec<Waker>,
}

impl OrderSocket {
    #[must_use]
    pub fn start(url: impl Into<String>) -> OrderSocket {
        let driver = Rc::new(RefCell::new(OrderDriver {
            url: url.into(),
            timers: Timers::new(),
            socket: None,
            open: false,
            calls: None,
            next_id: 1,
            queued: VecDeque::new(),
            pending: HashMap::new(),
            ready: Vec::new(),
        }));
        let calls = Calls::new(&Rc::downgrade(&driver));
        if let Ok(mut held) = driver.try_borrow_mut() {
            held.calls = Some(calls);
        }
        OrderSocket { driver }
    }

    async fn request(&self, frame: impl FnOnce(&str) -> String) -> Result<OrderAnswer, OrderSocketError> {
        if OFFLINE {
            return Err(OrderSocketError::Offline);
        }
        let slot: Slot = Rc::new(RefCell::new((None, None)));
        {
            let mut driver = self.driver.try_borrow_mut().map_err(|_| OrderSocketError::Stopped)?;
            let id = driver.next_id.to_string();
            driver.next_id += 1;
            let frame = frame(&id);
            if frame.len() > orders::MAX_FRAME_BYTES {
                return Err(OrderSocketError::Invalid("a frame is at most 64 KiB".to_owned()));
            }
            driver.pending.insert(id.clone(), slot.clone());
            driver.queued.push_back((id, frame));
            driver.pump();
        }
        poll_fn(|cx| {
            let Ok(mut held) = slot.try_borrow_mut() else {
                cx.waker().wake_by_ref();
                return Poll::Pending;
            };
            match held.0.take() {
                Some(answer) => Poll::Ready(answer),
                None => {
                    held.1 = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
        })
        .await
    }

    pub async fn place(
        &self,
        orders: &[Signed<PlaceBody>],
    ) -> Result<Vec<Result<Reply<Placed>, Refusal>>, OrderSocketError> {
        match self.request(|id| orders::place_frame(id, orders)).await? {
            OrderAnswer::Place { results, .. } => Ok(results),
            other => Err(unexpected(other)),
        }
    }

    pub async fn cancel(
        &self,
        cancels: &[Signed<CancelBody>],
    ) -> Result<Vec<Result<Reply<Order>, Refusal>>, OrderSocketError> {
        match self.request(|id| orders::cancel_frame(id, cancels)).await? {
            OrderAnswer::Cancel { results, .. } => Ok(results),
            other => Err(unexpected(other)),
        }
    }

    pub async fn mass_cancel(
        &self,
        mass_cancels: &[Signed<MassCancelBody>],
    ) -> Result<Vec<Result<Reply<MassCancelled>, Refusal>>, OrderSocketError> {
        match self.request(|id| orders::mass_cancel_frame(id, mass_cancels)).await? {
            OrderAnswer::MassCancel { results, .. } => Ok(results),
            other => Err(unexpected(other)),
        }
    }
}

impl Drop for OrderSocket {
    fn drop(&mut self) {
        if let Ok(mut driver) = self.driver.try_borrow_mut() {
            driver.fail_all(&OrderSocketError::Stopped);
            hang_up(driver.socket.take());
            if let Some(timers) = &mut driver.timers {
                timers.disarm();
            }
        }
    }
}

fn unexpected(answer: OrderAnswer) -> OrderSocketError {
    match answer {
        OrderAnswer::Rejected { code, message, .. } => OrderSocketError::Rejected { code, message },
        other => OrderSocketError::Invalid(format!("an answer of another kind: {other:?}")),
    }
}

impl OrderDriver {
    /// Sends queued frames if the socket is open, and connects if there is no socket.
    fn pump(&mut self) {
        if self.socket.is_none() {
            match connect(&self.url) {
                Ok(socket) => {
                    if let Some(calls) = &self.calls {
                        calls.attach(&socket);
                    }
                    self.socket = Some(socket);
                }
                Err(reason) => return self.fail_all(&OrderSocketError::Unreachable(reason)),
            }
        }
        if !self.open {
            return;
        }
        let in_flight = self.pending.len().saturating_sub(self.queued.len());
        let mut room = orders::MAX_IN_FLIGHT.saturating_sub(in_flight);
        while room > 0 {
            let Some((id, frame)) = self.queued.pop_front() else { break };
            let sent = self.socket.as_ref().map(|socket| socket.send_with_str(&frame));
            if !matches!(sent, Some(Ok(()))) {
                self.answer(&id, Err(OrderSocketError::Unreachable("the send failed".to_owned())));
                continue;
            }
            room -= 1;
        }
        self.arm_ping();
    }

    fn arm_ping(&mut self) {
        if let (Some(timers), Some(calls)) = (&mut self.timers, &self.calls) {
            timers.arm(PING_EVERY_MS, &calls.wake);
        }
    }

    fn answer(&mut self, id: &str, answer: Result<OrderAnswer, OrderSocketError>) {
        if let Some(slot) = self.pending.remove(id)
            && let Ok(mut held) = slot.try_borrow_mut()
        {
            held.0 = Some(answer);
            if let Some(waker) = held.1.take() {
                self.ready.push(waker);
            }
        }
    }

    fn fail_all(&mut self, error: &OrderSocketError) {
        let queued: Vec<String> = self.queued.drain(..).map(|(id, _)| id).collect();
        let ids: Vec<String> = self.pending.keys().cloned().collect();
        for id in ids {
            // Queued frames were never sent; the other requests may have been executed.
            let answer = match (queued.contains(&id), error) {
                (true, OrderSocketError::Lost(reason)) => OrderSocketError::Unreachable(reason.clone()),
                _ => error.clone(),
            };
            self.answer(&id, Err(answer));
        }
    }
}

impl Driven for OrderDriver {
    fn opened(&mut self) {
        self.open = true;
        self.pump();
    }

    fn received(&mut self, text: String) {
        match orders::decode_answer(&text) {
            Ok(OrderAnswer::Pong) | Err(_) => {}
            Ok(answer) => {
                if let Some(id) = answer.id().map(str::to_owned) {
                    self.answer(&id, Ok(answer));
                }
            }
        }
    }

    fn lost(&mut self, reason: String) {
        hang_up(self.socket.take());
        self.open = false;
        if let Some(timers) = &mut self.timers {
            timers.disarm();
        }
        self.fail_all(&OrderSocketError::Lost(reason));
    }

    fn woke(&mut self) {
        if self.open {
            if let Some(socket) = &self.socket {
                let _ = socket.send_with_str(orders::PING);
            }
            self.arm_ping();
        }
    }

    fn wakeable(&mut self) -> Vec<Waker> {
        std::mem::take(&mut self.ready)
    }
}
