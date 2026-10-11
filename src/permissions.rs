//! Channel and guild permission computation: the effective permissions
//! behind discord.js `channel.permissionsFor(member)` / `member.permissions`.
//!
//! The calculation follows Discord's documented algorithm
//! (<https://docs.discord.com/developers/topics/permissions>):
//!
//! 1. The guild owner has every permission.
//! 2. Base permissions are `@everyone` OR every role the member has; a base
//!    containing `ADMINISTRATOR` grants every permission and bypasses
//!    overwrites.
//! 3. Channel overwrites apply in a fixed order: the `@everyone` overwrite
//!    (deny, then allow), all of the member's role overwrites aggregated
//!    (deny, then allow), then the member's own overwrite (deny, then allow).
//!    Threads use their parent channel's overwrites.
//! 4. A member with an active timeout keeps only `VIEW_CHANNEL` and
//!    `READ_MESSAGE_HISTORY` (owners and administrators are exempt).
//! 5. Implicit permissions: see [`MemberPermissionInput::in_channel`].
//!
//! Steps 4 and 5 make this an effective-permission projection rather than a
//! bit-for-bit port: discord.js `GuildChannel.permissionsFor()` stops after
//! step 3 (`ThreadChannel.permissionsFor()` delegates to the parent), and
//! `GuildMember.permissions` stops after step 2. Those values are kept in
//! [`ComputedPermissions::raw`].
//!
//! Three layers are provided:
//!
//! - [`MemberPermissionInput`] + [`ChannelPermissionInput`]: the pure core
//!   over explicit, already-resolved data. It never fails.
//! - [`guild_permissions`] / [`permissions_for`]: resolve the inputs from
//!   [`Guild`], [`Member`], and [`Channel`] models and report missing data as
//!   a typed [`PermissionDataError`].
//! - [`cached_guild_permissions`] / [`cached_permissions_for`]: the same,
//!   reading the models from a [`CacheHandle`] through its public read API.
//!
//! ```text
//! [Decision Log]
//! - 목적과 의도: discord.js permissionsFor에 해당하는 계산을 제공하되, 불완전한
//!   데이터(가려진 채널, 캐시 누락)를 권위 있는 답으로 오인하지 않게 한다.
//! - 기존 구현 및 제약 조건: 크레이트에는 비트 타입(`Permissions`,
//!   `PermissionsBitField`)만 있고 계산은 없다. 2026-11-16부터 봇이 볼 수
//!   없는 채널은 합성된 @everyone VIEW_CHANNEL deny 하나만 가진 채로 온다.
//!   discord.js permissionsFor는 덮어쓰기 계산까지만 하고 타임아웃·암묵 규칙은
//!   적용하지 않는다.
//! - 검토한 주요 대안: (1) Guild/Member/Channel만 받는 함수 하나,
//!   (2) twilight처럼 채널 종류별로 무관한 비트를 지우는 계산기,
//!   (3) 명시적 입력의 순수 코어 + 모델 해석 계층 + 캐시 어댑터.
//! - 선택한 방식: (3). 결과는 `ChannelPermissions::{Known, Obfuscated}`이고
//!   누락 데이터는 `PermissionDataError`로 이름을 붙여 돌려준다. 실제로 쓸 수
//!   있는 권한(`permissions`)과 discord.js와 같은 덮어쓰기 결과(`raw`)를 함께
//!   돌려준다. 모델·캐시 계층은 소유자·관리자 여부를 먼저 판정해, 답을 바꿀 수
//!   없는 데이터가 없다는 이유로 실패하지 않는다(모순된 길드·채널 정보는 항상
//!   오류).
//! - 다른 대안 대신 이 방식을 선택한 이유: 캐시가 없는 HTTP 인터랙션 봇도
//!   자기 데이터로 계산할 수 있어야 하고, 캐시 어댑터가 "무엇이 없는지"를
//!   정확히 말하려면 해석 단계가 코어와 분리돼야 한다. 채널 종류별 비트 제거는
//!   Discord가 문서화하지 않았으므로 하지 않는다(discord.js와 같음).
//! - 장점, 단점 및 영향: 문서화된 규칙만 구현하므로 "such as"로만 언급된
//!   암묵 규칙(CONNECT 없을 때 MANAGE_CHANNELS 외의 음성 권한 등)과 비공개
//!   스레드 멤버십·잠긴 스레드 상태는 모델링하지 않는다. 비트 이름과 전체
//!   집합은 `bitfield::permissions`/`Permissions::all()` 하나만 쓴다.
//! ```

use std::fmt;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::bitfield::{permissions as flag, Permissions};
use crate::cache::CacheHandle;
use crate::error::DiscordError;
use crate::model::{Channel, GenericId, Guild, Member, PermissionOverwrite, Role};

/// Permissions that cannot be set in channel overwrites (no T/V/S column in
/// Discord's table): KICK_MEMBERS, BAN_MEMBERS, ADMINISTRATOR, MANAGE_GUILD,
/// VIEW_AUDIT_LOG, VIEW_GUILD_INSIGHTS, CHANGE_NICKNAME, MANAGE_NICKNAMES,
/// MANAGE_GUILD_EXPRESSIONS, MODERATE_MEMBERS,
/// VIEW_CREATOR_MONETIZATION_ANALYTICS, and CREATE_GUILD_EXPRESSIONS.
///
/// Losing `VIEW_CHANNEL` keeps only these. Bits not listed here, including
/// ones newer than the crate, are dropped with the channel-scoped ones: most
/// recent additions (`SEND_POLLS`, `PIN_MESSAGES`, `BYPASS_SLOWMODE`) are
/// channel-scoped, and erring toward "not allowed" fails safe. Add new
/// guild-wide bits here when `bitfield::permissions` gains them.
const GUILD_WIDE: u64 = flag::KICK_MEMBERS.bits()
    | flag::BAN_MEMBERS.bits()
    | flag::ADMINISTRATOR.bits()
    | flag::MANAGE_GUILD.bits()
    | flag::VIEW_AUDIT_LOG.bits()
    | flag::VIEW_GUILD_INSIGHTS.bits()
    | flag::CHANGE_NICKNAME.bits()
    | flag::MANAGE_NICKNAMES.bits()
    | flag::MANAGE_EMOJIS_AND_STICKERS.bits()
    | flag::MODERATE_MEMBERS.bits()
    | flag::VIEW_CREATOR_MONETIZATION_ANALYTICS.bits()
    | flag::CREATE_GUILD_EXPRESSIONS.bits();

