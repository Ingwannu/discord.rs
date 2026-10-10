#[cfg(feature = "collectors")]
use std::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "collectors")]
use std::sync::Arc;
#[cfg(feature = "collectors")]
use std::time::Duration;

#[cfg(feature = "collectors")]
use tokio::sync::{broadcast, watch};
#[cfg(feature = "collectors")]
use tokio::time;

#[cfg(feature = "collectors")]
use crate::event::{Event, ReactionEvent};
#[cfg(feature = "collectors")]
use crate::model::{ComponentInteraction, Interaction, Message, ModalSubmitInteraction, Snowflake};
#[cfg(feature = "collectors")]
use crate::types::Emoji;

#[cfg(feature = "collectors")]
type EventFilter<T> = Arc<dyn Fn(&T) -> bool + Send + Sync>;

#[cfg(feature = "collectors")]
/// Default reason recorded when a collector is stopped without an explicit
/// reason, mirroring discord.js's `collector.stop()` default of `"user"`.
const DEFAULT_STOP_REASON: &str = "user";

#[cfg(feature = "collectors")]
/// Default number of events the [`CollectorHub`] broadcast ring buffers per
/// subscriber before the oldest are overwritten.
const DEFAULT_HUB_CAPACITY: usize = 256;

#[cfg(feature = "collectors")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
/// Why a collector ended collection, mirroring discord.js's `Collector#endReason`.
///
/// Marked `#[non_exhaustive]` because discord.js keeps adding end reasons
/// (the reaction collector alone added four); match with a `_` arm.
pub enum CollectorEndReason {
    /// The configured `max_items` limit was reached (discord.js `"limit"`).
    Limit,
    /// A reaction collector reached `max_emojis` distinct emojis
    /// (discord.js `"emojiLimit"`).
    EmojiLimit,
    /// A reaction collector reached `max_users` distinct users
    /// (discord.js `"userLimit"`).
    UserLimit,
    /// The overall `timeout` window elapsed (discord.js `"time"`).
    Time,
    /// No matching item arrived within the `idle` window (discord.js `"idle"`).
    Idle,
    /// The message a reaction collector watches was deleted, alone or in a
    /// bulk delete (discord.js `"messageDelete"`).
    MessageDelete,
    /// The channel holding the watched message was deleted
    /// (discord.js `"channelDelete"`).
    ChannelDelete,
    /// The thread holding the watched message was deleted
    /// (discord.js `"threadDelete"`).
    ThreadDelete,
    /// The bot left or was removed from the guild holding the watched message
    /// (discord.js `"guildDelete"`). Outages (`unavailable: true`) do not count.
    GuildDelete,
    /// Collection was stopped explicitly via `stop`/`stop_with_reason`,
    /// carrying the supplied reason (`"user"` by default).
    User(String),
    /// The underlying event channel closed because the [`CollectorHub`]
    /// (and every other sender) was dropped.
    ChannelDropped,
}

#[cfg(feature = "collectors")]
#[derive(Clone)]
/// Cloneable handle that stops a collector from another task, ending any
/// in-flight `next()` call with `None`.
///
/// Obtain one with the collector's `stop_handle()` method before moving the
/// collector elsewhere (for example into a spawned task).
pub struct CollectorStopHandle {
    stop_tx: Arc<watch::Sender<Option<String>>>,
}

#[cfg(feature = "collectors")]
impl CollectorStopHandle {
    /// Stops the associated collector with the default reason `"user"`.
    pub fn stop(&self) {
        self.stop_with_reason(DEFAULT_STOP_REASON);
    }

    /// Stops the associated collector, recording `reason` as
    /// [`CollectorEndReason::User`]. The first stop reason wins; later calls
    /// are ignored.
    pub fn stop_with_reason(&self, reason: impl Into<String>) {
        request_stop(&self.stop_tx, reason.into());
    }
}

#[cfg(feature = "collectors")]
fn request_stop(stop_tx: &watch::Sender<Option<String>>, reason: String) {
    stop_tx.send_if_modified(|current| {
        if current.is_none() {
            *current = Some(reason);
            true
        } else {
            false
        }
    });
}

#[cfg(feature = "collectors")]
#[derive(Clone)]
/// Fan-out point that feeds gateway events to collectors.
///
/// The client publishes every dispatched [`Event`] here (after the cache is
/// updated) and each collector owns its own subscription. Two delivery rules
/// follow from the underlying `tokio::sync::broadcast` channel:
///
/// - **No history.** A collector only sees events published after it was
///   created. Reactions or messages that arrived earlier are not replayed;
///   fetch them over REST if you need them.
/// - **Bounded buffer, lossy on lag.** Each subscriber can fall at most
///   `capacity` events behind (256 by default, see
///   [`with_capacity`](Self::with_capacity)). The buffer holds *all* event
///   types, not just the ones a collector wants, and a collector only drains
///   it while `next()`/`collect()` is being awaited. When it falls further
///   behind, the oldest events are dropped for that collector, the drop is
///   counted in its `lagged_events()`, and collection continues with the
///   oldest event still buffered. Publishing never blocks the gateway.
///
/// Size the capacity for your event volume and check `lagged_events()` when
/// exact results matter: a reaction collector that lagged may have missed
/// adds (undercount) or, with `dispose(true)`, removals (overcount).
pub struct CollectorHub {
    sender: broadcast::Sender<Event>,
}

#[cfg(feature = "collectors")]
impl Default for CollectorHub {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "collectors")]
impl CollectorHub {
    /// Creates a hub whose subscribers buffer up to 256 events each.
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_HUB_CAPACITY)
    }

    /// Creates a hub whose subscribers buffer up to `capacity` events each
    /// (at least 1) before lagging collectors start losing the oldest ones.
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self { sender }
    }

    pub(crate) fn publish(&self, event: Event) {
        let _ = self.sender.send(event);
    }

    pub fn message_collector(&self) -> MessageCollector {
        MessageCollector::new(self.sender.subscribe())
    }

    pub fn interaction_collector(&self) -> InteractionCollector {
        InteractionCollector::new(self.sender.subscribe())
    }

    pub fn component_collector(&self) -> ComponentCollector {
        ComponentCollector::new(self.sender.subscribe())
    }

    pub fn modal_collector(&self) -> ModalCollector {
        ModalCollector::new(self.sender.subscribe())
    }

    /// Collects reactions on one message, the equivalent of discord.js's
    /// `message.createReactionCollector()` / `awaitReactions()`.
    ///
    /// The channel and guild are learned from the first reaction event, so
    /// channel/thread/guild deletion only ends collection after that; use
    /// [`reaction_collector_for_message`](Self::reaction_collector_for_message)
    /// or [`ReactionCollector::channel_id`] when you already know them.
    pub fn reaction_collector(&self, message_id: impl Into<Snowflake>) -> ReactionCollector {
        ReactionCollector::new(self.sender.subscribe(), message_id.into())
    }

    /// Collects reactions on `message`, taking its channel and guild from the
    /// message so deletion of either ends collection right away.
    pub fn reaction_collector_for_message(&self, message: &Message) -> ReactionCollector {
        let mut collector = self.reaction_collector(message.id.clone());
        collector.core.logic.channel_id = Some(message.channel_id.clone());
        collector.core.logic.guild_id = message.guild_id.clone();
        collector
    }
}

// Decision log: one collector core for every collector type.
// - Purpose: add the reaction collector without duplicating the stop handle,
//   end reasons, overall/idle timers, and lag accounting.
// - Constraint: the 2.2.0 core only knew a `fn(Event) -> Option<T>` extractor,
//   a filter, and "yielded >= max_items". A reaction collector must also
//   change state without yielding (removals), end on an event (message
//   deleted), and judge limits on live tallies (emojis, users).
// - Alternatives: a separate loop for reactions; reaction fields in the core;
//   a core generic over a `CollectorLogic` that maps each event to
//   Collect/Skip/End and reports limits.
// - Chosen: the generic core. The four existing collectors use `Extracting`,
//   the reaction collector uses `ReactionLogic`.
// - Why: a second loop would fork the select!/timer/stop race handling, and
//   reaction fields would teach the core one event shape. The generic core
//   shares all timing and stop logic; a new collector only interprets events.
// - Trade-offs: public collector APIs are unchanged. `handle` must stay
//   synchronous: the only cancellation point in `select!` is then the
//   cancel-safe broadcast `recv()`, so never add an await inside `handle`.

#[cfg(feature = "collectors")]
/// What a collector's logic decided about one gateway event.
enum Step<T> {
    /// Hand `T` to the caller: counts as received and restarts the idle window.
    Collect(T),
    /// Nothing to yield: an unrelated event, a filtered-out item, or a pure
    /// state change such as a disposed reaction.
    Skip,
    /// End collection with this reason (for example the watched message was
    /// deleted).
    End(CollectorEndReason),
}

#[cfg(feature = "collectors")]
/// Event interpretation plugged into [`CollectorCore`].
///
/// `handle` must stay synchronous so the only cancellation point inside the
/// core's `select!` is the cancel-safe broadcast `recv()`.
trait CollectorLogic: Send {
    type Item;

    fn handle(&mut self, event: Event) -> Step<Self::Item>;

    /// Limit-based end reason, if one is reached. `received` is the number of
    /// items yielded so far; logics with their own tallies may ignore it.
    fn limit_reached(&self, received: usize) -> Option<CollectorEndReason>;
}

#[cfg(feature = "collectors")]
/// Logic for the single-event collectors: extract one item type, filter it,
/// and stop after `max_items` yielded items.
struct Extracting<T> {
    extract: fn(Event) -> Option<T>,
    filter: Option<EventFilter<T>>,
    max_items: Option<usize>,
}

#[cfg(feature = "collectors")]
impl<T> Extracting<T> {
    fn new(extract: fn(Event) -> Option<T>) -> Self {
        Self {
            extract,
            filter: None,
            max_items: None,
        }
    }
}

#[cfg(feature = "collectors")]
impl<T> CollectorLogic for Extracting<T> {
    type Item = T;

    fn handle(&mut self, event: Event) -> Step<T> {
        let Some(item) = (self.extract)(event) else {
            return Step::Skip;
        };
        if passes(self.filter.as_ref(), &item) {
            Step::Collect(item)
        } else {
            Step::Skip
        }
    }

