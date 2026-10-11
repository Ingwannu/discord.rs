//! Open numeric discriminants for Discord's integer enumerations.
//!
//! Every type here is a transparent newtype over the wire integer with named
//! associated constants for the values Discord documents. Unknown values are
//! kept as-is and written back unchanged, so a value Discord adds tomorrow
//! never fails decoding or gets rewritten.
//!
//! ```
//! use discordrs::{ChannelType, MessageType};
//!
//! let kind = ChannelType::PUBLIC_THREAD;
//! assert!(kind.is_thread());
//! assert_eq!(format!("{kind:?}"), "PUBLIC_THREAD");
//!
//! let future = MessageType::new(250);
//! assert_eq!(future.name(), None);
//! assert_eq!(format!("{future:?}"), "Unknown(250)");
//! ```
//!
//! # Where these types live
//!
//! This module is the canonical home of every discriminant. All of them are
//! also re-exported from [`crate::model`] and the crate root, so
//! `discordrs::ChannelType` and `discordrs::model::discriminant::ChannelType`
//! name the same type. [`WebhookPayloadType`] is additionally re-exported from
//! [`crate::webhook_events`], its 2.x home.
//!
//! Since phase 3 series 3 every model field, request body, query, event
//! field, and builder setter that carries one of these values uses the
//! newtype. The one deliberate exception is
//! [`MessageComponent::style`](crate::model::MessageComponent::style), whose
//! meaning depends on the component type; its typed accessors are
//! `button_style()` and `text_input_style()`. Seven names replaced 2.x types
//! of the same name; the 2.x versions are gone, not deprecated:
//!
//! | Type | 2.x definition it replaced |
//! |---|---|
//! | [`ChannelType`] | closed `#[non_exhaustive]` enum without the thread values (`Text`, `News`, ...) |
//! | [`MessageReferenceType`] | `MessageReferenceType(pub u8)` |
//! | [`ApplicationIntegrationType`] | `ApplicationIntegrationType(pub u8)` with `Default` |
//! | [`InteractionContextType`] | `InteractionContextType(pub u8)` with `Default` |
//! | [`ApplicationCommandHandlerType`] | `ApplicationCommandHandlerType(pub u8)` with `Default` |
//! | [`ActivityType`] | `ActivityType(pub u8)` with `Default` |
//! | [`WebhookPayloadType`] | `webhook_events::WebhookPayloadType(pub u8)` without serde |
//!
//! The swap is a breaking change on purpose: the 2.x versions exposed their
//! storage (`.0`) and some implemented `Default` with a value Discord does
//! not define as a default. Replace `.0` with `get()`, `Type(n)` with
//! `Type::new(n)`, and the closed enum's variants with the constants
//! (`ChannelType::Text` → `ChannelType::GUILD_TEXT`, `News` →
//! `GUILD_ANNOUNCEMENT`, `StageVoice` → `GUILD_STAGE_VOICE`, ...).
//!
//! Classification helpers such as [`ChannelType::is_text_based`] describe
//! what a kind of channel supports. They are not permission checks.