/// What a timed-out member keeps.
const TIMEOUT_ALLOWED: u64 = flag::VIEW_CHANNEL.bits() | flag::READ_MESSAGE_HISTORY.bits();

/// Implicitly denied when the member cannot send messages (Discord lists
/// exactly these four).
const SEND_DEPENDENT: u64 = flag::MENTION_EVERYONE.bits()
    | flag::SEND_TTS_MESSAGES.bits()
    | flag::ATTACH_FILES.bits()
    | flag::EMBED_LINKS.bits();

/// `PermissionOverwrite::kind` values.
const OVERWRITE_ROLE: u8 = 0;
const OVERWRITE_MEMBER: u8 = 1;

/// Discord channel type `GUILD_DIRECTORY`, which cannot contain threads.
const GUILD_DIRECTORY: u8 = 14;

/// Why a [`ComputedPermissions`] holds what it holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PermissionSource {
    /// The member owns the guild: every permission, overwrites and timeouts
    /// ignored.
    Owner,
    /// A base role grants `ADMINISTRATOR`: every permission, overwrites and
    /// timeouts ignored.
    Administrator,
    /// Computed from roles (and, for a channel, overwrites, timeout, and
    /// implicit rules).
    Computed,
}

/// A fully computed permission set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ComputedPermissions {
    /// The effective permissions: what the member can actually use after the
    /// timeout and implicit rules. [`Permissions::all()`] for owners and
    /// administrators.
    pub permissions: Permissions,
    /// The permissions before the timeout and implicit rules: roles plus
    /// overwrites for a channel (what discord.js `permissionsFor()` returns,
    /// with a thread answered from its parent) or roles alone at guild
    /// level (discord.js `member.permissions`). Equal to `permissions` for
    /// owners and administrators.
    pub raw: Permissions,
    /// Whether this came from ownership, `ADMINISTRATOR`, or the full
    /// calculation.
    pub source: PermissionSource,
    /// `true` when an active timeout restricted the result to
    /// `VIEW_CHANNEL | READ_MESSAGE_HISTORY`.
    pub timeout_applied: bool,
}

impl ComputedPermissions {
    fn bypass(source: PermissionSource) -> Self {
        Self {
            permissions: Permissions::all(),
            raw: Permissions::all(),
            source,
            timeout_applied: false,
        }
    }

    /// Returns `true` when every bit of `required` is granted. Accepts a
    /// [`Permissions`] value (`permissions::SEND_MESSAGES | ...`) or raw
    /// bits.
    ///
    /// Owners and administrators pass every check, including permissions
    /// newer than this crate's [`Permissions::all()`], because Discord lets
    /// them bypass all permission checks.
    pub fn has(&self, required: impl Into<u64>) -> bool {
        self.is_bypass() || self.permissions.contains(required.into())
    }

    /// Returns the bits of `required` that are not granted (empty for owners
    /// and administrators), like discord.js `permissions.missing(...)`.
    pub fn missing(&self, required: impl Into<u64>) -> Permissions {
        if self.is_bypass() {
            return Permissions::default();
        }
        self.permissions
            .missing(Permissions::from_bits(required.into()))
    }

    /// Raw permission bits.
    pub fn bits(&self) -> u64 {
        self.permissions.bits()
    }

    fn is_bypass(&self) -> bool {
        matches!(
            self.source,
            PermissionSource::Owner | PermissionSource::Administrator
        )
    }
}

/// Result of a channel permission calculation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChannelPermissions {
    /// The permissions are known.
    Known(ComputedPermissions),
    /// The channel (or a thread's parent) arrived with the
    /// `CHANNEL_OBFUSCATED` flag, so its real overwrites are unknown and no
    /// answer would be trustworthy.
    ///
    /// Discord obfuscates only channels the connected bot cannot view, so for
    /// the bot itself this implies it lacks `VIEW_CHANNEL` there. Owners and
    /// administrators never get this outcome: their answer does not depend on
    /// overwrites.
    Obfuscated,
}

impl ChannelPermissions {
    /// The computed permissions, or `None` when they are unknown.
    pub fn known(self) -> Option<ComputedPermissions> {
        match self {
            Self::Known(permissions) => Some(permissions),
            Self::Obfuscated => None,
        }
    }

    /// Whether the channel was obfuscated.
    pub fn is_obfuscated(&self) -> bool {
        matches!(self, Self::Obfuscated)
    }

    /// Three-valued check: `Some(true)`/`Some(false)` when known, `None` when
    /// the channel is obfuscated. See [`ComputedPermissions::has`].
    pub fn has(&self, required: impl Into<u64>) -> Option<bool> {
        match self {
            Self::Known(permissions) => Some(permissions.has(required)),
            Self::Obfuscated => None,
        }
    }
}