    fn limit_reached(&self, received: usize) -> Option<CollectorEndReason> {
        reached(self.max_items, received).then_some(CollectorEndReason::Limit)
    }
}

#[cfg(feature = "collectors")]
fn passes<T>(filter: Option<&EventFilter<T>>, item: &T) -> bool {
    filter.map(|filter| filter(item)).unwrap_or(true)
}

#[cfg(feature = "collectors")]
fn reached(limit: Option<usize>, actual: usize) -> bool {
    limit.is_some_and(|limit| actual >= limit)
}

#[cfg(feature = "collectors")]
/// Outcome of racing the receive loop against the stop signal and timers.
enum NextOutcome<T> {
    Item(T),
    Ended(CollectorEndReason),
}

#[cfg(feature = "collectors")]
/// Shared implementation behind every collector type.
struct CollectorCore<L> {
    receiver: broadcast::Receiver<Event>,
    logic: L,
    timeout: Option<Duration>,
    idle: Option<Duration>,
    deadline: Option<time::Instant>,
    idle_deadline: Option<time::Instant>,
    received: usize,
    lagged_events: u64,
    end_reason: Option<CollectorEndReason>,
    stop_tx: Arc<watch::Sender<Option<String>>>,
    stop_rx: watch::Receiver<Option<String>>,
}

#[cfg(feature = "collectors")]
impl<L: CollectorLogic> CollectorCore<L> {
    fn new(receiver: broadcast::Receiver<Event>, logic: L) -> Self {
        let (stop_tx, stop_rx) = watch::channel(None);
        Self {
            receiver,
            logic,
            timeout: None,
            idle: None,
            deadline: None,
            idle_deadline: None,
            received: 0,
            lagged_events: 0,
            end_reason: None,
            stop_tx: Arc::new(stop_tx),
            stop_rx,
        }
    }

    fn stop(&self, reason: impl Into<String>) {
        request_stop(&self.stop_tx, reason.into());
    }

    fn stop_handle(&self) -> CollectorStopHandle {
        CollectorStopHandle {
            stop_tx: Arc::clone(&self.stop_tx),
        }
    }

    fn end_reason(&self) -> Option<CollectorEndReason> {
        if let Some(reason) = &self.end_reason {
            return Some(reason.clone());
        }
        self.stop_rx.borrow().clone().map(CollectorEndReason::User)
    }

    fn reset_timer(&mut self) {
        let now = time::Instant::now();
        if let Some(timeout) = self.timeout {
            self.deadline = Some(now + timeout);
        }
        if let Some(idle) = self.idle {
            self.idle_deadline = Some(now + idle);
        }
    }

    async fn next(&mut self) -> Option<L::Item> {
        if self.end_reason.is_some() {
            return None;
        }
        if let Some(reason) = self.stop_rx.borrow().clone() {
            self.end_reason = Some(CollectorEndReason::User(reason));
            return None;
        }
        if let Some(reason) = self.logic.limit_reached(self.received) {
            self.end_reason = Some(reason);
            return None;
        }

        let now = time::Instant::now();
        if let Some(timeout) = self.timeout {
            self.deadline.get_or_insert(now + timeout);
        }
        if let Some(idle) = self.idle {
            self.idle_deadline.get_or_insert(now + idle);
        }
        let deadline = self.deadline;
        let idle_deadline = self.idle_deadline;

        let receiver = &mut self.receiver;
        let logic = &mut self.logic;
        let lagged_events = &mut self.lagged_events;
        let stop_rx = &mut self.stop_rx;

        let outcome = tokio::select! {
            outcome = recv_next(receiver, logic, lagged_events) => outcome,
            reason = wait_for_stop(stop_rx) => {
                NextOutcome::Ended(CollectorEndReason::User(reason))
            }
            _ = sleep_until_opt(deadline) => NextOutcome::Ended(CollectorEndReason::Time),
            _ = sleep_until_opt(idle_deadline) => NextOutcome::Ended(CollectorEndReason::Idle),
        };

        match outcome {
            NextOutcome::Item(item) => {
                self.received = self.received.saturating_add(1);
                self.end_reason = self.logic.limit_reached(self.received);
                if let Some(idle) = self.idle {
                    self.idle_deadline = Some(time::Instant::now() + idle);
                }
                Some(item)
            }
            NextOutcome::Ended(reason) => {
                self.end_reason = Some(reason);
                None
            }
        }
    }
}

#[cfg(feature = "collectors")]
/// Feeds events to `logic` until it collects an item or ends collection,
/// tracking lag. Ends with [`CollectorEndReason::ChannelDropped`] when the
/// broadcast channel closes.
async fn recv_next<L: CollectorLogic>(
    receiver: &mut broadcast::Receiver<Event>,
    logic: &mut L,
    lagged_events: &mut u64,
) -> NextOutcome<L::Item> {
    loop {
        match receiver.recv().await {
            Ok(event) => match logic.handle(event) {
                Step::Collect(item) => return NextOutcome::Item(item),
                Step::Skip => {}
                Step::End(reason) => return NextOutcome::Ended(reason),
            },
            Err(broadcast::error::RecvError::Closed) => {
                return NextOutcome::Ended(CollectorEndReason::ChannelDropped)
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                *lagged_events = lagged_events.saturating_add(skipped);
            }
        }
    }
}

#[cfg(feature = "collectors")]
/// Resolves with the stop reason once a stop is requested.
async fn wait_for_stop(stop_rx: &mut watch::Receiver<Option<String>>) -> String {
    loop {
        if let Some(reason) = stop_rx.borrow_and_update().clone() {
            return reason;
        }
        if stop_rx.changed().await.is_err() {
            // The paired sender lives inside the collector itself, so it can
            // never be dropped while this future is polled. Pend forever so
            // the other `select!` branches keep making progress regardless.
            std::future::pending::<()>().await;
        }
    }
}