// [Decision Log]
// - 목적과 의도: u8 필드와 닫힌 enum이 섞인 판별값을 하나의 열린 표현으로 통일한다
//   (docs/design/3.0-types-events-requests.md §2).
// - 기존 구현 및 제약 조건: Channel.kind 같은 필드는 맨 u8이고, ChannelType은 닫힌
//   #[non_exhaustive] enum이라 스레드 값(10–12)을 디코딩하지 못하며, InteractionContextType
//   등은 pub u8 필드라 불변식을 둘 수 없다. 시리즈 1은 기존 사용처를 바꾸지 않는다.
// - 검토한 주요 대안: (1) #[non_exhaustive] enum + Unknown(u8) 변형, (2) 타입마다 손으로 쓴
//   newtype, (3) 비공개 선언 매크로로 만든 투명 newtype.
// - 선택한 방식: (3). 저장은 비공개, new(raw)/get(), 연관 상수, 알려진 이름 또는
//   Unknown(n)을 출력하는 Debug, #[serde(transparent)], Default 없음.
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 Unknown(5)와 알려진 5가 서로 다른 값이 될 수
//   있고(왕복 시 정규화 필요), serde 구현을 손으로 써야 한다. (2)는 50여 개 타입에서 같은
//   코드를 반복해 실수가 생긴다. 매크로는 모든 타입이 같은 계약을 지키게 한다.
// - 장점, 단점 및 영향: match에서 상수를 패턴으로 쓸 수 있지만(구조적 동등성) 컴파일러가
//   빠진 경우를 알려 주지 않는다. 열린 집합이라 `_` 갈래가 항상 필요하다. 저장 폭은 문서화된
//   최댓값을 기준으로 u8, 감사 로그 액션만 u16이다. 폭을 넘는 값은 잘리지 않고 디코딩 오류가
//   된다.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Declares an open numeric discriminant. Private on purpose: the generated
/// contract (private storage, `new`/`get`/`name`, `Unknown(n)` Debug,
/// transparent serde, no `Default`) is what callers rely on, so new types
/// must go through this macro instead of hand-written impls.
///
/// Values listed in one invocation must be distinct; `name()` matches on
/// them and a duplicate would make one name unreachable.
macro_rules! open_discriminant {
    (
        $(#[$meta:meta])*
        pub struct $name:ident($repr:ty) {
            $(
                $(#[$const_meta:meta])*
                $constant:ident = $value:literal,
            )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name($repr);

        impl $name {
            $(
                $(#[$const_meta])*
                pub const $constant: Self = Self($value);
            )*

            /// Wraps a raw wire value, including values this version of
            /// discord.rs has no name for.
            pub const fn new(raw: $repr) -> Self {
                Self(raw)
            }

            /// Returns the raw wire value.
            pub const fn get(self) -> $repr {
                self.0
            }

            /// Returns Discord's name for this value (the associated constant's
            /// name), or `None` if this version of discord.rs does not know it.
            pub const fn name(self) -> Option<&'static str> {
                match self.0 {
                    $($value => Some(stringify!($constant)),)*
                    _ => None,
                }
            }
        }

        /// Prints the constant name, or `Unknown(n)` for unnamed values.
        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                match self.name() {
                    Some(name) => f.write_str(name),
                    None => write!(f, "Unknown({})", self.0),
                }
            }
        }
    };
}

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

open_discriminant! {
    /// Channel `type` (Discord docs: Channel Object › Channel Types).
    pub struct ChannelType(u8) {
        /// Text channel within a server.
        GUILD_TEXT = 0,
        /// Direct message between two users.
        DM = 1,
        /// Voice channel within a server.
        GUILD_VOICE = 2,
        /// Direct message between multiple users.
        GROUP_DM = 3,
        /// Category that contains up to 50 channels.
        GUILD_CATEGORY = 4,
        /// Channel users can follow and crosspost into their own server
        /// (formerly "news").
        GUILD_ANNOUNCEMENT = 5,
        /// Thread within a `GUILD_ANNOUNCEMENT` channel.
        ANNOUNCEMENT_THREAD = 10,
        /// Thread within a `GUILD_TEXT`, `GUILD_FORUM`, or `GUILD_MEDIA` channel.
        PUBLIC_THREAD = 11,
        /// Invite-only thread within a `GUILD_TEXT` channel.
        PRIVATE_THREAD = 12,
        /// Voice channel for hosting events with an audience.
        GUILD_STAGE_VOICE = 13,
        /// Channel in a hub listing guilds.
        GUILD_DIRECTORY = 14,
        /// Channel that can only contain threads.
        GUILD_FORUM = 15,
        /// Thread-only channel similar to `GUILD_FORUM`.
        GUILD_MEDIA = 16,
    }
}

impl ChannelType {
    /// Whether this is an announcement, public, or private thread.
    pub const fn is_thread(self) -> bool {
        matches!(
            self,
            Self::ANNOUNCEMENT_THREAD | Self::PUBLIC_THREAD | Self::PRIVATE_THREAD
        )
    }

    /// Whether this is a voice or stage channel.
    pub const fn is_voice_based(self) -> bool {
        matches!(self, Self::GUILD_VOICE | Self::GUILD_STAGE_VOICE)
    }

    /// Whether this kind of channel carries a message timeline (the same set
    /// as discord.js `isTextBased()`): text, DM, group DM, announcement,
    /// threads, and voice/stage channels with text-in-voice.
    ///
    /// This classifies the channel type only. It does not mean the current
    /// user can send there: permissions, archived or locked threads, forum
    /// posting rules, timeouts, and obfuscated channels (whose metadata is a
    /// placeholder) can all still forbid it. Forum and media channels return
    /// `false` because messages live in their threads. Unknown types return
    /// `false`.
    pub const fn is_text_based(self) -> bool {
        matches!(
            self,
            Self::GUILD_TEXT
                | Self::DM
                | Self::GUILD_VOICE
                | Self::GROUP_DM
                | Self::GUILD_ANNOUNCEMENT
                | Self::ANNOUNCEMENT_THREAD
                | Self::PUBLIC_THREAD
                | Self::PRIVATE_THREAD
                | Self::GUILD_STAGE_VOICE
        )
    }
}

open_discriminant! {
    /// Permission overwrite `type` (Discord docs: Overwrite Object). Discord
    /// documents only the numbers; the names are discord.rs's.
    pub struct OverwriteType(u8) {
        /// The overwrite targets a role.
        ROLE = 0,
        /// The overwrite targets a member.
        MEMBER = 1,
    }
}

open_discriminant! {
    /// Voice channel `video_quality_mode` (Discord docs: Video Quality Modes).
    pub struct VideoQualityMode(u8) {
        /// Discord chooses the quality for optimal performance.
        AUTO = 1,
        /// 720p.
        FULL = 2,
    }
}

open_discriminant! {
    /// Forum and media channel `default_sort_order` (Discord docs: Sort Order
    /// Types).
    pub struct ForumSortOrder(u8) {
        /// Sort posts by activity.
        LATEST_ACTIVITY = 0,
        /// Sort posts by creation time, newest first.
        CREATION_DATE = 1,
    }
}

open_discriminant! {
    /// Forum channel `default_forum_layout` (Discord docs: Forum Layout Types).
    pub struct ForumLayout(u8) {
        /// No default has been set.
        NOT_SET = 0,
        /// Display posts as a list.
        LIST_VIEW = 1,
        /// Display posts as a collection of tiles.
        GALLERY_VIEW = 2,
    }
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

open_discriminant! {
    /// Component `type` (Discord docs: Component Object › Component Types).
    pub struct ComponentType(u8) {
        /// Container for other components in a row.
        ACTION_ROW = 1,
        /// Button.
        BUTTON = 2,
        /// Select menu with developer-defined text options.
        STRING_SELECT = 3,
        /// Text input (modals).
        TEXT_INPUT = 4,
        /// Select menu for users.
        USER_SELECT = 5,
        /// Select menu for roles.
        ROLE_SELECT = 6,
        /// Select menu for users and roles.
        MENTIONABLE_SELECT = 7,
        /// Select menu for channels.
        CHANNEL_SELECT = 8,
        /// Text next to an accessory component.
        SECTION = 9,
        /// Markdown text.
        TEXT_DISPLAY = 10,
        /// Small image usable as an accessory.
        THUMBNAIL = 11,
        /// Gallery of images and other media.
        MEDIA_GALLERY = 12,
        /// Attached file.
        FILE = 13,
        /// Vertical padding between components.
        SEPARATOR = 14,
        /// Visual group of components.
        CONTAINER = 17,
        /// Label and description wrapper for a component (modals).
        LABEL = 18,
        /// File upload (modals).
        FILE_UPLOAD = 19,
        /// Single-choice set of options (modals).
        RADIO_GROUP = 21,
        /// Multi-select group of checkboxes (modals).
        CHECKBOX_GROUP = 22,
        /// Single yes/no checkbox (modals).
        CHECKBOX = 23,
    }
}

open_discriminant! {
    /// Button `style` (Discord docs: Button Styles).
    pub struct ButtonStyle(u8) {
        /// Most important or recommended action (`custom_id`).
        PRIMARY = 1,
        /// Alternative or supporting action (`custom_id`).
        SECONDARY = 2,
        /// Positive confirmation or completion (`custom_id`).
        SUCCESS = 3,
        /// Action with irreversible consequences (`custom_id`).
        DANGER = 4,
        /// Navigates to a URL (`url`).
        LINK = 5,
        /// Purchase (`sku_id`).
        PREMIUM = 6,
    }
}

open_discriminant! {
    /// Text input `style` (Discord docs: Text Input Styles).
    pub struct TextInputStyle(u8) {
        /// Single-line input.
        SHORT = 1,
        /// Multi-line input.
        PARAGRAPH = 2,
    }
}

open_discriminant! {
    /// Separator `spacing` (Discord docs: Separator Structure). Discord
    /// documents only the numbers; the names are discord.rs's.
    pub struct SeparatorSpacing(u8) {
        /// Small padding (Discord's default).
        SMALL = 1,
        /// Large padding.
        LARGE = 2,
    }
}

// ---------------------------------------------------------------------------
// Messages and polls
// ---------------------------------------------------------------------------

open_discriminant! {
    /// Message `type` (Discord docs: Message Types).
    pub struct MessageType(u8) {
        /// Regular message.
        DEFAULT = 0,
        /// Recipient added to a group DM.
        RECIPIENT_ADD = 1,
        /// Recipient removed from a group DM.
        RECIPIENT_REMOVE = 2,
        /// Call.
        CALL = 3,
        /// Channel name changed.
        CHANNEL_NAME_CHANGE = 4,
        /// Channel icon changed.
        CHANNEL_ICON_CHANGE = 5,
        /// Message pinned.
        CHANNEL_PINNED_MESSAGE = 6,
        /// Member joined.
        USER_JOIN = 7,
        /// Server boosted.
        GUILD_BOOST = 8,
        /// Server reached boost tier 1.
        GUILD_BOOST_TIER_1 = 9,
        /// Server reached boost tier 2.
        GUILD_BOOST_TIER_2 = 10,
        /// Server reached boost tier 3.
        GUILD_BOOST_TIER_3 = 11,
        /// Channel follow added.
        CHANNEL_FOLLOW_ADD = 12,
        /// Server disqualified from Discovery.
        GUILD_DISCOVERY_DISQUALIFIED = 14,
        /// Server requalified for Discovery.
        GUILD_DISCOVERY_REQUALIFIED = 15,
        /// Discovery grace period initial warning.
        GUILD_DISCOVERY_GRACE_PERIOD_INITIAL_WARNING = 16,
        /// Discovery grace period final warning.
        GUILD_DISCOVERY_GRACE_PERIOD_FINAL_WARNING = 17,
        /// Thread created.
        THREAD_CREATED = 18,
        /// Reply.
        REPLY = 19,
        /// Slash command response.
        CHAT_INPUT_COMMAND = 20,
        /// First message of a thread started from a message.
        THREAD_STARTER_MESSAGE = 21,
        /// Server invite reminder.
        GUILD_INVITE_REMINDER = 22,
        /// Context menu command response.
        CONTEXT_MENU_COMMAND = 23,
        /// Auto moderation action.
        AUTO_MODERATION_ACTION = 24,
        /// Role subscription purchased.
        ROLE_SUBSCRIPTION_PURCHASE = 25,
        /// Premium upsell from an interaction.
        INTERACTION_PREMIUM_UPSELL = 26,
        /// Stage started.
        STAGE_START = 27,
        /// Stage ended.
        STAGE_END = 28,
        /// Stage speaker changed.
        STAGE_SPEAKER = 29,
        /// Stage topic changed.
        STAGE_TOPIC = 31,
        /// Server subscribed to an application's premium tier.
        GUILD_APPLICATION_PREMIUM_SUBSCRIPTION = 32,
        /// Incident alert mode enabled.
        GUILD_INCIDENT_ALERT_MODE_ENABLED = 36,
        /// Incident alert mode disabled.
        GUILD_INCIDENT_ALERT_MODE_DISABLED = 37,
        /// Raid reported.
        GUILD_INCIDENT_REPORT_RAID = 38,
        /// Raid report marked as a false alarm.
        GUILD_INCIDENT_REPORT_FALSE_ALARM = 39,
        /// Purchase notification.
        PURCHASE_NOTIFICATION = 44,
        /// Poll result.
        POLL_RESULT = 46,
    }
}

open_discriminant! {
    /// Message reference `type` (Discord docs: Message Reference Types). When
    /// absent, Discord says `DEFAULT` can be assumed.
    pub struct MessageReferenceType(u8) {
        /// Standard reference used by replies.
        DEFAULT = 0,
        /// Forward: the referenced message is copied as a snapshot.
        FORWARD = 1,
    }
}

open_discriminant! {
    /// Message activity `type` (Discord docs: Message Activity Types).
    pub struct MessageActivityType(u8) {
        /// Join.
        JOIN = 1,
        /// Spectate.
        SPECTATE = 2,
        /// Listen along.
        LISTEN = 3,
        /// Request to join.
        JOIN_REQUEST = 5,
        /// Request to watch a stream.
        STREAM_REQUEST = 6,
    }
}

open_discriminant! {
    /// `type` query parameter of Get Reactions (Discord docs: Reaction Types).
    pub struct ReactionType(u8) {
        /// Normal reaction (Discord's default).
        NORMAL = 0,
        /// Super (burst) reaction.
        BURST = 1,
    }
}

open_discriminant! {
    /// Poll `layout_type` (Discord docs: Layout Type).
    pub struct PollLayoutType(u8) {
        /// The default layout; currently the only one.
        DEFAULT = 1,
    }
}

open_discriminant! {
    /// Shared client theme `base_theme` (Discord docs: Message › Base Theme
    /// Types). Added in series 3; not on the design's series 1 list.
    pub struct BaseTheme(u8) {
        /// No base theme; Discord treats it like `DARK`.
        UNSET = 0,
        /// Dark.
        DARK = 1,
        /// Light.
        LIGHT = 2,
        /// Darker.
        DARKER = 3,
        /// Midnight.
        MIDNIGHT = 4,
    }
}

// ---------------------------------------------------------------------------
// Interactions and application commands
// ---------------------------------------------------------------------------

open_discriminant! {
    /// Interaction `type` (Discord docs: Interaction Type).
    pub struct InteractionType(u8) {
        /// Endpoint verification ping.
        PING = 1,
        /// Application command invocation.
        APPLICATION_COMMAND = 2,
        /// Message component interaction.
        MESSAGE_COMPONENT = 3,
        /// Autocomplete request.
        APPLICATION_COMMAND_AUTOCOMPLETE = 4,
        /// Modal submission.
        MODAL_SUBMIT = 5,
    }
}

open_discriminant! {
    /// Interaction response `type` (Discord docs: Interaction Callback Type).
    pub struct InteractionResponseType(u8) {
        /// Acknowledge a ping.
        PONG = 1,
        /// Respond with a message.
        CHANNEL_MESSAGE_WITH_SOURCE = 4,
        /// Acknowledge and edit a response later; the user sees a loading state.
        DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE = 5,
        /// Components and modal submits: acknowledge and edit the original
        /// message later, without a loading state.
        DEFERRED_UPDATE_MESSAGE = 6,
        /// Components and modal submits: edit the message the component is on.
        UPDATE_MESSAGE = 7,
        /// Respond to autocomplete with choices.
        APPLICATION_COMMAND_AUTOCOMPLETE_RESULT = 8,
        /// Respond with a popup modal.
        MODAL = 9,
        /// Deprecated by Discord in favor of premium buttons; kept so old
        /// payloads still name it.
        PREMIUM_REQUIRED = 10,
        /// Launch the application's Activity.
        LAUNCH_ACTIVITY = 12,
    }
}

open_discriminant! {
    /// Application command `type` (Discord docs: Application Command Types).
    pub struct CommandType(u8) {
        /// Slash command.
        CHAT_INPUT = 1,
        /// User context menu command.
        USER = 2,
        /// Message context menu command.
        MESSAGE = 3,
        /// Primary entry point that launches the application's Activity.
        PRIMARY_ENTRY_POINT = 4,
    }
}

open_discriminant! {
    /// Application command option `type` (Discord docs: Application Command
    /// Option Type).
    pub struct CommandOptionType(u8) {
        /// Subcommand.
        SUB_COMMAND = 1,
        /// Subcommand group.
        SUB_COMMAND_GROUP = 2,
        /// String.
        STRING = 3,
        /// Integer between -2^53+1 and 2^53-1.
        INTEGER = 4,
        /// Boolean.
        BOOLEAN = 5,
        /// User.
        USER = 6,
        /// Channel, including categories.
        CHANNEL = 7,
        /// Role.
        ROLE = 8,
        /// User or role.
        MENTIONABLE = 9,
        /// Double between -2^53 and 2^53.
        NUMBER = 10,
        /// Attachment.
        ATTACHMENT = 11,
    }
}

open_discriminant! {
    /// Application command permission `type` (Discord docs: Application
    /// Command Permission Type).
    pub struct CommandPermissionType(u8) {
        /// Role.
        ROLE = 1,
        /// User.
        USER = 2,
        /// Channel.
        CHANNEL = 3,
    }
}

open_discriminant! {
    /// Where an application can be installed (Discord docs: Application
    /// Integration Types).
    pub struct ApplicationIntegrationType(u8) {
        /// Installable to servers.
        GUILD_INSTALL = 0,
        /// Installable to users.
        USER_INSTALL = 1,
    }
}

open_discriminant! {
    /// Where an interaction can be used (Discord docs: Interaction Context
    /// Types).
    pub struct InteractionContextType(u8) {
        /// Within servers.
        GUILD = 0,
        /// In DMs with the application's bot user.
        BOT_DM = 1,
        /// In group DMs and DMs other than the bot user's.
        PRIVATE_CHANNEL = 2,
    }
}

open_discriminant! {
    /// Primary entry point command `handler` (Discord docs: Entry Point
    /// Command Handler Types).
    pub struct ApplicationCommandHandlerType(u8) {
        /// The application handles the interaction using its token.
        APP_HANDLER = 1,
        /// Discord launches the Activity and sends a follow-up message.
        DISCORD_LAUNCH_ACTIVITY = 2,
    }
}

open_discriminant! {
    /// Application role connection metadata `type` (Discord docs: Application
    /// Role Connection Metadata Type).
    pub struct RoleConnectionMetadataType(u8) {
        /// User's integer is less than or equal to the guild's value.
        INTEGER_LESS_THAN_OR_EQUAL = 1,
        /// User's integer is greater than or equal to the guild's value.
        INTEGER_GREATER_THAN_OR_EQUAL = 2,
        /// User's integer equals the guild's value.
        INTEGER_EQUAL = 3,
        /// User's integer does not equal the guild's value.
        INTEGER_NOT_EQUAL = 4,
        /// User's date is at most the guild's number of days before now.
        DATETIME_LESS_THAN_OR_EQUAL = 5,
        /// User's date is at least the guild's number of days before now.
        DATETIME_GREATER_THAN_OR_EQUAL = 6,
        /// User's boolean equals the guild's value.
        BOOLEAN_EQUAL = 7,
        /// User's boolean does not equal the guild's value.
        BOOLEAN_NOT_EQUAL = 8,
    }
}

open_discriminant! {
    /// Application `event_webhooks_status` (Discord docs: Application Event
    /// Webhook Status). Added in series 3; not on the design's series 1
    /// list.
    pub struct ApplicationEventWebhookStatus(u8) {
        /// Webhook events are disabled by the developer.
        DISABLED = 1,
        /// Webhook events are enabled by the developer.
        ENABLED = 2,
        /// Webhook events are disabled by Discord, usually after inactivity.
        DISABLED_BY_DISCORD = 3,
    }
}

// ---------------------------------------------------------------------------
// Guilds
// ---------------------------------------------------------------------------

open_discriminant! {
    /// Guild `verification_level` (Discord docs: Verification Level).
    pub struct VerificationLevel(u8) {
        /// Unrestricted.
        NONE = 0,
        /// Must have a verified email.
        LOW = 1,
        /// Registered on Discord for longer than 5 minutes.
        MEDIUM = 2,
        /// Member of the server for longer than 10 minutes.
        HIGH = 3,
        /// Must have a verified phone number.
        VERY_HIGH = 4,
    }
}

open_discriminant! {
    /// Guild `default_message_notifications` (Discord docs: Default Message
    /// Notification Level).
    pub struct DefaultMessageNotificationLevel(u8) {
        /// Notify for all messages by default.
        ALL_MESSAGES = 0,
        /// Notify only for mentions by default.
        ONLY_MENTIONS = 1,
    }
}

open_discriminant! {
    /// Guild `explicit_content_filter` (Discord docs: Explicit Content Filter
    /// Level).
    pub struct ExplicitContentFilterLevel(u8) {
        /// Media is not scanned.
        DISABLED = 0,
        /// Scan media from members without roles.
        MEMBERS_WITHOUT_ROLES = 1,
        /// Scan media from all members.
        ALL_MEMBERS = 2,
    }
}

open_discriminant! {
    /// Guild `mfa_level` (Discord docs: MFA Level).
    pub struct MfaLevel(u8) {
        /// No MFA requirement for moderation actions.
        NONE = 0,
        /// 2FA required for moderation actions.
        ELEVATED = 1,
    }
}

open_discriminant! {
    /// Guild `premium_tier` (Discord docs: Premium Tier).
    pub struct PremiumTier(u8) {
        /// No Server Boost perks.
        NONE = 0,
        /// Server Boost level 1 perks.
        TIER_1 = 1,
        /// Server Boost level 2 perks.
        TIER_2 = 2,
        /// Server Boost level 3 perks.
        TIER_3 = 3,
    }
}

open_discriminant! {
    /// Guild `nsfw_level` (Discord docs: Guild Age-Restriction Level, formerly
    /// Guild NSFW Level).
    pub struct GuildNsfwLevel(u8) {
        /// Default.
        DEFAULT = 0,
        /// Explicit.
        EXPLICIT = 1,
        /// Safe.
        SAFE = 2,
        /// Age-restricted.
        AGE_RESTRICTED = 3,
    }
}

open_discriminant! {
    /// Guild onboarding `mode` (Discord docs: Onboarding Mode). Added in
    /// series 3; not on the design's series 1 list.
    pub struct OnboardingMode(u8) {
        /// Only default channels count towards the constraints.
        ONBOARDING_DEFAULT = 0,
        /// Default channels and questions count towards the constraints.
        ONBOARDING_ADVANCED = 1,
    }
}

open_discriminant! {
    /// Integration `expire_behavior` (Discord docs: Integration Expire
    /// Behaviors). Discord gives the names as prose ("Remove role",
    /// "Kick"); the constant spelling is discord.rs's. Added in series 3;
    /// not on the design's series 1 list.
    pub struct IntegrationExpireBehavior(u8) {
        /// Remove the subscriber role.
        REMOVE_ROLE = 0,
        /// Kick the member.
        KICK = 1,
    }
}

// ---------------------------------------------------------------------------
// Scheduled events and stage instances
// ---------------------------------------------------------------------------

open_discriminant! {
    /// Guild scheduled event `status` (Discord docs: Guild Scheduled Event
    /// Status). `COMPLETED` and `CANCELED` are terminal.
    pub struct ScheduledEventStatus(u8) {
        /// Scheduled and not started.
        SCHEDULED = 1,
        /// Currently active.
        ACTIVE = 2,
        /// Finished.
        COMPLETED = 3,
        /// Canceled.
        CANCELED = 4,
    }
}

open_discriminant! {
    /// Guild scheduled event `entity_type` (Discord docs: Guild Scheduled
    /// Event Entity Types).
    pub struct ScheduledEventEntityType(u8) {
        /// Event in a stage channel.
        STAGE_INSTANCE = 1,
        /// Event in a voice channel.
        VOICE = 2,
        /// Event at an external location.
        EXTERNAL = 3,
    }
}

open_discriminant! {
    /// Guild scheduled event `privacy_level` (Discord docs: Guild Scheduled
    /// Event Privacy Level). Separate from [`StageInstancePrivacyLevel`]
    /// because the two documented sets differ.
    pub struct ScheduledEventPrivacyLevel(u8) {
        /// Only accessible to guild members.
        GUILD_ONLY = 2,
    }
}

open_discriminant! {
    /// Recurrence rule `frequency` (Discord docs: Guild Scheduled Event
    /// Recurrence Rule - Frequency). Added in series 3; not on the design's
    /// series 1 list.
    pub struct RecurrenceRuleFrequency(u8) {
        /// Every year.
        YEARLY = 0,
        /// Every month.
        MONTHLY = 1,
        /// Every week.
        WEEKLY = 2,
        /// Every day.
        DAILY = 3,
    }
}

open_discriminant! {
    /// Recurrence rule weekday (`by_weekday`, `by_n_weekday[].day`; Discord
    /// docs: Guild Scheduled Event Recurrence Rule - Weekday). Monday is
    /// `0`. Added in series 3; not on the design's series 1 list.
    pub struct RecurrenceRuleWeekday(u8) {
        /// Monday.
        MONDAY = 0,
        /// Tuesday.
        TUESDAY = 1,
        /// Wednesday.
        WEDNESDAY = 2,
        /// Thursday.
        THURSDAY = 3,
        /// Friday.
        FRIDAY = 4,
        /// Saturday.
        SATURDAY = 5,
        /// Sunday.
        SUNDAY = 6,
    }
}

open_discriminant! {
    /// Recurrence rule month (`by_month`; Discord docs: Guild Scheduled
    /// Event Recurrence Rule - Month). January is `1`. Added in series 3;
    /// not on the design's series 1 list.
    pub struct RecurrenceRuleMonth(u8) {
        /// January.
        JANUARY = 1,
        /// February.
        FEBRUARY = 2,
        /// March.
        MARCH = 3,
        /// April.
        APRIL = 4,
        /// May.
        MAY = 5,
        /// June.
        JUNE = 6,
        /// July.
        JULY = 7,
        /// August.
        AUGUST = 8,
        /// September.
        SEPTEMBER = 9,
        /// October.
        OCTOBER = 10,
        /// November.
        NOVEMBER = 11,
        /// December.
        DECEMBER = 12,
    }
}

open_discriminant! {
    /// Stage instance `privacy_level` (Discord docs: Stage Instance Privacy
    /// Level).
    pub struct StageInstancePrivacyLevel(u8) {
        /// Visible publicly. Deprecated by Discord; kept so old payloads
        /// still name it.
        PUBLIC = 1,
        /// Only visible to guild members.
        GUILD_ONLY = 2,
    }
}

// ---------------------------------------------------------------------------
// Moderation
// ---------------------------------------------------------------------------

open_discriminant! {
    /// Auto moderation rule `event_type` (Discord docs: Event Types).
    pub struct AutoModerationEventType(u8) {
        /// A member sends or edits a message.
        MESSAGE_SEND = 1,
        /// A member edits their profile.
        MEMBER_UPDATE = 2,
    }
}

open_discriminant! {
    /// Auto moderation rule `trigger_type` (Discord docs: Trigger Types).
    pub struct AutoModerationTriggerType(u8) {
        /// Content contains user-defined keywords.
        KEYWORD = 1,
        /// Content looks like generic spam.
        SPAM = 3,
        /// Content contains words from Discord's predefined lists.
        KEYWORD_PRESET = 4,
        /// Content has more unique mentions than allowed.
        MENTION_SPAM = 5,
        /// Member profile contains user-defined keywords.
        MEMBER_PROFILE = 6,
    }
}

open_discriminant! {
    /// Auto moderation action `type` (Discord docs: Action Types).
    pub struct AutoModerationActionType(u8) {
        /// Block the message, optionally with a custom explanation.
        BLOCK_MESSAGE = 1,
        /// Log the content to a channel.
        SEND_ALERT_MESSAGE = 2,
        /// Time out the member.
        TIMEOUT = 3,
        /// Prevent the member from interacting in the guild.
        BLOCK_MEMBER_INTERACTION = 4,
    }
}

open_discriminant! {
    /// Auto moderation keyword preset (Discord docs: Keyword Preset Types).
    pub struct KeywordPresetType(u8) {
        /// Swearing or cursing.
        PROFANITY = 1,
        /// Sexually explicit content.
        SEXUAL_CONTENT = 2,
        /// Personal insults or hate speech.
        SLURS = 3,
    }
}

open_discriminant! {
    /// Audit log entry `action_type` (Discord docs: Audit Log Events).
    ///
    /// Stored as `u16`: documented values already reach 193 and grow in
    /// blocks of ten per feature, so `u8` would leave little headroom.
    pub struct AuditLogActionType(u16) {
        /// Server settings updated.
        GUILD_UPDATE = 1,
        /// Channel created.
        CHANNEL_CREATE = 10,
        /// Channel settings updated.
        CHANNEL_UPDATE = 11,
        /// Channel deleted.
        CHANNEL_DELETE = 12,
        /// Permission overwrite added.
        CHANNEL_OVERWRITE_CREATE = 13,
        /// Permission overwrite updated.
        CHANNEL_OVERWRITE_UPDATE = 14,
        /// Permission overwrite deleted.
        CHANNEL_OVERWRITE_DELETE = 15,
        /// Member removed from the server.
        MEMBER_KICK = 20,
        /// Members pruned.
        MEMBER_PRUNE = 21,
        /// Member banned.
        MEMBER_BAN_ADD = 22,
        /// Ban lifted.
        MEMBER_BAN_REMOVE = 23,
        /// Member updated.
        MEMBER_UPDATE = 24,
        /// Member added to or removed from a role.
        MEMBER_ROLE_UPDATE = 25,
        /// Member moved to another voice channel.
        MEMBER_MOVE = 26,
        /// Member disconnected from voice.
        MEMBER_DISCONNECT = 27,
        /// Bot added to the server.
        BOT_ADD = 28,
        /// Role created.
        ROLE_CREATE = 30,
        /// Role updated.
        ROLE_UPDATE = 31,
        /// Role deleted.
        ROLE_DELETE = 32,
        /// Invite created.
        INVITE_CREATE = 40,
        /// Invite updated.
        INVITE_UPDATE = 41,
        /// Invite deleted.
        INVITE_DELETE = 42,
        /// Webhook created.
        WEBHOOK_CREATE = 50,
        /// Webhook updated.
        WEBHOOK_UPDATE = 51,
        /// Webhook deleted.
        WEBHOOK_DELETE = 52,
        /// Emoji created.
        EMOJI_CREATE = 60,
        /// Emoji renamed.
        EMOJI_UPDATE = 61,
        /// Emoji deleted.
        EMOJI_DELETE = 62,
        /// Single message deleted.
        MESSAGE_DELETE = 72,
        /// Multiple messages deleted.
        MESSAGE_BULK_DELETE = 73,
        /// Message pinned.
        MESSAGE_PIN = 74,
        /// Message unpinned.
        MESSAGE_UNPIN = 75,
        /// Application added to the server.
        INTEGRATION_CREATE = 80,
        /// Application updated.
        INTEGRATION_UPDATE = 81,
        /// Application removed from the server.
        INTEGRATION_DELETE = 82,
        /// Stage instance created.
        STAGE_INSTANCE_CREATE = 83,
        /// Stage instance updated.
        STAGE_INSTANCE_UPDATE = 84,
        /// Stage instance deleted.
        STAGE_INSTANCE_DELETE = 85,
        /// Sticker created.
        STICKER_CREATE = 90,
        /// Sticker updated.
        STICKER_UPDATE = 91,
        /// Sticker deleted.
        STICKER_DELETE = 92,
        /// Scheduled event created.
        GUILD_SCHEDULED_EVENT_CREATE = 100,
        /// Scheduled event updated.
        GUILD_SCHEDULED_EVENT_UPDATE = 101,
        /// Scheduled event cancelled.
        GUILD_SCHEDULED_EVENT_DELETE = 102,
        /// Thread created.
        THREAD_CREATE = 110,
        /// Thread updated.
        THREAD_UPDATE = 111,
        /// Thread deleted.
        THREAD_DELETE = 112,
        /// Application command permissions updated.
        APPLICATION_COMMAND_PERMISSION_UPDATE = 121,
        /// Soundboard sound created.
        SOUNDBOARD_SOUND_CREATE = 130,
        /// Soundboard sound updated.
        SOUNDBOARD_SOUND_UPDATE = 131,
        /// Soundboard sound deleted.
        SOUNDBOARD_SOUND_DELETE = 132,
        /// Auto moderation rule created.
        AUTO_MODERATION_RULE_CREATE = 140,
        /// Auto moderation rule updated.
        AUTO_MODERATION_RULE_UPDATE = 141,
        /// Auto moderation rule deleted.
        AUTO_MODERATION_RULE_DELETE = 142,
        /// Message blocked by auto moderation.
        AUTO_MODERATION_BLOCK_MESSAGE = 143,
        /// Message flagged by auto moderation.
        AUTO_MODERATION_FLAG_TO_CHANNEL = 144,
        /// Member timed out by auto moderation.
        AUTO_MODERATION_USER_COMMUNICATION_DISABLED = 145,
        /// Member quarantined by auto moderation.
        AUTO_MODERATION_QUARANTINE_USER = 146,
        /// Creator monetization request created.
        CREATOR_MONETIZATION_REQUEST_CREATED = 150,
        /// Creator monetization terms accepted.
        CREATOR_MONETIZATION_TERMS_ACCEPTED = 151,
        /// Onboarding question created.
        ONBOARDING_PROMPT_CREATE = 163,
        /// Onboarding question updated.
        ONBOARDING_PROMPT_UPDATE = 164,
        /// Onboarding question deleted.
        ONBOARDING_PROMPT_DELETE = 165,
        /// Guild onboarding created.
        ONBOARDING_CREATE = 166,
        /// Guild onboarding updated.
        ONBOARDING_UPDATE = 167,
        /// Server guide created.
        HOME_SETTINGS_CREATE = 190,
        /// Server guide updated.
        HOME_SETTINGS_UPDATE = 191,
        /// Voice channel status set.
        VOICE_CHANNEL_STATUS_CREATE = 192,
        /// Voice channel status deleted.
        VOICE_CHANNEL_STATUS_DELETE = 193,
    }
}

// ---------------------------------------------------------------------------
// Invites, webhooks, stickers
// ---------------------------------------------------------------------------

open_discriminant! {
    /// Invite `type` (Discord docs: Invite Types).
    pub struct InviteType(u8) {
        /// Guild invite.
        GUILD = 0,
        /// Group DM invite.
        GROUP_DM = 1,
        /// Friend invite.
        FRIEND = 2,
    }
}

open_discriminant! {
    /// Invite `target_type` (Discord docs: Invite Target Types).
    pub struct InviteTargetType(u8) {
        /// A user's stream in a voice channel.
        STREAM = 1,
        /// An embedded application (Activity).
        EMBEDDED_APPLICATION = 2,
    }
}

open_discriminant! {
    /// Status of an invite target-users job (Discord docs: Get Target Users
    /// Job Status › Status Codes).
    pub struct InviteTargetUsersJobStatusType(u8) {
        /// Default value.
        UNSPECIFIED = 0,
        /// Still processing.
        PROCESSING = 1,
        /// Completed successfully.
        COMPLETED = 2,
        /// Failed; see `error_message`.
        FAILED = 3,
    }
}

open_discriminant! {
    /// Webhook `type` (Discord docs: Webhook Types).
    pub struct WebhookType(u8) {
        /// Posts messages using its token.
        INCOMING = 1,
        /// Internal webhook that posts followed-channel messages.
        CHANNEL_FOLLOWER = 2,
        /// Webhook used with interactions.
        APPLICATION = 3,
    }
}

open_discriminant! {
    /// Webhook Events payload `type` (Discord docs: Webhook Event Payloads ›
    /// Webhook Types).
    pub struct WebhookPayloadType(u8) {
        /// Verifies that the Webhook Events URL is active.
        PING = 0,
        /// Carries an event body.
        EVENT = 1,
    }
}

open_discriminant! {
    /// Sticker `type` (Discord docs: Sticker Types).
    pub struct StickerType(u8) {
        /// Official sticker in a pack.
        STANDARD = 1,
        /// Sticker uploaded to a guild.
        GUILD = 2,
    }
}

open_discriminant! {
    /// Sticker `format_type` (Discord docs: Sticker Format Types).
    pub struct StickerFormatType(u8) {
        /// PNG.
        PNG = 1,
        /// Animated PNG.
        APNG = 2,
        /// Lottie animation.
        LOTTIE = 3,
        /// GIF.
        GIF = 4,
    }
}

// ---------------------------------------------------------------------------
// Monetization
// ---------------------------------------------------------------------------

open_discriminant! {
    /// SKU `type` (Discord docs: SKU Types).
    pub struct SkuType(u8) {
        /// Durable one-time purchase.
        DURABLE = 2,
        /// Consumable one-time purchase.
        CONSUMABLE = 3,
        /// Recurring subscription.
        SUBSCRIPTION = 5,
        /// System-generated group for a subscription SKU.
        SUBSCRIPTION_GROUP = 6,
    }
}

open_discriminant! {
    /// SKU `access_type`.
    ///
    /// Discord sends this field (its SKU examples show `1`) but does not
    /// document the values, so no names are defined; every value prints as
    /// `Unknown(n)` and round-trips unchanged.
    pub struct SkuAccessType(u8) {}
}

open_discriminant! {
    /// Entitlement `type` (Discord docs: Entitlement Types).
    pub struct EntitlementType(u8) {
        /// Purchased by the user.
        PURCHASE = 1,
        /// Discord Nitro subscription.
        PREMIUM_SUBSCRIPTION = 2,
        /// Gifted by the developer.
        DEVELOPER_GIFT = 3,
        /// Purchased by a developer in test mode.
        TEST_MODE_PURCHASE = 4,
        /// Granted when the SKU was free.
        FREE_PURCHASE = 5,
        /// Gifted by another user.
        USER_GIFT = 6,
        /// Claimed for free by a Nitro subscriber.
        PREMIUM_PURCHASE = 7,
        /// Purchased as an application subscription.
        APPLICATION_SUBSCRIPTION = 8,
    }
}

open_discriminant! {
    /// `owner_type` of Create Test Entitlement. Discord documents only the
    /// numbers; the names are discord.rs's.
    pub struct EntitlementOwnerType(u8) {
        /// Guild subscription.
        GUILD = 1,
        /// User subscription.
        USER = 2,
    }
}

open_discriminant! {
    /// Subscription `status` (Discord docs: Subscription Statuses).
    pub struct SubscriptionStatus(u8) {
        /// Active and scheduled to renew.
        ACTIVE = 0,
        /// Inactive and not being charged.
        INACTIVE = 1,
        /// Active but will not renew.
        ENDING = 2,
    }
}

// ---------------------------------------------------------------------------
// Voice
// ---------------------------------------------------------------------------

open_discriminant! {
    /// Voice channel effect `animation_type` (Discord docs: Gateway Events ›
    /// Voice Channel Effect Send › Animation Types). Added in series 3; not
    /// on the design's series 1 list.
    pub struct VoiceChannelEffectAnimationType(u8) {
        /// A fun animation, sent by a Nitro subscriber.
        PREMIUM = 0,
        /// The standard animation.
        BASIC = 1,
    }
}

// ---------------------------------------------------------------------------
// Users and presence
// ---------------------------------------------------------------------------

open_discriminant! {
    /// User `premium_type` (Discord docs: Premium Types).
    pub struct UserPremiumType(u8) {
        /// No Nitro.
        NONE = 0,
        /// Nitro Classic.
        NITRO_CLASSIC = 1,
        /// Nitro.
        NITRO = 2,
        /// Nitro Basic.
        NITRO_BASIC = 3,
    }
}

open_discriminant! {
    /// Connection `visibility` (Discord docs: Visibility Types).
    pub struct ConnectionVisibility(u8) {
        /// Invisible to everyone except the user.
        NONE = 0,
        /// Visible to everyone.
        EVERYONE = 1,
    }
}

open_discriminant! {
    /// Presence activity `type` (Discord docs: Activity Types).
    pub struct ActivityType(u8) {
        /// "Playing {name}".
        PLAYING = 0,
        /// "Streaming {details}" (Twitch and YouTube URLs only).
        STREAMING = 1,
        /// "Listening to {name}".
        LISTENING = 2,
        /// "Watching {name}".
        WATCHING = 3,
        /// "{emoji} {state}".
        CUSTOM = 4,
        /// "Competing in {name}".
        COMPETING = 5,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        ActivityType, AuditLogActionType, ButtonStyle, ChannelType, ComponentType,
        InteractionContextType, MessageType, SkuAccessType, StageInstancePrivacyLevel,
        WebhookPayloadType,
    };

    /// Checks the macro contract for every raw value of every type: serde
    /// round-trips the number unchanged and `Debug` agrees with `name()`.
    macro_rules! assert_every_value_round_trips {
        ($($name:ident: $repr:ty),+ $(,)?) => {$(
            let mut named = 0_usize;
            for raw in <$repr>::MIN..=<$repr>::MAX {
                let value = super::$name::new(raw);
                assert_eq!(value.get(), raw);
                let wire = match serde_json::to_value(value) {
                    Ok(wire) => wire,
                    Err(error) => panic!("{} {raw} must serialize: {error}", stringify!($name)),
                };
                assert_eq!(wire, json!(raw), "{} {raw}", stringify!($name));
                assert_eq!(
                    serde_json::from_value::<super::$name>(wire).ok(),
                    Some(value),
                    "{} {raw}",
                    stringify!($name)
                );
                let debug = format!("{value:?}");
                match value.name() {
                    Some(name) => {
                        named += 1;
                        assert_eq!(debug, name);
                    }
                    None => assert_eq!(debug, format!("Unknown({raw})")),
                }
            }
            assert!(
                named > 0 || stringify!($name) == "SkuAccessType",
                "{} names no values",
                stringify!($name)
            );
        )+};
    }

    #[test]
    fn every_value_of_every_discriminant_round_trips() {
        assert_every_value_round_trips! {
            ChannelType: u8, OverwriteType: u8, VideoQualityMode: u8, ForumSortOrder: u8,
            ForumLayout: u8, ComponentType: u8, ButtonStyle: u8, TextInputStyle: u8,
            SeparatorSpacing: u8, MessageType: u8, MessageReferenceType: u8,
            MessageActivityType: u8, ReactionType: u8, PollLayoutType: u8,
            InteractionType: u8, InteractionResponseType: u8, CommandType: u8,
            CommandOptionType: u8, CommandPermissionType: u8, ApplicationIntegrationType: u8,
            InteractionContextType: u8, ApplicationCommandHandlerType: u8,
            RoleConnectionMetadataType: u8, VerificationLevel: u8,
            DefaultMessageNotificationLevel: u8, ExplicitContentFilterLevel: u8, MfaLevel: u8,
            PremiumTier: u8, GuildNsfwLevel: u8, ScheduledEventStatus: u8,
            ScheduledEventEntityType: u8, ScheduledEventPrivacyLevel: u8,
            StageInstancePrivacyLevel: u8, AutoModerationEventType: u8,
            AutoModerationTriggerType: u8, AutoModerationActionType: u8, KeywordPresetType: u8,
            AuditLogActionType: u16, InviteType: u8, InviteTargetType: u8,
            InviteTargetUsersJobStatusType: u8, WebhookType: u8, WebhookPayloadType: u8,
            StickerType: u8, StickerFormatType: u8, SkuType: u8, SkuAccessType: u8,
            EntitlementType: u8, EntitlementOwnerType: u8, SubscriptionStatus: u8,
            UserPremiumType: u8, ConnectionVisibility: u8, ActivityType: u8,
            BaseTheme: u8, ApplicationEventWebhookStatus: u8, OnboardingMode: u8,
            IntegrationExpireBehavior: u8, RecurrenceRuleFrequency: u8,
            RecurrenceRuleWeekday: u8, RecurrenceRuleMonth: u8,
            VoiceChannelEffectAnimationType: u8,
        }
    }

    #[test]
    fn known_values_serialize_as_integers_and_print_their_names() {
        assert_eq!(
            serde_json::to_value(ChannelType::GUILD_FORUM).ok(),
            Some(json!(15))
        );
        assert_eq!(
            serde_json::from_value::<ChannelType>(json!(11)).ok(),
            Some(ChannelType::PUBLIC_THREAD)
        );
        assert_eq!(format!("{:?}", ChannelType::PUBLIC_THREAD), "PUBLIC_THREAD");
        assert_eq!(ButtonStyle::PREMIUM.name(), Some("PREMIUM"));
        assert_eq!(ComponentType::CHECKBOX.get(), 23);
        assert_eq!(ComponentType::new(23), ComponentType::CHECKBOX);
        assert_eq!(
            format!("{:?}", Some(InteractionContextType::BOT_DM)),
            "Some(BOT_DM)"
        );
    }

    #[test]
    fn unknown_values_round_trip_unchanged() {
        let unknown: ChannelType = match serde_json::from_value(json!(99)) {
            Ok(kind) => kind,
            Err(error) => panic!("unknown channel types must decode: {error}"),
        };
        assert_eq!(unknown.get(), 99);
        assert_eq!(unknown.name(), None);
        assert_eq!(format!("{unknown:?}"), "Unknown(99)");
        assert_eq!(serde_json::to_value(unknown).ok(), Some(json!(99)));

        // Gaps inside a documented range are unknown too, not a neighbor.
        assert_eq!(format!("{:?}", MessageType::new(13)), "Unknown(13)");
        assert_eq!(format!("{:?}", ComponentType::new(20)), "Unknown(20)");

        // The widest storage keeps values past u8.
        let action: AuditLogActionType = match serde_json::from_value(json!(1000)) {
            Ok(action) => action,
            Err(error) => panic!("u16 audit log actions must decode: {error}"),
        };
        assert_eq!(serde_json::to_value(action).ok(), Some(json!(1000)));
        assert_eq!(AuditLogActionType::VOICE_CHANNEL_STATUS_DELETE.get(), 193);

        // A type with no documented names still round-trips.
        assert_eq!(format!("{:?}", SkuAccessType::new(1)), "Unknown(1)");
        assert_eq!(
            serde_json::from_value::<SkuAccessType>(json!(1)).ok(),
            Some(SkuAccessType::new(1))
        );
    }

    #[test]
    fn out_of_range_values_fail_instead_of_truncating() {
        assert!(serde_json::from_value::<ChannelType>(json!(256)).is_err());
        assert!(serde_json::from_value::<ChannelType>(json!(-1)).is_err());
        assert!(serde_json::from_value::<ChannelType>(json!("0")).is_err());
        assert!(serde_json::from_value::<AuditLogActionType>(json!(65_536)).is_err());
    }

    #[test]
    fn channel_type_classification() {
        let threads = [
            ChannelType::ANNOUNCEMENT_THREAD,
            ChannelType::PUBLIC_THREAD,
            ChannelType::PRIVATE_THREAD,
        ];
        for kind in threads {
            assert!(kind.is_thread(), "{kind:?}");
            assert!(kind.is_text_based(), "{kind:?}");
            assert!(!kind.is_voice_based(), "{kind:?}");
        }
        for kind in [ChannelType::GUILD_VOICE, ChannelType::GUILD_STAGE_VOICE] {
            assert!(kind.is_voice_based(), "{kind:?}");
            assert!(kind.is_text_based(), "text-in-voice: {kind:?}");
            assert!(!kind.is_thread(), "{kind:?}");
        }
        for kind in [
            ChannelType::GUILD_TEXT,
            ChannelType::DM,
            ChannelType::GROUP_DM,
            ChannelType::GUILD_ANNOUNCEMENT,
        ] {
            assert!(kind.is_text_based(), "{kind:?}");
            assert!(!kind.is_thread() && !kind.is_voice_based(), "{kind:?}");
        }
        for kind in [
            ChannelType::GUILD_CATEGORY,
            ChannelType::GUILD_DIRECTORY,
            ChannelType::GUILD_FORUM,
            ChannelType::GUILD_MEDIA,
            ChannelType::new(99),
        ] {
            assert!(
                !kind.is_text_based() && !kind.is_thread() && !kind.is_voice_based(),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn swapped_types_keep_the_2x_wire_values() {
        // Series 3 replaced the 2.x definitions of these names; the numbers
        // 2.x code sent (its enum discriminants and `Type(n)` constants) must
        // be what the new constants send.
        let pairs: [(u8, u8); 10] = [
            (ChannelType::GUILD_TEXT.get(), 0), // 2.x ChannelType::Text
            (ChannelType::GUILD_ANNOUNCEMENT.get(), 5), // 2.x ChannelType::News
            (ChannelType::GUILD_STAGE_VOICE.get(), 13), // 2.x ChannelType::StageVoice
            (ChannelType::GUILD_MEDIA.get(), 16), // 2.x ChannelType::GuildMedia
            (InteractionContextType::PRIVATE_CHANNEL.get(), 2),
            (ActivityType::COMPETING.get(), 5),
            (WebhookPayloadType::EVENT.get(), 1),
            (super::MessageReferenceType::FORWARD.get(), 1),
            (super::ApplicationIntegrationType::USER_INSTALL.get(), 1),
            (
                super::ApplicationCommandHandlerType::DISCORD_LAUNCH_ACTIVITY.get(),
                2,
            ),
        ];
        for (new, old) in pairs {
            assert_eq!(new, old);
        }
    }

    #[test]
    fn deprecated_values_keep_their_names() {
        assert_eq!(StageInstancePrivacyLevel::PUBLIC.name(), Some("PUBLIC"));
        assert_eq!(super::InteractionResponseType::PREMIUM_REQUIRED.get(), 10);
    }

    #[test]
    fn constants_work_as_match_patterns() {
        fn describe(kind: ChannelType) -> &'static str {
            match kind {
                ChannelType::GUILD_TEXT => "text",
                ChannelType::GUILD_VOICE => "voice",
                _ => "other",
            }
        }
        assert_eq!(describe(ChannelType::GUILD_TEXT), "text");
        assert_eq!(describe(ChannelType::GUILD_VOICE), "voice");
        assert_eq!(describe(ChannelType::new(200)), "other");
    }
}