/// Which implicit-permission rules a channel follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ChannelClass {
    /// GUILD_TEXT (0), GUILD_ANNOUNCEMENT (5), GUILD_FORUM (15), and
    /// GUILD_MEDIA (16): Discord's "T" channels.
    Text,
    /// GUILD_VOICE (2) and GUILD_STAGE_VOICE (13).
    Voice,
    /// GUILD_CATEGORY (4).
    Category,
    /// ANNOUNCEMENT_THREAD (10), PUBLIC_THREAD (11), PRIVATE_THREAD (12).
    /// A thread's [`ChannelPermissionInput`] must carry its **parent's**
    /// overwrites.
    Thread,
    /// GUILD_DIRECTORY (14) and channel types this crate does not know yet:
    /// only the `VIEW_CHANNEL` rule applies.
    Other,
}

impl ChannelClass {
    /// Classifies a Discord channel `type`. Returns `None` for DM (1) and
    /// group DM (3), which have no guild permissions.
    pub fn from_channel_type(kind: u8) -> Option<Self> {
        Some(match kind {
            1 | 3 => return None,
            0 | 5 | 15 | 16 => Self::Text,
            2 | 13 => Self::Voice,
            4 => Self::Category,
            10..=12 => Self::Thread,
            _ => Self::Other,
        })
    }
}

/// The member side of a permission calculation: guild, owner, `@everyone`,
/// the member's roles, and their timeout. All data is explicit; nothing is
/// read from a clock or a cache.
///
/// ```
/// use discordrs::bitfield::permissions;
/// use discordrs::permissions::{ChannelClass, ChannelPermissionInput, MemberPermissionInput};
/// use discordrs::GenericId;
///
/// # fn main() -> Result<(), discordrs::InvalidId> {
/// let guild = GenericId::try_from(100_u64)?;
/// let owner = GenericId::try_from(1_u64)?;
/// let member = GenericId::try_from(2_u64)?;
/// let moderators: GenericId = "300".parse()?;
///
/// let input = MemberPermissionInput::new(guild, owner, member, permissions::VIEW_CHANNEL)
///     .role(moderators, permissions::SEND_MESSAGES | permissions::ATTACH_FILES);
///
/// let channel = ChannelPermissionInput::new(ChannelClass::Text, &[]);
/// let result = input.in_channel(&channel).known().unwrap();
/// assert!(result.has(permissions::ATTACH_FILES));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct MemberPermissionInput {
    guild_id: GenericId,
    owner_id: GenericId,
    member_id: GenericId,
    everyone: Permissions,
    roles: Vec<(GenericId, Permissions)>,
    timed_out: bool,
}

impl MemberPermissionInput {
    /// Starts an input for `member_id` in `guild_id` (owned by `owner_id`),
    /// where `everyone` is the `@everyone` role's permissions.
    pub fn new(
        guild_id: GenericId,
        owner_id: GenericId,
        member_id: GenericId,
        everyone: Permissions,
    ) -> Self {
        Self {
            guild_id,
            owner_id,
            member_id,
            everyone,
            roles: Vec::new(),
            timed_out: false,
        }
    }

    /// Adds one of the member's roles with its guild-level permissions.
    /// An entry for the `@everyone` role (ID equal to the guild ID) only adds
    /// its permissions to the base; it never counts as a role overwrite.
    pub fn role(mut self, role_id: GenericId, permissions: Permissions) -> Self {
        self.roles.push((role_id, permissions));
        self
    }

    /// Adds several roles; see [`MemberPermissionInput::role`].
    pub fn roles<I>(mut self, roles: I) -> Self
    where
        I: IntoIterator<Item = (GenericId, Permissions)>,
    {
        self.roles.extend(roles);
        self
    }

    /// Sets the member's `communication_disabled_until`. The member counts
    /// as timed out when `until` is later than `now` (the discord.js
    /// `isCommunicationDisabled()` rule); `now` is a parameter so the result
    /// stays reproducible.
    pub fn communication_disabled_until(mut self, until: SystemTime, now: SystemTime) -> Self {
        self.timed_out = until > now;
        self
    }

    /// Guild-level permissions: owner and `ADMINISTRATOR` short-circuit to
    /// everything, otherwise `@everyone` OR the roles (discord.js
    /// `member.permissions`, kept in [`ComputedPermissions::raw`]), limited
    /// by an active timeout.
    pub fn guild_permissions(&self) -> ComputedPermissions {
        let base = self.base_permissions();
        match self.bypass(base) {
            Some(bypass) => bypass,
            None => self.finish(base.bits()),
        }
    }

    /// Effective permissions in a channel.
    ///
    /// This is a projection of discord.js `channel.permissionsFor(member)`,
    /// not a bit-for-bit port: discord.js returns the overwrite calculation
    /// alone (kept here in [`ComputedPermissions::raw`]), and
    /// `ThreadChannel.permissionsFor()` simply delegates to the parent. Here
    /// the timeout and the implicit rules Discord documents are applied on
    /// top:
    ///
    /// - Without `VIEW_CHANNEL`, every channel-scoped permission is dropped;
    ///   guild-wide ones such as `KICK_MEMBERS` remain, as in discord.js.
    /// - In [`ChannelClass::Text`] and [`ChannelClass::Voice`] channels,
    ///   without `SEND_MESSAGES` the member also loses `MENTION_EVERYONE`,
    ///   `SEND_TTS_MESSAGES`, `ATTACH_FILES`, and `EMBED_LINKS`.
    /// - In threads `SEND_MESSAGES` is not inherited, so it is removed, and
    ///   `SEND_MESSAGES_IN_THREADS` takes its place in the rule above.
    /// - In [`ChannelClass::Voice`] channels, without `CONNECT` the member
    ///   loses `MANAGE_CHANNELS`.
    ///
    /// Not modeled (Discord does not document them as permission bits):
    /// private-thread visibility (Discord requires thread membership or
    /// `MANAGE_THREADS`), archived or locked thread state,
    /// voice-connection-based visibility, other "such as" consequences of a
    /// missing `CONNECT`, and `SEND_POLLS` / `SEND_VOICE_MESSAGES` without
    /// `SEND_MESSAGES`. Bits that do not apply to the channel type (e.g.
    /// `CONNECT` in a text channel) are kept.
    pub fn in_channel(&self, channel: &ChannelPermissionInput<'_>) -> ChannelPermissions {
        let base = self.base_permissions();
        if let Some(bypass) = self.bypass(base) {
            return ChannelPermissions::Known(bypass);
        }
        if channel.obfuscated {
            return ChannelPermissions::Obfuscated;
        }

        let overwritten = self.apply_overwrites(base, channel.overwrites);
        let mut computed = self.finish(overwritten);
        computed.permissions = apply_implicit(computed.permissions, channel.class);
        ChannelPermissions::Known(computed)
    }