#[cfg(feature = "collectors")]
/// Sleeps until `deadline`, or pends forever when no deadline is configured.
async fn sleep_until_opt(deadline: Option<time::Instant>) {
    match deadline {
        Some(deadline) => time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

#[cfg(feature = "collectors")]
/// Control surface shared by every collector: timers, stop, end reason,
/// counters, and `next()`.
macro_rules! impl_collector_controls {
    ($collector:ident, $item:ty) => {
        #[cfg(feature = "collectors")]
        impl $collector {
            /// Sets the overall collection window. Once `duration` has
            /// elapsed since the first `next()` call (or the last
            /// [`reset_timer`](Self::reset_timer)), collection ends with
            /// [`CollectorEndReason::Time`].
            pub fn timeout(mut self, duration: Duration) -> Self {
                self.core.timeout = Some(duration);
                self
            }

            /// Sets the idle window. If no matching item arrives for
            /// `duration`, collection ends with [`CollectorEndReason::Idle`].
            /// The window restarts every time a matching item is collected,
            /// unlike [`timeout`](Self::timeout) which is absolute.
            pub fn idle(mut self, duration: Duration) -> Self {
                self.core.idle = Some(duration);
                self
            }

            /// Number of events dropped because this collector lagged behind
            /// the broadcast channel (see [`CollectorHub`] for when that
            /// happens).
            pub fn lagged_events(&self) -> u64 {
                self.core.lagged_events
            }

            /// Number of matching items this collector has yielded so far.
            pub fn received_count(&self) -> usize {
                self.core.received
            }

            /// Why collection ended, or `None` while the collector is still
            /// live. Mirrors discord.js's `Collector#endReason`.
            pub fn end_reason(&self) -> Option<CollectorEndReason> {
                self.core.end_reason()
            }

            /// Stops collection with the default reason `"user"`. Subsequent
            /// `next()` calls return `None` and
            /// [`end_reason`](Self::end_reason) reports
            /// [`CollectorEndReason::User`].
            pub fn stop(&self) {
                self.core.stop(DEFAULT_STOP_REASON);
            }

            /// Stops collection with a custom reason, recorded as
            /// [`CollectorEndReason::User`]. The first stop reason wins.
            pub fn stop_with_reason(&self, reason: impl Into<String>) {
                self.core.stop(reason);
            }

            /// Returns a cloneable [`CollectorStopHandle`] that can stop this
            /// collector from another task, ending an in-flight `next()`
            /// call with `None`.
            pub fn stop_handle(&self) -> CollectorStopHandle {
                self.core.stop_handle()
            }

            /// Restarts both the overall timeout and the idle window from
            /// now, where configured. Equivalent to discord.js's
            /// `Collector#resetTimer`.
            pub fn reset_timer(&mut self) {
                self.core.reset_timer();
            }

            /// Waits for the next matching item. Returns `None` once
            /// collection has ended; consult
            /// [`end_reason`](Self::end_reason) to learn why.
            pub async fn next(&mut self) -> Option<$item> {
                self.core.next().await
            }
        }
    };
}

#[cfg(feature = "collectors")]
/// Builder methods and `collect()` for the single-event collectors.
macro_rules! impl_extracting_collector {
    ($collector:ident, $item:ty, $extract:path) => {
        #[cfg(feature = "collectors")]
        impl $collector {
            fn new(receiver: broadcast::Receiver<Event>) -> Self {
                Self {
                    core: CollectorCore::new(receiver, Extracting::new($extract)),
                }
            }

            /// Only collects items for which `filter` returns `true`.
            /// Non-matching items are skipped: they do not count towards
            /// `max_items` and do not reset the idle window.
            pub fn filter<F>(mut self, filter: F) -> Self
            where
                F: Fn(&$item) -> bool + Send + Sync + 'static,
            {
                self.core.logic.filter = Some(Arc::new(filter));
                self
            }

            /// Stops collecting once this many items have been collected,
            /// recording [`CollectorEndReason::Limit`].
            pub fn max_items(mut self, max_items: usize) -> Self {
                self.core.logic.max_items = Some(max_items);
                self
            }

            /// Collects items until the collector ends (limit, timeout, idle,
            /// stop, or channel close) and returns everything gathered.
            pub async fn collect(mut self) -> Vec<$item> {
                let mut items = Vec::new();
                while let Some(item) = self.core.next().await {
                    items.push(item);
                }
                items
            }
        }

        impl_collector_controls!($collector, $item);
    };
}

#[cfg(feature = "collectors")]
fn extract_message(event: Event) -> Option<Message> {
    match event {
        Event::MessageCreate(event) | Event::MessageUpdate(event) => Some(event.message),
        _ => None,
    }
}

#[cfg(feature = "collectors")]
fn extract_interaction(event: Event) -> Option<Interaction> {
    match event {
        Event::InteractionCreate(event) => Some(event.interaction),
        _ => None,
    }
}

#[cfg(feature = "collectors")]
fn extract_component(event: Event) -> Option<ComponentInteraction> {
    match extract_interaction(event)? {
        Interaction::Component(component) => Some(component),
        _ => None,
    }
}

#[cfg(feature = "collectors")]
fn extract_modal(event: Event) -> Option<ModalSubmitInteraction> {
    match extract_interaction(event)? {
        Interaction::ModalSubmit(modal) => Some(modal),
        _ => None,
    }
}

#[cfg(feature = "collectors")]
/// Collects `MESSAGE_CREATE` and `MESSAGE_UPDATE` messages.
pub struct MessageCollector {
    core: CollectorCore<Extracting<Message>>,
}

#[cfg(feature = "collectors")]
impl_extracting_collector!(MessageCollector, Message, extract_message);

#[cfg(feature = "collectors")]
/// Collects every `INTERACTION_CREATE` interaction.
pub struct InteractionCollector {
    core: CollectorCore<Extracting<Interaction>>,
}

#[cfg(feature = "collectors")]
impl_extracting_collector!(InteractionCollector, Interaction, extract_interaction);

#[cfg(feature = "collectors")]
/// Collects message component interactions (buttons, select menus).
pub struct ComponentCollector {
    core: CollectorCore<Extracting<ComponentInteraction>>,
}

#[cfg(feature = "collectors")]
impl_extracting_collector!(ComponentCollector, ComponentInteraction, extract_component);

#[cfg(feature = "collectors")]
/// Collects modal submit interactions.
pub struct ModalCollector {
    core: CollectorCore<Extracting<ModalSubmitInteraction>>,
}

#[cfg(feature = "collectors")]
impl_extracting_collector!(ModalCollector, ModalSubmitInteraction, extract_modal);

#[cfg(feature = "collectors")]
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
/// Identity of a reaction emoji, the key of a [`ReactionCollection`].
///
/// Mirrors discord.js's `ReactionCollector.key(reaction)`
/// (`emoji.id ?? emoji.name`): custom emojis are keyed by id because their
/// names can change and need not be unique; Unicode emojis by their text.
/// The text is compared as sent by Discord, so `"❤"` and `"❤️"` (with the
/// U+FE0F variation selector) are different keys.
pub enum EmojiKey {
    /// A custom (guild or application) emoji.
    Custom(Snowflake),
    /// A Unicode emoji such as `"✅"`.
    Unicode(String),
}

#[cfg(feature = "collectors")]
impl EmojiKey {
    /// Key for a custom emoji id.
    pub fn custom(id: impl Into<Snowflake>) -> Self {
        Self::Custom(id.into())
    }

    /// Key for a Unicode emoji.
    pub fn unicode(emoji: impl Into<String>) -> Self {
        Self::Unicode(emoji.into())
    }

    /// Key of a gateway emoji object, or `None` when it carries neither an
    /// id nor a name.
    pub fn from_emoji(emoji: &Emoji) -> Option<Self> {
        match (&emoji.id, &emoji.name) {
            (Some(id), _) => Some(Self::Custom(Snowflake::new(id.clone()))),
            (None, Some(name)) => Some(Self::Unicode(name.clone())),
            (None, None) => None,
        }
    }
}

#[cfg(feature = "collectors")]
#[derive(Clone, Debug, Default)]
/// Users who reacted with one emoji, as observed by a [`ReactionCollector`].
///
/// Normal and burst ("super") reactions are tracked separately because a
/// user can hold both for the same emoji and remove them independently.
pub struct CollectedReaction {
    emoji: Emoji,
    users: BTreeSet<Snowflake>,
    burst_users: BTreeSet<Snowflake>,
}

#[cfg(feature = "collectors")]
impl CollectedReaction {
    /// The emoji as last seen in a reaction event (custom emoji names can
    /// change while collecting).
    pub fn emoji(&self) -> &Emoji {
        &self.emoji
    }

    /// Users holding a normal reaction with this emoji.
    pub fn users(&self) -> &BTreeSet<Snowflake> {
        &self.users
    }

    /// Users holding a burst (super) reaction with this emoji.
    pub fn burst_users(&self) -> &BTreeSet<Snowflake> {
        &self.burst_users
    }

    /// Normal plus burst reactions, matching how Discord's reaction `count`
    /// includes super reactions.
    pub fn count(&self) -> usize {
        self.users.len() + self.burst_users.len()
    }

    /// Whether `user_id` holds a normal or burst reaction with this emoji.
    pub fn contains_user(&self, user_id: &Snowflake) -> bool {
        self.users.contains(user_id) || self.burst_users.contains(user_id)
    }

    fn set_mut(&mut self, burst: bool) -> &mut BTreeSet<Snowflake> {
        if burst {
            &mut self.burst_users
        } else {
            &mut self.users
        }
    }
}

#[cfg(feature = "collectors")]
#[derive(Clone, Debug, Default)]
/// Reactions gathered by a [`ReactionCollector`], keyed by [`EmojiKey`] in
/// the order each emoji was first collected.
///
/// This is what discord.js exposes as `collector.collected` plus
/// `collector.users` and `collector.total`. Only reactions observed while
/// collecting are included; with `dispose(true)` removals are subtracted.
pub struct ReactionCollection {
    // Discord allows at most 20 distinct reactions per message, so an
    // insertion-ordered Vec with linear lookup is both simpler and faster
    // than hashing, and keeps the discord.js `Collection` iteration order.
    reactions: Vec<(EmojiKey, CollectedReaction)>,
    // How many (emoji, normal/burst) slots each user holds, so `users()` and
    // the `max_users` check stay O(log n) instead of re-scanning every set
    // on each event.
    user_slots: BTreeMap<Snowflake, usize>,
    total: usize,
}

#[cfg(feature = "collectors")]
impl ReactionCollection {
    /// Users and count for one emoji.
    pub fn get(&self, key: &EmojiKey) -> Option<&CollectedReaction> {
        self.reactions
            .iter()
            .find_map(|(candidate, reaction)| (candidate == key).then_some(reaction))
    }

    /// Emojis and their reactions, in first-collected order.
    pub fn iter(&self) -> impl Iterator<Item = (&EmojiKey, &CollectedReaction)> {
        self.reactions.iter().map(|(key, reaction)| (key, reaction))
    }

    /// Number of distinct emojis (discord.js `collected.size`).
    pub fn len(&self) -> usize {
        self.reactions.len()
    }

    /// Whether no reaction is currently collected.
    pub fn is_empty(&self) -> bool {
        self.reactions.is_empty()
    }

    /// Reactions currently collected across all emojis, normal and burst
    /// (discord.js `collector.total`).
    pub fn total(&self) -> usize {
        self.total
    }

    /// Distinct users holding at least one collected reaction
    /// (discord.js `collector.users`), in id order.
    pub fn users(&self) -> impl Iterator<Item = &Snowflake> {
        self.user_slots.keys()
    }

    /// Number of distinct users holding at least one collected reaction.
    pub fn user_count(&self) -> usize {
        self.user_slots.len()
    }

    fn position(&self, key: &EmojiKey) -> Option<usize> {
        self.reactions
            .iter()
            .position(|(candidate, _)| candidate == key)
    }

    fn insert(&mut self, key: EmojiKey, emoji: Emoji, user_id: Snowflake, burst: bool) {
        let index = match self.position(&key) {
            Some(index) => index,
            None => {
                self.reactions.push((key, CollectedReaction::default()));
                self.reactions.len() - 1
            }
        };
        let Some((_, reaction)) = self.reactions.get_mut(index) else {
            return;
        };
        reaction.emoji = emoji;
        if reaction.set_mut(burst).insert(user_id.clone()) {
            self.total = self.total.saturating_add(1);
            *self.user_slots.entry(user_id).or_insert(0) += 1;
        }
    }

    fn remove_user(&mut self, key: &EmojiKey, user_id: &Snowflake, burst: bool) {
        let Some(index) = self.position(key) else {
            return;
        };
        let Some((_, reaction)) = self.reactions.get_mut(index) else {
            return;
        };
        if !reaction.set_mut(burst).remove(user_id) {
            return;
        }
        // discord.js drops the emoji from `collected` once its count hits 0.
        if reaction.count() == 0 {
            self.reactions.remove(index);
        }
        self.total = self.total.saturating_sub(1);
        self.release_user(user_id);
    }

    fn remove_emoji(&mut self, key: &EmojiKey) {
        let Some(index) = self.position(key) else {
            return;
        };
        let (_, reaction) = self.reactions.remove(index);
        for user_id in reaction.users.iter().chain(reaction.burst_users.iter()) {
            self.total = self.total.saturating_sub(1);
            self.release_user(user_id);
        }
    }

    fn clear(&mut self) {
        self.reactions.clear();
        self.user_slots.clear();
        self.total = 0;
    }

    fn release_user(&mut self, user_id: &Snowflake) {
        if let Some(slots) = self.user_slots.get_mut(user_id) {
            *slots = slots.saturating_sub(1);
            if *slots == 0 {
                self.user_slots.remove(user_id);
            }
        }
    }
}

// Decision log: reaction collector.
// - Purpose: discord.js `ReactionCollector` / `awaitReactions` parity: wait
//   for reactions on one message and tally users per emoji (polls,
//   confirmations).
// - Constraint: events only arrive through the `CollectorHub` broadcast
//   (`publish` keeps its signature while gateway dispatch is restructured);
//   there is no history and lagging subscribers lose events.
// - Alternatives: a hub-wide collector with an optional message filter; a
//   collector always bound to one message (discord.js); tallies keyed by
//   (message, emoji).
// - Chosen: bound to one message, via `reaction_collector(message_id)` and
//   `reaction_collector_for_message(&Message)`. REMOVE / REMOVE_ALL /
//   REMOVE_EMOJI change the tally only with `dispose(true)`. Message, channel,
//   thread, and guild deletion always end collection, as in discord.js.
// - Why: emoji-keyed tallies and REMOVE_ALL / REMOVE_EMOJI are per-message
//   concepts; hub-wide tallies would mix messages and let another message's
//   REMOVE_ALL wipe everything, and (message, emoji) keys complicate the API
//   for what is in practice a one-message flow. Cross-message reaction logic
//   belongs in event handlers. Gating REMOVE_ALL behind `dispose` differs
//   from discord.js (which always empties) but keeps one meaning for
//   `dispose(false)`: "everyone who reacted while collecting".
// - Trade-offs: `CollectorEndReason` gains six variants and becomes
//   `#[non_exhaustive]` (a 3.0 break). Normal and burst reactions are tracked
//   separately so removing one of the two keeps the tally right. With only a
//   message id, channel/guild deletion is noticed only after the first
//   reaction reveals them. A thread that disappears because its parent
//   channel was deleted is not detected (that needs the cache).

#[cfg(feature = "collectors")]
/// Logic behind [`ReactionCollector`].
struct ReactionLogic {
    message_id: Snowflake,
    channel_id: Option<Snowflake>,
    guild_id: Option<Snowflake>,
    filter: Option<EventFilter<ReactionEvent>>,
    dispose: bool,
    max_items: Option<usize>,
    max_emojis: Option<usize>,
    max_users: Option<usize>,
    collected: ReactionCollection,
}

#[cfg(feature = "collectors")]
impl ReactionLogic {
    fn new(message_id: Snowflake) -> Self {
        Self {
            message_id,
            channel_id: None,
            guild_id: None,
            filter: None,
            dispose: false,
            max_items: None,
            max_emojis: None,
            max_users: None,
            collected: ReactionCollection::default(),
        }
    }

    /// Whether an event about `message_id` targets the watched message. On a
    /// match, remembers the channel and guild so their deletion can end
    /// collection even when only the message id was given.
    fn targets(
        &mut self,
        message_id: Option<&Snowflake>,
        channel_id: Option<&Snowflake>,
        guild_id: Option<&Snowflake>,
    ) -> bool {
        if message_id != Some(&self.message_id) {
            return false;
        }
        if self.channel_id.is_none() {
            self.channel_id = channel_id.cloned();
        }
        if self.guild_id.is_none() {
            self.guild_id = guild_id.cloned();
        }
        true
    }

    fn targets_reaction(&mut self, reaction: &ReactionEvent) -> bool {
        self.targets(
            reaction.message_id.as_ref(),
            reaction.channel_id.as_ref(),
            reaction.guild_id.as_ref(),
        )
    }

    fn end_if(condition: bool, reason: CollectorEndReason) -> Step<ReactionEvent> {
        if condition {
            Step::End(reason)
        } else {
            Step::Skip
        }
    }
}

#[cfg(feature = "collectors")]
/// Key, user, and normal/burst kind of a reaction add/remove event, or `None`
/// when Discord omitted the emoji or user.
fn reaction_identity(reaction: &ReactionEvent) -> Option<(EmojiKey, Snowflake, bool)> {
    let key = EmojiKey::from_emoji(reaction.emoji.as_ref()?)?;
    let user_id = reaction.user_id.clone()?;
    // `type` 1 is a burst reaction; older payloads only carry `burst`.
    let burst = reaction.burst == Some(true) || reaction.reaction_type == Some(1);
    Some((key, user_id, burst))
}

#[cfg(feature = "collectors")]
impl CollectorLogic for ReactionLogic {
    type Item = ReactionEvent;

    fn handle(&mut self, event: Event) -> Step<ReactionEvent> {
        match event {
            Event::MessageReactionAdd(reaction) => {
                if !self.targets_reaction(&reaction) || !passes(self.filter.as_ref(), &reaction) {
                    return Step::Skip;
                }
                let Some((key, user_id, burst)) = reaction_identity(&reaction) else {
                    return Step::Skip;
                };
                let emoji = reaction.emoji.clone().unwrap_or_default();
                self.collected.insert(key, emoji, user_id, burst);
                Step::Collect(reaction)
            }
            // discord.js also runs the filter on disposals, so a filter that
            // scopes collection (for example to one user) scopes removal too.
            Event::MessageReactionRemove(reaction) if self.dispose => {
                if self.targets_reaction(&reaction) && passes(self.filter.as_ref(), &reaction) {
                    if let Some((key, user_id, burst)) = reaction_identity(&reaction) {
                        self.collected.remove_user(&key, &user_id, burst);
                    }
                }
                Step::Skip
            }
            Event::MessageReactionRemoveAll(event) if self.dispose => {
                if self.targets(
                    event.message_id.as_ref(),
                    event.channel_id.as_ref(),
                    event.guild_id.as_ref(),
                ) {
                    self.collected.clear();
                }
                Step::Skip
            }
            Event::MessageReactionRemoveEmoji(event) if self.dispose => {
                if self.targets(
                    event.message_id.as_ref(),
                    event.channel_id.as_ref(),
                    event.guild_id.as_ref(),
                ) {
                    if let Some(key) = event.emoji.as_ref().and_then(EmojiKey::from_emoji) {
                        self.collected.remove_emoji(&key);
                    }
                }
                Step::Skip
            }
            Event::MessageDelete(event) => Self::end_if(
                event.data.id == self.message_id,
                CollectorEndReason::MessageDelete,
            ),
            Event::MessageDeleteBulk(event) => Self::end_if(
                event.ids.contains(&self.message_id),
                CollectorEndReason::MessageDelete,
            ),
            Event::ChannelDelete(event) => Self::end_if(
                self.channel_id.as_ref() == Some(&event.channel.id),
                CollectorEndReason::ChannelDelete,
            ),
            Event::ThreadDelete(event) => Self::end_if(
                self.channel_id.as_ref() == Some(&event.thread.id),
                CollectorEndReason::ThreadDelete,
            ),
            Event::GuildDelete(event) => Self::end_if(
                self.guild_id.as_ref() == Some(&event.data.id)
                    && event.data.unavailable != Some(true),
                CollectorEndReason::GuildDelete,
            ),
            _ => Step::Skip,
        }
    }

    fn limit_reached(&self, _received: usize) -> Option<CollectorEndReason> {
        // Compared against the live tallies, like discord.js, so disposed
        // reactions make room again under `max_items`.
        if reached(self.max_items, self.collected.total()) {
            Some(CollectorEndReason::Limit)
        } else if reached(self.max_emojis, self.collected.len()) {
            Some(CollectorEndReason::EmojiLimit)
        } else if reached(self.max_users, self.collected.user_count()) {
            Some(CollectorEndReason::UserLimit)
        } else {
            None
        }
    }
}

#[cfg(feature = "collectors")]
/// Collects reactions added to one message, with discord.js
/// `ReactionCollector` semantics.
///
/// Create one with [`CollectorHub::reaction_collector`] or
/// [`CollectorHub::reaction_collector_for_message`]. [`next`](Self::next)
/// yields each `MESSAGE_REACTION_ADD` that passes the filter;
/// [`collected`](Self::collected) and [`collect`](Self::collect) give the
/// tally keyed by emoji.
///
/// - `max_items` counts reactions currently collected (discord.js `max`),
///   `max_emojis` distinct emojis, `max_users` distinct users; reaching one
///   ends collection with `Limit`, `EmojiLimit`, or `UserLimit`. A limit of 0
///   ends collection immediately.
/// - With [`dispose(true)`](Self::dispose), `MESSAGE_REACTION_REMOVE`,
///   `MESSAGE_REACTION_REMOVE_ALL`, and `MESSAGE_REACTION_REMOVE_EMOJI`
///   update the tally; otherwise the tally only grows.
/// - Deleting the message (alone or in bulk), its channel or thread, or
///   removing the bot from its guild ends collection with
///   `MessageDelete` / `ChannelDelete` / `ThreadDelete` / `GuildDelete`.
///
/// Reactions added before the collector was created are not included, and
/// events lost to broadcast lag are not recovered; see [`CollectorHub`].
/// Needs the `GUILD_MESSAGE_REACTIONS` (or `DIRECT_MESSAGE_REACTIONS`) intent.
pub struct ReactionCollector {
    core: CollectorCore<ReactionLogic>,
}

#[cfg(feature = "collectors")]
impl ReactionCollector {
    fn new(receiver: broadcast::Receiver<Event>, message_id: Snowflake) -> Self {
        Self {
            core: CollectorCore::new(receiver, ReactionLogic::new(message_id)),
        }
    }

    /// Only collects reactions for which `filter` returns `true`, for example
    /// `|reaction| reaction.user_id.as_ref() == Some(&author_id)`. With
    /// `dispose(true)` the filter also gates which removals are applied.
    pub fn filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&ReactionEvent) -> bool + Send + Sync + 'static,
    {
        self.core.logic.filter = Some(Arc::new(filter));
        self
    }

    /// Applies reaction removals to the tally (discord.js `dispose: true`).
    pub fn dispose(mut self, enabled: bool) -> Self {
        self.core.logic.dispose = enabled;
        self
    }

    /// Ends with [`CollectorEndReason::Limit`] once this many reactions are
    /// collected (normal and burst; disposed reactions no longer count).
    pub fn max_items(mut self, max_items: usize) -> Self {
        self.core.logic.max_items = Some(max_items);
        self
    }

    /// Ends with [`CollectorEndReason::EmojiLimit`] once this many distinct
    /// emojis are collected.
    pub fn max_emojis(mut self, max_emojis: usize) -> Self {
        self.core.logic.max_emojis = Some(max_emojis);
        self
    }

    /// Ends with [`CollectorEndReason::UserLimit`] once this many distinct
    /// users have a collected reaction.
    pub fn max_users(mut self, max_users: usize) -> Self {
        self.core.logic.max_users = Some(max_users);
        self
    }

    /// Channel (or thread) holding the message, so its deletion ends
    /// collection before the first reaction arrives.
    pub fn channel_id(mut self, channel_id: impl Into<Snowflake>) -> Self {
        self.core.logic.channel_id = Some(channel_id.into());
        self
    }

    /// Guild holding the message, so removal from it ends collection before
    /// the first reaction arrives.
    pub fn guild_id(mut self, guild_id: impl Into<Snowflake>) -> Self {
        self.core.logic.guild_id = Some(guild_id.into());
        self
    }

    /// The watched message.
    pub fn message_id(&self) -> &Snowflake {
        &self.core.logic.message_id
    }

    /// Reactions collected so far. Updated as `next()` consumes events.
    pub fn collected(&self) -> &ReactionCollection {
        &self.core.logic.collected
    }

    /// Collects until the collector ends and returns the final tally keyed by
    /// emoji (the discord.js `awaitReactions` result).
    pub async fn collect(mut self) -> ReactionCollection {
        while self.core.next().await.is_some() {}
        self.core.logic.collected
    }
}

#[cfg(feature = "collectors")]
impl_collector_controls!(ReactionCollector, ReactionEvent);

#[cfg(all(test, feature = "collectors"))]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use serde_json::Value;
    use tokio::time;

    use crate::event::{decode_event, Event};
    use crate::model::{Interaction, Message, Snowflake};
    use crate::types::Emoji;

    use super::{CollectorEndReason, CollectorHub, EmojiKey, ReactionCollection};

    fn interaction_event(payload: Value) -> crate::event::Event {
        decode_event("INTERACTION_CREATE", payload).expect("valid interaction event")
    }

    fn ping_interaction(id: &str) -> crate::event::Event {
        interaction_event(json!({
            "id": id,
            "application_id": "2",
            "token": "token",
            "type": 1
        }))
    }

    fn component_interaction(id: &str, custom_id: &str) -> crate::event::Event {
        interaction_event(json!({
            "id": id,
            "application_id": "2",
            "token": "token",
            "type": 3,
            "data": {
                "custom_id": custom_id,
                "component_type": 2,
                "values": ["one"]
            }
        }))
    }

    fn modal_interaction(id: &str, custom_id: &str) -> crate::event::Event {
        interaction_event(json!({
            "id": id,
            "application_id": "2",
            "token": "token",
            "type": 5,
            "data": {
                "custom_id": custom_id,
                "components": []
            }
        }))
    }

    fn message_create(id: &str, content: &str) -> crate::event::Event {
        decode_event(
            "MESSAGE_CREATE",
            json!({
                "id": id,
                "channel_id": "1",
                "content": content,
                "mentions": [],
                "attachments": []
            }),
        )
        .expect("valid message event")
    }

    #[tokio::test(start_paused = true)]
    async fn message_collector_stops_after_max_items() {
        let hub = CollectorHub::new();
        let collector = hub
            .message_collector()
            .max_items(1)
            .timeout(Duration::from_secs(1));

        hub.publish(
            decode_event(
                "MESSAGE_CREATE",
                json!({
                    "id": "2",
                    "channel_id": "1",
                    "content": "hello",
                    "mentions": [],
                    "attachments": []
                }),
            )
            .unwrap(),
        );

        let messages = collector.collect().await;
        assert_eq!(messages.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn message_collector_reports_lagged_events() {
        let hub = CollectorHub::with_capacity(1);
        let mut collector = hub.message_collector().timeout(Duration::from_secs(1));

        for id in ["2", "3", "4"] {
            hub.publish(
                decode_event(
                    "MESSAGE_CREATE",
                    json!({
                        "id": id,
                        "channel_id": "1",
                        "content": format!("message-{id}"),
                        "mentions": [],
                        "attachments": []
                    }),
                )
                .unwrap(),
            );
        }

        let message = collector
            .next()
            .await
            .expect("collector should still yield the newest buffered message");
        assert_eq!(message.id.as_str(), "4");
        assert!(collector.lagged_events() >= 1);
    }

    #[tokio::test(start_paused = true)]
    async fn default_hub_and_message_filter_cover_default_paths() {
        let hub = CollectorHub::default();
        let mut collector = hub
            .message_collector()
            .filter(|message| message.content == "keep")
            .timeout(Duration::from_secs(1));

        hub.publish(
            decode_event(
                "MESSAGE_CREATE",
                json!({
                    "id": "2",
                    "channel_id": "1",
                    "content": "skip",
                    "mentions": [],
                    "attachments": []
                }),
            )
            .unwrap(),
        );
        hub.publish(ping_interaction("3"));
        hub.publish(
            decode_event(
                "MESSAGE_UPDATE",
                json!({
                    "id": "4",
                    "channel_id": "1",
                    "content": "keep",
                    "mentions": [],
                    "attachments": []
                }),
            )
            .unwrap(),
        );

        let message = collector.next().await.expect("matching message update");
        assert_eq!(message.id.as_str(), "4");
        assert_eq!(message.content, "keep");
    }

    #[tokio::test(start_paused = true)]
    async fn interaction_component_and_modal_collectors_yield_typed_variants() {
        let hub = CollectorHub::new();
        let mut interaction_collector = hub
            .interaction_collector()
            .filter(|interaction| matches!(interaction, Interaction::Component(_)))
            .timeout(Duration::from_secs(1));
        let mut component_collector = hub.component_collector().timeout(Duration::from_secs(1));
        let mut modal_collector = hub.modal_collector().timeout(Duration::from_secs(1));

        hub.publish(
            decode_event(
                "INTERACTION_CREATE",
                json!({
                    "id": "1",
                    "application_id": "2",
                    "token": "token",
                    "type": 1
                }),
            )
            .unwrap(),
        );
        hub.publish(
            decode_event(
                "INTERACTION_CREATE",
                json!({
                    "id": "3",
                    "application_id": "4",
                    "token": "token",
                    "type": 3,
                    "data": {
                        "custom_id": "button",
                        "component_type": 2,
                        "values": ["one"]
                    }
                }),
            )
            .unwrap(),
        );
        hub.publish(
            decode_event(
                "INTERACTION_CREATE",
                json!({
                    "id": "5",
                    "application_id": "6",
                    "token": "token",
                    "type": 5,
                    "data": {
                        "custom_id": "modal",
                        "components": []
                    }
                }),
            )
            .unwrap(),
        );

        match interaction_collector.next().await {
            Some(Interaction::Component(component)) => {
                assert_eq!(component.data.custom_id, "button");
            }
            other => panic!("unexpected filtered interaction: {other:?}"),
        }

        let component = component_collector
            .next()
            .await
            .expect("component interaction");
        assert_eq!(component.context.id.as_str(), "3");
        assert_eq!(component.context.application_id.as_str(), "4");
        assert_eq!(component.context.token, "token");
        assert_eq!(component.data.custom_id, "button");
        assert_eq!(component.data.component_type, 2);
        assert_eq!(component.data.values, vec!["one".to_string()]);

        let modal = modal_collector.next().await.expect("modal interaction");
        assert_eq!(modal.context.id.as_str(), "5");
        assert_eq!(modal.submission.custom_id, "modal");
    }

    #[tokio::test(start_paused = true)]
    async fn interaction_collector_filter_skips_non_matches_and_max_items_chain_is_usable() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .interaction_collector()
            .max_items(1)
            .filter(|interaction| matches!(interaction, Interaction::Component(_)))
            .timeout(Duration::from_secs(1));

        hub.publish(ping_interaction("1"));
        hub.publish(component_interaction("2", "keep-me"));

        match collector.next().await {
            Some(Interaction::Component(component)) => {
                assert_eq!(component.context.id.as_str(), "2");
                assert_eq!(component.data.custom_id, "keep-me");
            }
            other => panic!("unexpected interaction after filter skip: {other:?}"),
        }
    }

    #[tokio::test(start_paused = true)]
    async fn interaction_collector_collect_stops_at_max_items() {
        let hub = CollectorHub::new();
        let collector = hub
            .interaction_collector()
            .max_items(2)
            .timeout(Duration::from_secs(1));

        hub.publish(component_interaction("1", "first"));
        hub.publish(component_interaction("2", "second"));
        hub.publish(component_interaction("3", "third"));

        let interactions = collector.collect().await;
        assert_eq!(interactions.len(), 2);
        assert_eq!(interactions[0].id().as_str(), "1");
        assert_eq!(interactions[1].id().as_str(), "2");
    }

    #[tokio::test(start_paused = true)]
    async fn component_and_modal_collectors_collect_and_expose_lagged_counts() {
        let component_hub = CollectorHub::new();
        let component_collector = component_hub
            .component_collector()
            .max_items(2)
            .timeout(Duration::from_secs(1));
        component_hub.publish(component_interaction("1", "first"));
        component_hub.publish(component_interaction("2", "second"));
        component_hub.publish(component_interaction("3", "third"));

        let components = component_collector.collect().await;
        assert_eq!(components.len(), 2);
        assert_eq!(components[0].data.custom_id, "first");
        assert_eq!(components[1].data.custom_id, "second");

        let modal_hub = CollectorHub::with_capacity(1);
        let mut lagged_modal_collector = modal_hub
            .modal_collector()
            .max_items(1)
            .timeout(Duration::from_secs(1));
        modal_hub.publish(component_interaction("4", "skipped"));
        modal_hub.publish(modal_interaction("5", "latest"));
        let modal = lagged_modal_collector.next().await.expect("latest modal");
        assert_eq!(modal.submission.custom_id, "latest");
        assert!(lagged_modal_collector.lagged_events() >= 1);

        let modal_collector = modal_hub
            .modal_collector()
            .max_items(1)
            .timeout(Duration::from_secs(1));
        modal_hub.publish(modal_interaction("6", "collected"));
        let modals = modal_collector.collect().await;
        assert_eq!(modals.len(), 1);
        assert_eq!(modals[0].submission.custom_id, "collected");
    }

    #[tokio::test(start_paused = true)]
    async fn component_collector_timeout_is_absolute_across_non_matching_interactions() {
        let hub = CollectorHub::new();
        let mut collector = hub.component_collector().timeout(Duration::from_millis(30));
        let publisher = hub.clone();

        tokio::spawn(async move {
            for id in 0..20 {
                publisher.publish(ping_interaction(&id.to_string()));
                time::sleep(Duration::from_millis(10)).await;
            }
        });

        let result = time::timeout(Duration::from_millis(120), collector.next())
            .await
            .expect("collector timeout should not reset for every non-component interaction");
        assert!(result.is_none());
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::Time));
    }

    #[tokio::test(start_paused = true)]
    async fn interaction_collector_reports_lagged_interactions() {
        let hub = CollectorHub::with_capacity(1);
        let mut collector = hub.interaction_collector().timeout(Duration::from_secs(1));

        hub.publish(ping_interaction("1"));
        hub.publish(component_interaction("2", "older"));
        hub.publish(modal_interaction("3", "latest"));

        let interaction = collector
            .next()
            .await
            .expect("collector should yield the newest buffered interaction");
        match interaction {
            Interaction::ModalSubmit(modal) => {
                assert_eq!(modal.context.id.as_str(), "3");
                assert_eq!(modal.submission.custom_id, "latest");
            }
            other => panic!("unexpected interaction after lag: {other:?}"),
        }
        assert!(collector.lagged_events() >= 1);
    }

    #[tokio::test(start_paused = true)]
    async fn component_collector_returns_none_after_only_non_component_events() {
        let hub = CollectorHub::new();
        let mut collector = hub.component_collector().timeout(Duration::from_secs(1));

        hub.publish(ping_interaction("1"));
        hub.publish(modal_interaction("2", "not-a-component"));
        drop(hub);

        assert!(collector.next().await.is_none());
        assert_eq!(
            collector.end_reason(),
            Some(CollectorEndReason::ChannelDropped)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn component_collector_times_out_when_no_component_arrives() {
        let hub = CollectorHub::new();
        let mut collector = hub.component_collector().timeout(Duration::from_millis(20));

        hub.publish(ping_interaction("1"));

        assert!(collector.next().await.is_none());
        assert_eq!(collector.lagged_events(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn modal_collector_returns_none_after_only_non_modal_events() {
        let hub = CollectorHub::new();
        let mut collector = hub.modal_collector().timeout(Duration::from_secs(1));

        hub.publish(ping_interaction("1"));
        hub.publish(component_interaction("2", "button"));
        drop(hub);

        assert!(collector.next().await.is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn modal_collector_times_out_when_no_modal_arrives() {
        let hub = CollectorHub::new();
        let mut collector = hub.modal_collector().timeout(Duration::from_millis(20));

        hub.publish(component_interaction("1", "button"));

        assert!(collector.next().await.is_none());
        assert_eq!(collector.lagged_events(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn idle_timeout_ends_collection_before_slower_overall_timeout() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .message_collector()
            .timeout(Duration::from_secs(5))
            .idle(Duration::from_millis(40));

        let started = time::Instant::now();
        assert!(collector.next().await.is_none());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "idle should end collection long before the overall timeout"
        );
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::Idle));
    }

    #[tokio::test(start_paused = true)]
    async fn idle_window_resets_on_each_collected_item() {
        let hub = CollectorHub::new();
        let mut collector = hub.message_collector().idle(Duration::from_millis(100));
        let publisher = hub.clone();

        tokio::spawn(async move {
            for id in 10..13 {
                time::sleep(Duration::from_millis(30)).await;
                publisher.publish(message_create(&id.to_string(), "tick"));
            }
        });

        let mut count = 0;
        while collector.next().await.is_some() {
            count += 1;
        }
        assert_eq!(count, 3);
        assert_eq!(collector.received_count(), 3);
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::Idle));
    }

    #[tokio::test(start_paused = true)]
    async fn overall_timeout_records_time_end_reason() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .interaction_collector()
            .timeout(Duration::from_millis(30));

        assert!(collector.next().await.is_none());
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::Time));
    }

    #[tokio::test(start_paused = true)]
    async fn stop_with_reason_ends_in_flight_next_and_records_reason() {
        let hub = CollectorHub::new();
        let mut collector = hub.component_collector();
        let handle = collector.stop_handle();

        tokio::spawn(async move {
            time::sleep(Duration::from_millis(20)).await;
            handle.stop_with_reason("done");
        });

        let result = time::timeout(Duration::from_millis(500), collector.next())
            .await
            .expect("stop_with_reason should end the in-flight next()");
        assert!(result.is_none());
        assert_eq!(
            collector.end_reason(),
            Some(CollectorEndReason::User("done".to_string()))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn stop_makes_subsequent_next_return_none_with_default_reason() {
        let hub = CollectorHub::new();
        let mut collector = hub.message_collector();

        hub.publish(message_create("2", "ignored after stop"));
        collector.stop();

        assert!(collector.next().await.is_none());
        assert!(collector.next().await.is_none());
        assert_eq!(collector.received_count(), 0);
        assert_eq!(
            collector.end_reason(),
            Some(CollectorEndReason::User("user".to_string()))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn first_stop_reason_wins() {
        let hub = CollectorHub::new();
        let mut collector = hub.modal_collector();

        collector.stop_with_reason("first");
        collector.stop_with_reason("second");
        collector.stop();

        assert!(collector.next().await.is_none());
        assert_eq!(
            collector.end_reason(),
            Some(CollectorEndReason::User("first".to_string()))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn component_collector_filter_only_passes_matching_custom_ids() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .component_collector()
            .filter(|component| component.data.custom_id == "keep")
            .timeout(Duration::from_secs(1));

        hub.publish(component_interaction("1", "drop"));
        hub.publish(ping_interaction("2"));
        hub.publish(component_interaction("3", "keep"));

        let component = collector.next().await.expect("matching component");
        assert_eq!(component.context.id.as_str(), "3");
        assert_eq!(component.data.custom_id, "keep");
        assert_eq!(collector.received_count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn modal_collector_filter_only_passes_matching_custom_ids() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .modal_collector()
            .filter(|modal| modal.submission.custom_id == "wanted")
            .timeout(Duration::from_secs(1));

        hub.publish(modal_interaction("1", "other"));
        hub.publish(modal_interaction("2", "wanted"));

        let modal = collector.next().await.expect("matching modal");
        assert_eq!(modal.context.id.as_str(), "2");
        assert_eq!(modal.submission.custom_id, "wanted");
        assert_eq!(collector.received_count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn reset_timer_extends_idle_deadline() {
        let hub = CollectorHub::new();
        let mut collector = hub.message_collector().idle(Duration::from_millis(100));

        collector.reset_timer();
        time::sleep(Duration::from_millis(60)).await;
        collector.reset_timer();

        let started = time::Instant::now();
        assert!(collector.next().await.is_none());
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(80),
            "reset_timer should have restarted the idle window, elapsed {elapsed:?}"
        );
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::Idle));
    }

    #[tokio::test(start_paused = true)]
    async fn max_items_records_limit_end_reason_and_ends_next() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .message_collector()
            .max_items(1)
            .timeout(Duration::from_secs(1));

        hub.publish(message_create("2", "hello"));
        hub.publish(message_create("3", "beyond the limit"));

        assert!(collector.next().await.is_some());
        assert_eq!(collector.received_count(), 1);
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::Limit));
        assert!(collector.next().await.is_none());
        assert_eq!(collector.received_count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn end_reason_is_none_while_collector_is_live() {
        let hub = CollectorHub::new();
        let mut collector = hub.message_collector().timeout(Duration::from_secs(1));

        hub.publish(message_create("2", "hello"));

        assert!(collector.next().await.is_some());
        assert_eq!(collector.end_reason(), None);
        assert_eq!(collector.received_count(), 1);
    }

    // ---- reaction collector -------------------------------------------------

    const MESSAGE: &str = "500";
    const CHANNEL: &str = "10";
    const GUILD: &str = "20";

    fn unicode(name: &str) -> Value {
        json!({ "id": null, "name": name })
    }

    fn custom(id: &str, name: &str) -> Value {
        json!({ "id": id, "name": name, "animated": false })
    }

    fn reaction_payload(message_id: &str, user_id: &str, emoji: Value, burst: bool) -> Value {
        json!({
            "user_id": user_id,
            "channel_id": CHANNEL,
            "message_id": message_id,
            "guild_id": GUILD,
            "emoji": emoji,
            "burst": burst,
            "type": if burst { 1 } else { 0 }
        })
    }

    fn reaction_add(message_id: &str, user_id: &str, emoji: Value) -> Event {
        let mut payload = reaction_payload(message_id, user_id, emoji, false);
        // Only MESSAGE_REACTION_ADD carries the author, which lets a test
        // write a filter that passes adds but rejects removals.
        payload["message_author_id"] = json!("99");
        decode_event("MESSAGE_REACTION_ADD", payload).expect("valid reaction add")
    }

    fn burst_add(message_id: &str, user_id: &str, emoji: Value) -> Event {
        decode_event(
            "MESSAGE_REACTION_ADD",
            reaction_payload(message_id, user_id, emoji, true),
        )
        .expect("valid burst reaction add")
    }

    fn reaction_remove(message_id: &str, user_id: &str, emoji: Value, burst: bool) -> Event {
        decode_event(
            "MESSAGE_REACTION_REMOVE",
            reaction_payload(message_id, user_id, emoji, burst),
        )
        .expect("valid reaction remove")
    }

    fn remove_all(message_id: &str) -> Event {
        decode_event(
            "MESSAGE_REACTION_REMOVE_ALL",
            json!({ "channel_id": CHANNEL, "message_id": message_id, "guild_id": GUILD }),
        )
        .expect("valid remove all")
    }

    fn remove_emoji(message_id: &str, emoji: Value) -> Event {
        decode_event(
            "MESSAGE_REACTION_REMOVE_EMOJI",
            json!({
                "channel_id": CHANNEL,
                "message_id": message_id,
                "guild_id": GUILD,
                "emoji": emoji
            }),
        )
        .expect("valid remove emoji")
    }

    fn key(name: &str) -> EmojiKey {
        EmojiKey::unicode(name)
    }

    fn user(id: &str) -> Snowflake {
        Snowflake::from(id)
    }

    fn users_of(collection: &ReactionCollection, emoji: &str) -> Vec<String> {
        collection
            .get(&key(emoji))
            .map(|reaction| reaction.users().iter().map(ToString::to_string).collect())
            .unwrap_or_default()
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_tallies_users_per_emoji_in_first_seen_order() {
        let hub = CollectorHub::new();
        let collector = hub
            .reaction_collector(MESSAGE)
            .timeout(Duration::from_secs(30));

        hub.publish(reaction_add(MESSAGE, "1", unicode("✅")));
        hub.publish(reaction_add(MESSAGE, "2", custom("777", "party")));
        hub.publish(reaction_add(MESSAGE, "2", unicode("✅")));
        hub.publish(burst_add(MESSAGE, "3", unicode("✅")));

        let collected = collector.collect().await;
        let keys: Vec<&EmojiKey> = collected.iter().map(|(key, _)| key).collect();
        assert_eq!(keys, vec![&key("✅"), &EmojiKey::custom("777")]);
        assert_eq!(collected.len(), 2);
        assert!(!collected.is_empty());
        assert_eq!(collected.total(), 4);
        assert_eq!(collected.user_count(), 3);
        assert_eq!(
            collected.users().map(Snowflake::as_str).collect::<Vec<_>>(),
            vec!["1", "2", "3"]
        );

        let check = collected.get(&key("✅")).expect("✅ collected");
        assert_eq!(users_of(&collected, "✅"), vec!["1", "2"]);
        assert_eq!(
            check.burst_users().iter().collect::<Vec<_>>(),
            vec![&user("3")]
        );
        assert_eq!(check.count(), 3);
        assert!(check.contains_user(&user("3")));
        assert!(!check.contains_user(&user("4")));
        assert_eq!(check.emoji().name.as_deref(), Some("✅"));

        let party = collected
            .get(&EmojiKey::custom(777_u64))
            .expect("custom emoji keyed by id");
        assert_eq!(party.emoji().name.as_deref(), Some("party"));
        assert_eq!(party.count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_yields_only_matching_message_and_filter() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .reaction_collector(MESSAGE)
            .filter(|reaction| reaction.user_id.as_ref().map(Snowflake::as_str) == Some("1"))
            .timeout(Duration::from_secs(30));
        assert_eq!(collector.message_id().as_str(), MESSAGE);

        hub.publish(reaction_add("501", "1", unicode("✅")));
        hub.publish(reaction_add(MESSAGE, "2", unicode("✅")));
        hub.publish(message_create("600", "not a reaction"));
        hub.publish(reaction_add(MESSAGE, "1", unicode("❌")));

        let reaction = collector.next().await.expect("matching reaction");
        assert_eq!(reaction.user_id, Some(user("1")));
        assert_eq!(
            reaction.emoji.and_then(|emoji| emoji.name).as_deref(),
            Some("❌")
        );
        assert_eq!(collector.received_count(), 1);
        assert_eq!(collector.collected().len(), 1);
        assert!(collector.collected().get(&key("✅")).is_none());
        assert_eq!(collector.end_reason(), None);
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_skips_events_without_emoji_or_user() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .reaction_collector(MESSAGE)
            .timeout(Duration::from_secs(30));

        hub.publish(
            decode_event(
                "MESSAGE_REACTION_ADD",
                json!({ "user_id": "1", "message_id": MESSAGE }),
            )
            .expect("reaction without emoji"),
        );
        hub.publish(
            decode_event(
                "MESSAGE_REACTION_ADD",
                json!({ "message_id": MESSAGE, "emoji": unicode("✅") }),
            )
            .expect("reaction without user"),
        );
        hub.publish(
            decode_event(
                "MESSAGE_REACTION_ADD",
                json!({ "user_id": "1", "message_id": MESSAGE, "emoji": { "id": null, "name": null } }),
            )
            .expect("reaction with an empty emoji"),
        );
        hub.publish(reaction_add(MESSAGE, "2", unicode("✅")));

        let reaction = collector.next().await.expect("well-formed reaction");
        assert_eq!(reaction.user_id, Some(user("2")));
        assert_eq!(collector.received_count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_max_items_ends_with_limit() {
        let hub = CollectorHub::new();
        let collector = hub
            .reaction_collector(MESSAGE)
            .max_items(2)
            .timeout(Duration::from_secs(30));

        hub.publish(reaction_add(MESSAGE, "1", unicode("✅")));
        hub.publish(reaction_add(MESSAGE, "2", unicode("✅")));
        hub.publish(reaction_add(MESSAGE, "3", unicode("✅")));

        let mut collector = collector;
        assert!(collector.next().await.is_some());
        assert!(collector.next().await.is_some());
        assert!(collector.next().await.is_none());
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::Limit));
        assert_eq!(collector.collected().total(), 2);
        assert_eq!(users_of(collector.collected(), "✅"), vec!["1", "2"]);
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_max_emojis_ends_with_emoji_limit() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .reaction_collector(MESSAGE)
            .max_emojis(2)
            .timeout(Duration::from_secs(30));

        hub.publish(reaction_add(MESSAGE, "1", unicode("✅")));
        hub.publish(reaction_add(MESSAGE, "2", unicode("✅")));
        hub.publish(reaction_add(MESSAGE, "3", unicode("❌")));
        hub.publish(reaction_add(MESSAGE, "4", unicode("🔥")));

        let mut yielded = 0;
        while collector.next().await.is_some() {
            yielded += 1;
        }
        assert_eq!(yielded, 3, "a repeated emoji does not count twice");
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::EmojiLimit));
        assert_eq!(collector.collected().len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_max_users_ends_with_user_limit() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .reaction_collector(MESSAGE)
            .max_users(2)
            .timeout(Duration::from_secs(30));

        hub.publish(reaction_add(MESSAGE, "1", unicode("✅")));
        hub.publish(reaction_add(MESSAGE, "1", unicode("❌")));
        hub.publish(reaction_add(MESSAGE, "2", unicode("✅")));
        hub.publish(reaction_add(MESSAGE, "3", unicode("✅")));

        let mut yielded = 0;
        while collector.next().await.is_some() {
            yielded += 1;
        }
        assert_eq!(
            yielded, 3,
            "a second emoji from the same user is not a new user"
        );
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::UserLimit));
        assert_eq!(collector.collected().user_count(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_zero_limits_end_before_waiting() {
        let hub = CollectorHub::new();
        for (collector, expected) in [
            (
                hub.reaction_collector(MESSAGE).max_items(0),
                CollectorEndReason::Limit,
            ),
            (
                hub.reaction_collector(MESSAGE).max_emojis(0),
                CollectorEndReason::EmojiLimit,
            ),
            (
                hub.reaction_collector(MESSAGE).max_users(0),
                CollectorEndReason::UserLimit,
            ),
        ] {
            let mut collector = collector;
            assert!(collector.next().await.is_none());
            assert_eq!(collector.end_reason(), Some(expected));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_without_dispose_ignores_removals() {
        let hub = CollectorHub::new();
        let collector = hub.reaction_collector(MESSAGE).idle(Duration::from_secs(5));

        hub.publish(reaction_add(MESSAGE, "1", unicode("✅")));
        hub.publish(reaction_add(MESSAGE, "2", unicode("❌")));
        hub.publish(reaction_remove(MESSAGE, "1", unicode("✅"), false));
        hub.publish(remove_emoji(MESSAGE, unicode("❌")));
        hub.publish(remove_all(MESSAGE));

        let collected = collector.collect().await;
        assert_eq!(collected.total(), 2);
        assert_eq!(users_of(&collected, "✅"), vec!["1"]);
        assert_eq!(users_of(&collected, "❌"), vec!["2"]);
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_dispose_applies_each_removal_kind() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .reaction_collector(MESSAGE)
            .dispose(true)
            .idle(Duration::from_secs(5));

        hub.publish(reaction_add(MESSAGE, "1", unicode("✅")));
        hub.publish(reaction_add(MESSAGE, "2", unicode("✅")));
        hub.publish(reaction_add(MESSAGE, "1", unicode("❌")));
        hub.publish(reaction_add(MESSAGE, "3", custom("777", "party")));
        // Removals for other messages, unknown emojis, and users who were
        // never collected change nothing.
        hub.publish(reaction_remove("501", "1", unicode("✅"), false));
        hub.publish(reaction_remove(MESSAGE, "9", unicode("✅"), false));
        hub.publish(reaction_remove(MESSAGE, "1", unicode("🔥"), false));
        hub.publish(remove_emoji("501", unicode("✅")));
        hub.publish(remove_emoji(MESSAGE, unicode("🔥")));
        hub.publish(remove_all("501"));
        // A real removal, then a whole emoji.
        hub.publish(reaction_remove(MESSAGE, "1", unicode("✅"), false));
        hub.publish(remove_emoji(MESSAGE, custom("777", "renamed")));

        for _ in 0..4 {
            assert!(collector.next().await.is_some());
        }
        assert!(collector.next().await.is_none());
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::Idle));
        let collected = collector.collected();
        assert_eq!(users_of(collected, "✅"), vec!["2"]);
        assert_eq!(users_of(collected, "❌"), vec!["1"]);
        assert!(collected.get(&EmojiKey::custom("777")).is_none());
        assert_eq!(collected.total(), 2);
        assert_eq!(collected.user_count(), 2, "user 3 left with the emoji");

        // Removing the last user of an emoji drops the emoji entirely.
        let hub = CollectorHub::new();
        let collector = hub
            .reaction_collector(MESSAGE)
            .dispose(true)
            .idle(Duration::from_secs(5));
        hub.publish(reaction_add(MESSAGE, "1", unicode("✅")));
        hub.publish(reaction_add(MESSAGE, "2", unicode("❌")));
        hub.publish(reaction_remove(MESSAGE, "2", unicode("❌"), false));
        hub.publish(remove_all(MESSAGE));
        hub.publish(reaction_add(MESSAGE, "3", unicode("🔥")));
        let collected = collector.collect().await;
        assert_eq!(
            collected.len(),
            1,
            "REMOVE_ALL cleared everything before 🔥"
        );
        assert_eq!(users_of(&collected, "🔥"), vec!["3"]);
        assert_eq!(collected.total(), 1);
        assert_eq!(collected.user_count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_dispose_tracks_burst_reactions_separately() {
        let hub = CollectorHub::new();
        let collector = hub
            .reaction_collector(MESSAGE)
            .dispose(true)
            .idle(Duration::from_secs(5));

        hub.publish(reaction_add(MESSAGE, "1", unicode("✅")));
        hub.publish(burst_add(MESSAGE, "1", unicode("✅")));
        hub.publish(reaction_remove(MESSAGE, "1", unicode("✅"), true));

        let collected = collector.collect().await;
        let check = collected.get(&key("✅")).expect("normal reaction remains");
        assert_eq!(check.count(), 1);
        assert!(check.burst_users().is_empty());
        assert!(check.contains_user(&user("1")));
        assert_eq!(collected.total(), 1);
        assert_eq!(collected.user_count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_disposed_reactions_free_room_under_max_items() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .reaction_collector(MESSAGE)
            .dispose(true)
            .max_items(2)
            .timeout(Duration::from_secs(30));

        hub.publish(reaction_add(MESSAGE, "1", unicode("✅")));
        hub.publish(reaction_remove(MESSAGE, "1", unicode("✅"), false));
        hub.publish(reaction_add(MESSAGE, "2", unicode("✅")));
        hub.publish(reaction_add(MESSAGE, "3", unicode("✅")));

        let mut yielded = 0;
        while collector.next().await.is_some() {
            yielded += 1;
        }
        assert_eq!(yielded, 3);
        assert_eq!(collector.received_count(), 3);
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::Limit));
        assert_eq!(users_of(collector.collected(), "✅"), vec!["2", "3"]);
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_filter_also_gates_disposals() {
        let hub = CollectorHub::new();
        // Removal payloads carry no `message_author_id`, so this filter
        // accepts every add and rejects every removal, as in discord.js.
        let collector = hub
            .reaction_collector(MESSAGE)
            .dispose(true)
            .filter(|reaction| reaction.message_author_id.is_some())
            .idle(Duration::from_secs(5));

        hub.publish(reaction_add(MESSAGE, "1", unicode("✅")));
        hub.publish(reaction_remove(MESSAGE, "1", unicode("✅"), false));

        let collected = collector.collect().await;
        assert_eq!(users_of(&collected, "✅"), vec!["1"]);
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_ends_when_the_message_is_deleted() {
        let hub = CollectorHub::new();
        let mut single = hub.reaction_collector(MESSAGE);

        hub.publish(
            decode_event(
                "MESSAGE_DELETE",
                json!({ "id": "501", "channel_id": CHANNEL }),
            )
            .expect("other message delete"),
        );
        hub.publish(
            decode_event(
                "MESSAGE_DELETE_BULK",
                json!({ "ids": ["502", "503"], "channel_id": CHANNEL }),
            )
            .expect("unrelated bulk delete"),
        );
        hub.publish(
            decode_event(
                "MESSAGE_DELETE",
                json!({ "id": MESSAGE, "channel_id": CHANNEL }),
            )
            .expect("message delete"),
        );
        assert!(single.next().await.is_none());
        assert_eq!(single.end_reason(), Some(CollectorEndReason::MessageDelete));

        let mut bulk_only = hub.reaction_collector(MESSAGE);
        hub.publish(
            decode_event(
                "MESSAGE_DELETE_BULK",
                json!({ "ids": ["499", MESSAGE], "channel_id": CHANNEL }),
            )
            .expect("bulk delete"),
        );
        assert!(bulk_only.next().await.is_none());
        assert_eq!(
            bulk_only.end_reason(),
            Some(CollectorEndReason::MessageDelete)
        );
    }

    fn channel_delete(id: &str) -> Event {
        decode_event("CHANNEL_DELETE", json!({ "id": id, "type": 0 })).expect("channel delete")
    }

    fn thread_delete(id: &str) -> Event {
        decode_event(
            "THREAD_DELETE",
            json!({ "id": id, "type": 11, "guild_id": GUILD, "parent_id": "5" }),
        )
        .expect("thread delete")
    }

    fn guild_delete(id: &str, unavailable: Option<bool>) -> Event {
        let mut payload = json!({ "id": id });
        if let Some(unavailable) = unavailable {
            payload["unavailable"] = json!(unavailable);
        }
        decode_event("GUILD_DELETE", payload).expect("guild delete")
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_learns_location_from_the_first_reaction() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .reaction_collector(MESSAGE)
            .timeout(Duration::from_secs(30));

        // Before any reaction the channel is unknown, so this is ignored.
        hub.publish(channel_delete(CHANNEL));
        hub.publish(reaction_add(MESSAGE, "1", unicode("✅")));
        hub.publish(channel_delete("11"));
        hub.publish(channel_delete(CHANNEL));

        assert!(collector.next().await.is_some());
        assert!(collector.next().await.is_none());
        assert_eq!(
            collector.end_reason(),
            Some(CollectorEndReason::ChannelDelete)
        );
        assert_eq!(collector.collected().total(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_for_message_ends_on_thread_and_guild_deletion() {
        let hub = CollectorHub::new();
        let message: Message = serde_json::from_value(json!({
            "id": MESSAGE,
            "channel_id": CHANNEL,
            "guild_id": GUILD,
            "content": "vote",
            "mentions": [],
            "attachments": []
        }))
        .expect("valid message");

        let mut thread_watch = hub.reaction_collector_for_message(&message);
        hub.publish(thread_delete("11"));
        hub.publish(thread_delete(CHANNEL));
        assert!(thread_watch.next().await.is_none());
        assert_eq!(
            thread_watch.end_reason(),
            Some(CollectorEndReason::ThreadDelete)
        );

        let mut guild_watch = hub.reaction_collector_for_message(&message);
        hub.publish(guild_delete(GUILD, Some(true)));
        hub.publish(guild_delete("21", None));
        hub.publish(guild_delete(GUILD, Some(false)));
        assert!(guild_watch.next().await.is_none());
        assert_eq!(
            guild_watch.end_reason(),
            Some(CollectorEndReason::GuildDelete)
        );

        let mut explicit = hub
            .reaction_collector(MESSAGE)
            .channel_id(CHANNEL)
            .guild_id(GUILD);
        hub.publish(guild_delete(GUILD, None));
        assert!(explicit.next().await.is_none());
        assert_eq!(explicit.end_reason(), Some(CollectorEndReason::GuildDelete));
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_time_idle_and_stop_controls() {
        let hub = CollectorHub::new();

        let mut timed = hub
            .reaction_collector(MESSAGE)
            .timeout(Duration::from_secs(60));
        let started = time::Instant::now();
        assert!(timed.next().await.is_none());
        assert_eq!(started.elapsed(), Duration::from_secs(60));
        assert_eq!(timed.end_reason(), Some(CollectorEndReason::Time));

        let mut idle = hub
            .reaction_collector(MESSAGE)
            .timeout(Duration::from_secs(60))
            .idle(Duration::from_secs(10));
        let publisher = hub.clone();
        tokio::spawn(async move {
            time::sleep(Duration::from_secs(8)).await;
            publisher.publish(reaction_add(MESSAGE, "1", unicode("✅")));
        });
        let started = time::Instant::now();
        assert!(idle.next().await.is_some());
        assert!(idle.next().await.is_none());
        assert_eq!(
            started.elapsed(),
            Duration::from_secs(18),
            "the idle window restarts after each collected reaction"
        );
        assert_eq!(idle.end_reason(), Some(CollectorEndReason::Idle));

        let mut stopped = hub.reaction_collector(MESSAGE);
        let handle = stopped.stop_handle();
        tokio::spawn(async move {
            time::sleep(Duration::from_secs(1)).await;
            handle.stop_with_reason("vote closed");
        });
        assert!(stopped.next().await.is_none());
        assert_eq!(
            stopped.end_reason(),
            Some(CollectorEndReason::User("vote closed".to_string()))
        );

        let mut reset = hub
            .reaction_collector(MESSAGE)
            .timeout(Duration::from_secs(10));
        reset.reset_timer();
        time::sleep(Duration::from_secs(6)).await;
        reset.reset_timer();
        let started = time::Instant::now();
        assert!(reset.next().await.is_none());
        assert_eq!(started.elapsed(), Duration::from_secs(10));

        let mut user_stop = hub.reaction_collector(MESSAGE);
        user_stop.stop();
        user_stop.stop_with_reason("ignored");
        assert!(user_stop.next().await.is_none());
        assert_eq!(
            user_stop.end_reason(),
            Some(CollectorEndReason::User("user".to_string()))
        );
    }

    #[tokio::test(start_paused = true)]
    async fn reaction_collector_reports_lag_and_channel_drop() {
        let hub = CollectorHub::with_capacity(2);
        let mut collector = hub.reaction_collector(MESSAGE);

        for user_id in ["1", "2", "3", "4"] {
            hub.publish(reaction_add(MESSAGE, user_id, unicode("✅")));
        }
        drop(hub);

        let mut yielded = Vec::new();
        while let Some(reaction) = collector.next().await {
            yielded.extend(reaction.user_id.map(|id| id.to_string()));
        }
        assert_eq!(yielded, vec!["3", "4"], "the two oldest adds were lost");
        assert_eq!(collector.lagged_events(), 2);
        assert_eq!(
            collector.end_reason(),
            Some(CollectorEndReason::ChannelDropped)
        );
        assert_eq!(users_of(collector.collected(), "✅"), vec!["3", "4"]);
    }

    #[test]
    fn emoji_key_from_emoji_prefers_custom_ids() {
        assert_eq!(
            EmojiKey::from_emoji(&Emoji::custom("party", "777", true)),
            Some(EmojiKey::Custom(Snowflake::from("777")))
        );
        assert_eq!(
            EmojiKey::from_emoji(&Emoji::unicode("✅")),
            Some(EmojiKey::Unicode("✅".to_string()))
        );
        assert_eq!(EmojiKey::from_emoji(&Emoji::default()), None);
        assert_ne!(EmojiKey::unicode("❤"), EmojiKey::unicode("❤\u{fe0f}"));
    }

    #[test]
    fn collector_futures_are_send() {
        fn assert_send<T: Send>(_: T) {}
        let hub = CollectorHub::new();
        let mut reactions = hub.reaction_collector(MESSAGE);
        assert_send(reactions.next());
        assert_send(hub.reaction_collector(MESSAGE).collect());
        let mut messages = hub.message_collector();
        assert_send(messages.next());
        assert_send(hub.component_collector().collect());
    }
}