    fn bypass(&self, base: Permissions) -> Option<ComputedPermissions> {
        if self.member_id == self.owner_id {
            return Some(ComputedPermissions::bypass(PermissionSource::Owner));
        }
        if base.contains(flag::ADMINISTRATOR.bits()) {
            return Some(ComputedPermissions::bypass(PermissionSource::Administrator));
        }
        None
    }

    fn base_permissions(&self) -> Permissions {
        self.roles
            .iter()
            .fold(self.everyone, |acc, (_, permissions)| acc | *permissions)
    }

    fn has_role(&self, role_id: GenericId) -> bool {
        role_id != self.guild_id && self.roles.iter().any(|(id, _)| *id == role_id)
    }

    /// Discord's `compute_overwrites`. Allow/deny pairs are aggregated per
    /// tier, so duplicate overwrites for one target cannot reorder the tiers.
    fn apply_overwrites(&self, base: Permissions, overwrites: &[PermissionOverwrite]) -> u64 {
        let mut everyone = (0u64, 0u64);
        let mut roles = (0u64, 0u64);
        let mut member = (0u64, 0u64);
        for overwrite in overwrites {
            let tier = match overwrite.kind {
                OVERWRITE_ROLE if overwrite.id == self.guild_id => &mut everyone,
                OVERWRITE_ROLE if self.has_role(overwrite.id) => &mut roles,
                OVERWRITE_MEMBER if overwrite.id == self.member_id => &mut member,
                // Other members' and roles' overwrites, plus overwrite types
                // Discord has not defined, do not apply to this member.
                _ => continue,
            };
            tier.0 |= overwrite.allow.map_or(0, |bits| bits.bits());
            tier.1 |= overwrite.deny.map_or(0, |bits| bits.bits());
        }

        [everyone, roles, member]
            .iter()
            .fold(base.bits(), |acc, (allow, deny)| (acc & !deny) | allow)
    }

    fn finish(&self, raw: u64) -> ComputedPermissions {
        let (bits, timeout_applied) = if self.timed_out {
            (raw & TIMEOUT_ALLOWED, true)
        } else {
            (raw, false)
        };
        ComputedPermissions {
            permissions: Permissions::from_bits(bits),
            raw: Permissions::from_bits(raw),
            source: PermissionSource::Computed,
            timeout_applied,
        }
    }
}

fn apply_implicit(permissions: Permissions, class: ChannelClass) -> Permissions {
    let mut bits = permissions.bits();
    if bits & flag::VIEW_CHANNEL.bits() == 0 {
        return Permissions::from_bits(bits & GUILD_WIDE);
    }

    let send_gate = match class {
        ChannelClass::Text | ChannelClass::Voice => Some(flag::SEND_MESSAGES.bits()),
        ChannelClass::Thread => {
            bits &= !flag::SEND_MESSAGES.bits();
            Some(flag::SEND_MESSAGES_IN_THREADS.bits())
        }
        ChannelClass::Category | ChannelClass::Other => None,
    };
    if send_gate.is_some_and(|gate| bits & gate == 0) {
        bits &= !SEND_DEPENDENT;
    }

    if class == ChannelClass::Voice && bits & flag::CONNECT.bits() == 0 {
        bits &= !flag::MANAGE_CHANNELS.bits();
    }
    Permissions::from_bits(bits)
}

/// The channel side of a permission calculation.
#[derive(Clone, Copy, Debug)]
pub struct ChannelPermissionInput<'a> {
    class: ChannelClass,
    overwrites: &'a [PermissionOverwrite],
    obfuscated: bool,
}

impl<'a> ChannelPermissionInput<'a> {
    /// A channel of `class` with these overwrites. For
    /// [`ChannelClass::Thread`] pass the parent channel's overwrites.
    pub fn new(class: ChannelClass, overwrites: &'a [PermissionOverwrite]) -> Self {
        Self {
            class,
            overwrites,
            obfuscated: false,
        }
    }

    /// Marks the channel (or a thread's parent) as obfuscated, which makes
    /// the outcome [`ChannelPermissions::Obfuscated`] for everyone but owners
    /// and administrators.
    pub fn obfuscated(mut self, obfuscated: bool) -> Self {
        self.obfuscated = obfuscated;
        self
    }

    /// Builds the input from a channel model. `parent` is required for
    /// threads (their overwrites live on the parent) and ignored otherwise.
    ///
    /// Fails, instead of guessing, when the channel is a DM, belongs to a
    /// different guild than `guild_id`, is a thread without a matching
    /// parent, or lacks `permission_overwrites` (partial channels such as
    /// interaction `resolved` entries omit them). Obfuscated channels do not
    /// need overwrites.
    ///
    /// A thread's parent must be a text, announcement, forum, or media
    /// channel; voice, stage, category, directory, and thread parents are
    /// rejected. Parents of channel types this crate does not know yet are
    /// accepted as the caller asserts them.
    pub fn from_channel(
        guild_id: GenericId,
        channel: &'a Channel,
        parent: Option<&'a Channel>,
    ) -> Result<Self, PermissionDataError> {
        let class = validate_target(guild_id, channel, parent)?;
        if class != ChannelClass::Thread {
            return overwrite_source(class, channel, channel.is_obfuscated());
        }
        let parent = parent.ok_or(PermissionDataError::MissingParentChannel {
            thread_id: channel.id,
            parent_id: channel.parent_id,
        })?;
        overwrite_source(
            ChannelClass::Thread,
            parent,
            channel.is_obfuscated() || parent.is_obfuscated(),
        )
    }
}

/// Checks that `channel` (and a thread's `parent`, when given) can belong
/// to `guild_id` at all. Only contradictions fail here; data that is merely
/// absent is left to [`ChannelPermissionInput::from_channel`], so owners and
/// administrators, whose answer needs no overwrites, are not blocked by it.
fn validate_target(
    guild_id: GenericId,
    channel: &Channel,
    parent: Option<&Channel>,
) -> Result<ChannelClass, PermissionDataError> {
    let class = guild_channel_class(guild_id, channel)?;
    let Some(parent) = parent.filter(|_| class == ChannelClass::Thread) else {
        return Ok(class);
    };
    if let Some(expected) = &channel.parent_id {
        if *expected != parent.id {
            return Err(PermissionDataError::ParentMismatch {
                thread_id: channel.id,
                expected: *expected,
                found: parent.id,
            });
        }
    }
    let parent_class = guild_channel_class(guild_id, parent)?;
    // Threads live in GUILD_TEXT, GUILD_ANNOUNCEMENT, GUILD_FORUM, and
    // GUILD_MEDIA channels. GUILD_DIRECTORY (14) is `Other` but known.
    let impossible = matches!(
        parent_class,
        ChannelClass::Voice | ChannelClass::Category | ChannelClass::Thread
    ) || parent.kind == GUILD_DIRECTORY;
    if impossible {
        return Err(PermissionDataError::InvalidParentChannel {
            thread_id: channel.id,
            parent_id: parent.id,
            kind: parent.kind,
        });
    }
    Ok(class)
}

fn guild_channel_class(
    guild_id: GenericId,
    channel: &Channel,
) -> Result<ChannelClass, PermissionDataError> {
    let class = ChannelClass::from_channel_type(channel.kind).ok_or({
        PermissionDataError::NotGuildChannel {
            channel_id: channel.id,
            kind: channel.kind,
        }
    })?;
    // GUILD_CREATE channels omit `guild_id`; only a contradicting value is
    // an error.
    if let Some(found) = &channel.guild_id {
        if *found != guild_id {
            return Err(PermissionDataError::GuildMismatch {
                channel_id: channel.id,
                expected: guild_id,
                found: *found,
            });
        }
    }
    Ok(class)
}

fn overwrite_source(
    class: ChannelClass,
    source: &Channel,
    obfuscated: bool,
) -> Result<ChannelPermissionInput<'_>, PermissionDataError> {
    let overwrites = match (&source.permission_overwrites, obfuscated) {
        (Some(overwrites), _) => overwrites.as_slice(),
        (None, true) => &[],
        (None, false) => {
            return Err(PermissionDataError::MissingPermissionOverwrites {
                channel_id: source.id,
            })
        }
    };
    Ok(ChannelPermissionInput::new(class, overwrites).obfuscated(obfuscated))
}

/// Data a permission calculation needed but did not get, or got in a
/// contradictory form. Every variant names the missing object so callers
/// can fetch it instead of acting on a guess.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PermissionDataError {
    /// The crate was built without the `cache` feature, so the cache adapter
    /// has nothing to read.
    CacheDisabled,
    /// The guild is not cached.
    MissingGuild {
        /// Guild that was looked up.
        guild_id: GenericId,
    },
    /// The guild object has no `owner_id`.
    MissingGuildOwner {
        /// Guild whose owner is unknown.
        guild_id: GenericId,
    },
    /// A role (the `@everyone` role when `role_id == guild_id`) is not
    /// available.
    MissingRole {
        /// Guild the role belongs to.
        guild_id: GenericId,
        /// Role that was looked up.
        role_id: GenericId,
    },
    /// A role object has no `permissions` field.
    MissingRolePermissions {
        /// Role without permissions.
        role_id: GenericId,
    },
    /// The member is not cached.
    MissingMember {
        /// Guild that was looked up.
        guild_id: GenericId,
        /// User whose member object is missing.
        user_id: GenericId,
    },
    /// The member object has no `user`, so its ID (needed for the owner
    /// check and member overwrites) is unknown. Message `member` objects are
    /// like this; copy `message.author` into `member.user` first.
    MissingMemberUser,
    /// The channel is not cached.
    MissingChannel {
        /// Channel that was looked up.
        channel_id: GenericId,
    },
    /// A thread was given without its parent channel (or the parent is not
    /// cached).
    MissingParentChannel {
        /// The thread.
        thread_id: GenericId,
        /// The thread's `parent_id`, when the thread object has one.
        parent_id: Option<GenericId>,
    },
    /// The channel has no `permission_overwrites` (a partial channel).
    MissingPermissionOverwrites {
        /// Channel without overwrites.
        channel_id: GenericId,
    },
    /// The channel is a DM or group DM.
    NotGuildChannel {
        /// The channel.
        channel_id: GenericId,
        /// Its Discord channel type.
        kind: u8,
    },
    /// The channel belongs to another guild.
    GuildMismatch {
        /// The channel.
        channel_id: GenericId,
        /// Guild the calculation is for.
        expected: GenericId,
        /// Guild the channel says it belongs to.
        found: GenericId,
    },
    /// The supplied parent is not the thread's `parent_id`.
    ParentMismatch {
        /// The thread.
        thread_id: GenericId,
        /// The thread's `parent_id`.
        expected: GenericId,
        /// ID of the channel supplied as parent.
        found: GenericId,
    },
    /// The supplied parent cannot contain threads (a voice, stage,
    /// category, directory, or thread channel).
    InvalidParentChannel {
        /// The thread.
        thread_id: GenericId,
        /// The supplied parent.
        parent_id: GenericId,
        /// The parent's Discord channel type.
        kind: u8,
    },
    /// `communication_disabled_until` is not an ISO 8601 timestamp.
    InvalidTimestamp {
        /// The unparseable value.
        value: String,
    },
}

impl fmt::Display for PermissionDataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CacheDisabled => f.write_str("the `cache` feature is disabled"),
            Self::MissingGuild { guild_id } => write!(f, "guild {guild_id} is not cached"),
            Self::MissingGuildOwner { guild_id } => {
                write!(f, "guild {guild_id} has no owner_id")
            }
            Self::MissingRole { guild_id, role_id } if guild_id == role_id => {
                write!(f, "@everyone role of guild {guild_id} is not available")
            }
            Self::MissingRole { guild_id, role_id } => {
                write!(f, "role {role_id} of guild {guild_id} is not available")
            }
            Self::MissingRolePermissions { role_id } => {
                write!(f, "role {role_id} has no permissions field")
            }
            Self::MissingMember { guild_id, user_id } => {
                write!(f, "member {user_id} of guild {guild_id} is not cached")
            }
            Self::MissingMemberUser => f.write_str("member object has no user"),
            Self::MissingChannel { channel_id } => {
                write!(f, "channel {channel_id} is not cached")
            }
            Self::MissingParentChannel {
                thread_id,
                parent_id: Some(parent_id),
            } => write!(
                f,
                "parent channel {parent_id} of thread {thread_id} is not available"
            ),
            Self::MissingParentChannel {
                thread_id,
                parent_id: None,
            } => write!(f, "thread {thread_id} has no parent channel"),
            Self::MissingPermissionOverwrites { channel_id } => {
                write!(f, "channel {channel_id} has no permission_overwrites")
            }
            Self::NotGuildChannel { channel_id, kind } => {
                write!(
                    f,
                    "channel {channel_id} (type {kind}) is not a guild channel"
                )
            }
            Self::GuildMismatch {
                channel_id,
                expected,
                found,
            } => write!(
                f,
                "channel {channel_id} belongs to guild {found}, not {expected}"
            ),
            Self::ParentMismatch {
                thread_id,
                expected,
                found,
            } => write!(
                f,
                "thread {thread_id} has parent {expected}, but channel {found} was supplied"
            ),
            Self::InvalidParentChannel {
                thread_id,
                parent_id,
                kind,
            } => write!(
                f,
                "channel {parent_id} (type {kind}) cannot be the parent of thread {thread_id}"
            ),
            Self::InvalidTimestamp { value } => {
                write!(
                    f,
                    "invalid communication_disabled_until timestamp {value:?}"
                )
            }
        }
    }
}

impl std::error::Error for PermissionDataError {}

impl From<PermissionDataError> for DiscordError {
    fn from(error: PermissionDataError) -> Self {
        DiscordError::model(error.to_string())
    }
}

/// Guild-level permissions of `member`, resolving roles from `guild.roles`.
/// [`ComputedPermissions::raw`] equals discord.js `member.permissions`;
/// [`ComputedPermissions::permissions`] also applies an active timeout.
///
/// Owners and administrators are answered as soon as that is known, so data
/// that cannot change their answer (other roles, the timeout) is not
/// required.
pub fn guild_permissions(
    guild: &Guild,
    member: &Member,
    now: SystemTime,
) -> Result<ComputedPermissions, PermissionDataError> {
    match MemberFacts::from_guild(guild, member).resolve(now)? {
        Resolution::Bypass(bypass) => Ok(bypass),
        Resolution::Member(resolved) => Ok(resolved.input().guild_permissions()),
    }
}

/// Effective permissions of `member` in `channel`, resolving roles from
/// `guild.roles`; see [`MemberPermissionInput::in_channel`] for how this
/// relates to discord.js `channel.permissionsFor(member)`. `parent` is
/// required when `channel` is a thread and the member is neither owner nor
/// administrator.
///
/// Contradictions (a DM, a channel or parent from another guild, a parent
/// that is not the thread's `parent_id` or cannot hold threads) are always
/// errors. Missing data is only an error when the answer depends on it:
/// owners and administrators need no roles beyond the one granting
/// `ADMINISTRATOR`, no overwrites, no parent, and no timeout.
///
/// `now` decides whether the member's timeout is active; pass
/// `SystemTime::now()` outside tests.
pub fn permissions_for(
    guild: &Guild,
    member: &Member,
    channel: &Channel,
    parent: Option<&Channel>,
    now: SystemTime,
) -> Result<ChannelPermissions, PermissionDataError> {
    validate_target(guild.id, channel, parent)?;
    match MemberFacts::from_guild(guild, member).resolve(now)? {
        Resolution::Bypass(bypass) => Ok(ChannelPermissions::Known(bypass)),
        Resolution::Member(resolved) => {
            let target = ChannelPermissionInput::from_channel(guild.id, channel, parent)?;
            Ok(resolved.input().in_channel(&target))
        }
    }
}

/// Guild-level permissions of a cached member. See [`cached_permissions_for`]
/// for what is read.
pub async fn cached_guild_permissions(
    cache: &CacheHandle,
    guild_id: GenericId,
    user_id: GenericId,
    now: SystemTime,
) -> Result<ComputedPermissions, PermissionDataError> {
    match MemberFacts::from_cache(cache, guild_id, user_id)
        .await?
        .resolve(now)?
    {
        Resolution::Bypass(bypass) => Ok(bypass),
        Resolution::Member(resolved) => Ok(resolved.input().guild_permissions()),
    }
}

/// Permissions of a cached member in a cached channel.
///
/// Reads the guild (for `owner_id`), the member, the channel, a thread's
/// parent, and every role from the cache's role store — not from the
/// cached guild's `roles` list, which is a GUILD_CREATE snapshot that role
/// update events do not refresh. Each piece is a separate cache read, so a
/// concurrent update can be observed half-applied, exactly as when reading
/// the cache field by field.
///
/// Anything the answer depends on but the cache lacks is reported as a
/// [`PermissionDataError`]; nothing is filled in with defaults. The channel
/// itself is always required, because without it the channel cannot be
/// checked against the guild; as in [`permissions_for`], owners and
/// administrators need nothing else beyond what proves their status.
pub async fn cached_permissions_for(
    cache: &CacheHandle,
    guild_id: GenericId,
    user_id: GenericId,
    channel_id: GenericId,
    now: SystemTime,
) -> Result<ChannelPermissions, PermissionDataError> {
    let facts = MemberFacts::from_cache(cache, guild_id, user_id).await?;
    let channel = cache
        .channel(channel_id)
        .await
        .ok_or(PermissionDataError::MissingChannel { channel_id })?;
    let parent = match (&channel.parent_id, channel.is_thread()) {
        (Some(parent_id), true) => cache.channel(*parent_id).await,
        _ => None,
    };
    validate_target(guild_id, &channel, parent.as_ref())?;
    match facts.resolve(now)? {
        Resolution::Bypass(bypass) => Ok(ChannelPermissions::Known(bypass)),
        Resolution::Member(resolved) => {
            let target = ChannelPermissionInput::from_channel(guild_id, &channel, parent.as_ref())?;
            Ok(resolved.input().in_channel(&target))
        }
    }
}

/// A piece of member-side data, or the error naming why it is absent.
type Found<T> = Result<T, PermissionDataError>;

/// Member-side data as found in models or the cache, before deciding which
/// of it the answer needs. An absent piece holds the error that names it.
///
/// Resolution is two-step so owners and administrators are answered from
/// the facts that prove their status, while everyone else still gets an
/// error naming the first missing piece.
struct MemberFacts {
    guild_id: GenericId,
    user_id: Found<GenericId>,
    owner_id: Found<GenericId>,
    everyone: Found<Permissions>,
    /// `Err` when the member object itself is missing; otherwise one entry
    /// per role the member lists.
    roles: Found<Vec<Found<(GenericId, Permissions)>>>,
    communication_disabled_until: Option<String>,
}

enum Resolution {
    /// Owner or administrator: every permission, nothing else needed.
    Bypass(ComputedPermissions),
    /// Everything needed for the full calculation.
    Member(ResolvedMember),
}

impl MemberFacts {
    fn from_guild(guild: &Guild, member: &Member) -> Self {
        let find = |role_id: GenericId| {
            guild
                .roles
                .iter()
                .find(|role| role.id == role_id)
                .ok_or(PermissionDataError::MissingRole {
                    guild_id: guild.id,
                    role_id,
                })
                .and_then(role_permissions)
        };
        Self {
            guild_id: guild.id,
            user_id: member
                .user
                .as_ref()
                .map(|user| user.id)
                .ok_or(PermissionDataError::MissingMemberUser),
            owner_id: guild
                .owner_id
                .ok_or(PermissionDataError::MissingGuildOwner { guild_id: guild.id }),
            everyone: find(guild.id),
            roles: Ok(member_role_ids(&guild.id, member)
                .map(|role_id| find(*role_id).map(|permissions| (*role_id, permissions)))
                .collect()),
            communication_disabled_until: member.communication_disabled_until.clone(),
        }
    }

    async fn from_cache(
        cache: &CacheHandle,
        guild_id: GenericId,
        user_id: GenericId,
    ) -> Result<Self, PermissionDataError> {
        if !cache.is_enabled() {
            return Err(PermissionDataError::CacheDisabled);
        }
        let owner_id = match cache.guild(guild_id).await {
            Some(guild) => guild
                .owner_id
                .ok_or(PermissionDataError::MissingGuildOwner { guild_id }),
            None => Err(PermissionDataError::MissingGuild { guild_id }),
        };
        let everyone = cached_role(cache, guild_id, guild_id).await;
        let mut communication_disabled_until = None;
        let roles = match cache.member(guild_id, user_id).await {
            Some(member) => {
                let mut roles = Vec::with_capacity(member.roles.len());
                for role_id in member_role_ids(&guild_id, &member) {
                    let permissions = cached_role(cache, guild_id, *role_id).await;
                    roles.push(permissions.map(|permissions| (*role_id, permissions)));
                }
                communication_disabled_until = member.communication_disabled_until.clone();
                Ok(roles)
            }
            None => Err(PermissionDataError::MissingMember { guild_id, user_id }),
        };
        // The cache key is the user ID, so a member stored without its
        // `user` object (possible through `upsert_member`) is still usable.
        Ok(Self {
            guild_id,
            user_id: Ok(user_id),
            owner_id,
            everyone,
            roles,
            communication_disabled_until,
        })
    }

    fn resolve(self, now: SystemTime) -> Result<Resolution, PermissionDataError> {
        if matches!((&self.user_id, &self.owner_id), (Ok(user), Ok(owner)) if user == owner) {
            return Ok(Resolution::Bypass(ComputedPermissions::bypass(
                PermissionSource::Owner,
            )));
        }
        // The base is an OR, so one known role with ADMINISTRATOR settles it
        // whatever the missing roles hold.
        let grants_admin =
            |permissions: &Permissions| permissions.contains(flag::ADMINISTRATOR.bits());
        let known_admin = self.everyone.as_ref().is_ok_and(grants_admin)
            || self.roles.as_ref().is_ok_and(|roles| {
                roles.iter().any(|role| {
                    role.as_ref()
                        .is_ok_and(|(_, permissions)| grants_admin(permissions))
                })
            });
        if known_admin {
            return Ok(Resolution::Bypass(ComputedPermissions::bypass(
                PermissionSource::Administrator,
            )));
        }

        // Neither shortcut applies, so every piece is needed.
        let user_id = self.user_id?;
        let owner_id = self.owner_id?;
        let roles = self.roles?;
        let everyone = self.everyone?;
        let roles = roles.into_iter().collect::<Result<Vec<_>, _>>()?;
        let timeout = self
            .communication_disabled_until
            .map(|value| {
                parse_timestamp(&value).ok_or(PermissionDataError::InvalidTimestamp { value })
            })
            .transpose()?;
        Ok(Resolution::Member(ResolvedMember {
            guild_id: self.guild_id,
            owner_id,
            user_id,
            everyone,
            roles,
            timeout,
            now,
        }))
    }
}

/// Owned member-side data for the full calculation.
struct ResolvedMember {
    guild_id: GenericId,
    owner_id: GenericId,
    user_id: GenericId,
    everyone: Permissions,
    roles: Vec<(GenericId, Permissions)>,
    timeout: Option<SystemTime>,
    now: SystemTime,
}

impl ResolvedMember {
    fn input(&self) -> MemberPermissionInput {
        let input =
            MemberPermissionInput::new(self.guild_id, self.owner_id, self.user_id, self.everyone)
                .roles(self.roles.iter().copied());
        match self.timeout {
            Some(until) => input.communication_disabled_until(until, self.now),
            None => input,
        }
    }
}

/// The member's role IDs without `@everyone`, which Discord never lists but
/// a hand-built member might.
fn member_role_ids<'m>(
    guild_id: &'m GenericId,
    member: &'m Member,
) -> impl Iterator<Item = &'m GenericId> + 'm {
    member
        .roles
        .iter()
        .filter(move |role_id| *role_id != guild_id)
}

fn role_permissions(role: &Role) -> Result<Permissions, PermissionDataError> {
    role.permissions
        .map(|bits| Permissions::from_bits(bits.bits()))
        .ok_or(PermissionDataError::MissingRolePermissions { role_id: role.id })
}

async fn cached_role(
    cache: &CacheHandle,
    guild_id: GenericId,
    role_id: GenericId,
) -> Result<Permissions, PermissionDataError> {
    let role = cache
        .role(guild_id, role_id)
        .await
        .ok_or(PermissionDataError::MissingRole { guild_id, role_id })?;
    role_permissions(&role)
}

/// Parses the ISO 8601 / RFC 3339 timestamps Discord sends
/// (`2026-05-01T00:00:00+00:00`, `...00.000000+00:00`, `...Z`).
///
/// Hand-written because the crate has no date dependency and only needs an
/// instant to compare against `now`. Accepts `T`/`t`/space as separator,
/// `Z`/`z` or `±HH:MM`/`±HHMM` offsets, up to nanosecond fractions, and a
/// leap second `:60`. Returns `None` for anything else.
pub(crate) fn parse_timestamp(value: &str) -> Option<SystemTime> {
    let bytes = value.as_bytes();
    let number = |range: std::ops::Range<usize>| -> Option<i64> {
        let digits = bytes.get(range)?;
        if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
            return None;
        }
        Some(
            digits
                .iter()
                .fold(0i64, |acc, digit| acc * 10 + i64::from(digit - b'0')),
        )
    };
    let expect =
        |index: usize, allowed: &[u8]| bytes.get(index).is_some_and(|b| allowed.contains(b));

    if !(expect(4, b"-")
        && expect(7, b"-")
        && expect(10, b"Tt ")
        && expect(13, b":")
        && expect(16, b":"))
    {
        return None;
    }
    let (year, month, day) = (number(0..4)?, number(5..7)?, number(8..10)?);
    let (hour, minute, second) = (number(11..13)?, number(14..16)?, number(17..19)?);
    if !(1..=12).contains(&month)
        || day < 1
        || day > days_in_month(year, month)
        || hour > 23
        || minute > 59
        || second > 60
    {
        return None;
    }

    let mut index = 19;
    let mut nanos = 0u32;
    if expect(index, b".") {
        let start = index + 1;
        let end = start
            + bytes[start..]
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
        if end == start {
            return None;
        }
        let mut scale = 100_000_000u32;
        for digit in bytes[start..end].iter().take(9) {
            nanos += u32::from(digit - b'0') * scale;
            scale /= 10;
        }
        index = end;
    }

    let offset_seconds = match bytes.get(index)? {
        b'Z' | b'z' if index + 1 == bytes.len() => 0,
        sign @ (b'+' | b'-') => {
            let rest = &value[index + 1..];
            let (hours, minutes) = match rest.len() {
                5 if rest.as_bytes().get(2) == Some(&b':') => {
                    (number(index + 1..index + 3)?, number(index + 4..index + 6)?)
                }
                4 => (number(index + 1..index + 3)?, number(index + 3..index + 5)?),
                _ => return None,
            };
            if hours > 23 || minutes > 59 {
                return None;
            }
            let magnitude = hours * 3600 + minutes * 60;
            if *sign == b'-' {
                -magnitude
            } else {
                magnitude
            }
        }
        _ => return None,
    };

    let seconds = days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second
        - offset_seconds;
    let whole = Duration::from_secs(seconds.unsigned_abs());
    let instant = if seconds >= 0 {
        UNIX_EPOCH.checked_add(whole)?
    } else {
        UNIX_EPOCH.checked_sub(whole)?
    };
    instant.checked_add(Duration::from_nanos(u64::from(nanos)))
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_index = (month + 9) % 12;
    let day_of_year = (153 * month_index + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests;
