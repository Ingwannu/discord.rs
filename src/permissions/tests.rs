use crate::model::test_id;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::{
    cached_guild_permissions, cached_permissions_for, guild_permissions, parse_timestamp,
    permissions_for, ChannelClass, ChannelPermissionInput, ChannelPermissions, ComputedPermissions,
    MemberPermissionInput, PermissionDataError, PermissionSource,
};
use crate::bitfield::{permissions as p, Permissions};
use crate::cache::CacheHandle;
use crate::error::DiscordError;
use crate::model::{
    Channel, GenericId, Guild, Member, PermissionOverwrite, PermissionsBitField, Role, User,
};

const GUILD: u64 = 100;
const OWNER: u64 = 1;
const MEMBER: u64 = 2;
const OTHER_MEMBER: u64 = 3;
const ROLE_A: u64 = 300;
const ROLE_B: u64 = 301;
const ROLE_C: u64 = 302;
const TEXT: u8 = 0;
const VOICE: u8 = 2;
const CATEGORY: u8 = 4;
const PUBLIC_THREAD: u8 = 11;

fn sf(id: u64) -> GenericId {
    test_id(id)
}

fn bits(permissions: Permissions) -> u64 {
    permissions.bits()
}

fn role_overwrite(id: u64, allow: Permissions, deny: Permissions) -> PermissionOverwrite {
    overwrite(id, 0, allow, deny)
}

fn member_overwrite(id: u64, allow: Permissions, deny: Permissions) -> PermissionOverwrite {
    overwrite(id, 1, allow, deny)
}

fn overwrite(id: u64, kind: u8, allow: Permissions, deny: Permissions) -> PermissionOverwrite {
    PermissionOverwrite {
        id: sf(id),
        kind,
        allow: Some(PermissionsBitField(allow.bits())),
        deny: Some(PermissionsBitField(deny.bits())),
    }
}

fn none() -> Permissions {
    Permissions::default()
}

/// Everyone can view and send; the member has ROLE_A and ROLE_B (no
/// permissions of their own unless a test adds them).
struct Fixture {
    guild: GenericId,
    owner: GenericId,
    member: GenericId,
    role_a: GenericId,
    role_b: GenericId,
}

impl Fixture {
    fn new() -> Self {
        Self {
            guild: sf(GUILD),
            owner: sf(OWNER),
            member: sf(MEMBER),
            role_a: sf(ROLE_A),
            role_b: sf(ROLE_B),
        }
    }

    fn input(&self, everyone: Permissions) -> MemberPermissionInput {
        MemberPermissionInput::new(self.guild, self.owner, self.member, everyone)
            .role(self.role_a, none())
            .role(self.role_b, none())
    }
}

fn known(outcome: ChannelPermissions) -> ComputedPermissions {
    match outcome {
        ChannelPermissions::Known(computed) => computed,
        other => panic!("expected known permissions, got {other:?}"),
    }
}

fn text(overwrites: &[PermissionOverwrite]) -> ChannelPermissionInput<'_> {
    ChannelPermissionInput::new(ChannelClass::Text, overwrites)
}

fn base_everyone() -> Permissions {
    p::VIEW_CHANNEL | p::SEND_MESSAGES | p::READ_MESSAGE_HISTORY
}

// ---------------------------------------------------------------------------
// Owner and administrator
// ---------------------------------------------------------------------------

#[test]
fn owner_gets_everything_despite_overwrites_and_timeout() {
    let fx = Fixture::new();
    let input = MemberPermissionInput::new(fx.guild, fx.owner, fx.owner, none())
        .communication_disabled_until(UNIX_EPOCH + Duration::from_secs(10), UNIX_EPOCH);
    let overwrites = [member_overwrite(OWNER, none(), p::VIEW_CHANNEL)];

    let computed = known(input.in_channel(&text(&overwrites)));
    assert_eq!(computed.permissions, Permissions::all());
    assert_eq!(computed.raw, Permissions::all());
    assert_eq!(computed.source, PermissionSource::Owner);
    assert!(!computed.timeout_applied);
    assert_eq!(input.guild_permissions().source, PermissionSource::Owner);
}

#[test]
fn administrator_role_bypasses_overwrites_and_timeout() {
    let fx = Fixture::new();
    let input = MemberPermissionInput::new(fx.guild, fx.owner, fx.member, none())
        .role(fx.role_a, p::ADMINISTRATOR)
        .communication_disabled_until(UNIX_EPOCH + Duration::from_secs(10), UNIX_EPOCH);
    let overwrites = [
        role_overwrite(GUILD, none(), p::VIEW_CHANNEL),
        member_overwrite(MEMBER, none(), p::VIEW_CHANNEL | p::SEND_MESSAGES),
    ];

    let computed = known(input.in_channel(&text(&overwrites)));
    assert_eq!(computed.permissions, Permissions::all());
    assert_eq!(computed.source, PermissionSource::Administrator);
    assert!(!computed.timeout_applied);
    assert_eq!(input.guild_permissions().permissions, Permissions::all());
}

#[test]
fn administrator_on_everyone_grants_everything() {
    let fx = Fixture::new();
    let computed = fx.input(p::ADMINISTRATOR).guild_permissions();
    assert_eq!(computed.source, PermissionSource::Administrator);
}

#[test]
fn administrator_from_an_overwrite_does_not_bypass() {
    // Discord only checks ADMINISTRATOR on the base permissions.
    let fx = Fixture::new();
    let overwrites = [member_overwrite(MEMBER, p::ADMINISTRATOR, none())];
    let computed = known(fx.input(base_everyone()).in_channel(&text(&overwrites)));
    assert_eq!(computed.source, PermissionSource::Computed);
    assert!(!computed.has(p::KICK_MEMBERS));
}

#[test]
fn bypass_has_every_permission_including_unknown_bits() {
    let fx = Fixture::new();
    let computed = fx.input(p::ADMINISTRATOR).guild_permissions();
    assert!(computed.has(p::SEND_POLLS | p::MANAGE_THREADS));
    assert!(computed.has(1u64 << 60));
    assert!(computed.missing(1u64 << 60).is_empty());
    assert_eq!(computed.bits(), Permissions::all().bits());
}

#[test]
fn bypass_grants_every_documented_permission() {
    // Discord's table: bits 0-46 and 48-52 (bit 47 is retired).
    let all = Permissions::all().bits();
    assert_eq!(all, ((1u64 << 47) - 1) | (0b11111u64 << 48));
    assert!(Permissions::all().contains(p::PIN_MESSAGES.bits() | p::BYPASS_SLOWMODE.bits()));
}

// ---------------------------------------------------------------------------
// Base permissions and overwrite precedence
// ---------------------------------------------------------------------------

#[test]
fn base_is_everyone_or_member_roles() {
    let fx = Fixture::new();
    let input = MemberPermissionInput::new(fx.guild, fx.owner, fx.member, p::VIEW_CHANNEL)
        .roles([(fx.role_a, p::KICK_MEMBERS), (fx.role_b, p::BAN_MEMBERS)]);
    let computed = input.guild_permissions();
    assert_eq!(
        computed.permissions,
        p::VIEW_CHANNEL | p::KICK_MEMBERS | p::BAN_MEMBERS
    );
    assert_eq!(computed.source, PermissionSource::Computed);
    assert!(!computed.timeout_applied);
}

#[test]
fn everyone_overwrite_applies_deny_before_allow() {
    let fx = Fixture::new();
    let overwrites = [role_overwrite(
        GUILD,
        p::ADD_REACTIONS,
        p::SEND_MESSAGES | p::ADD_REACTIONS,
    )];
    let computed = known(fx.input(base_everyone()).in_channel(&text(&overwrites)));
    assert!(!computed.has(p::SEND_MESSAGES));
    assert!(computed.has(p::ADD_REACTIONS), "allow wins inside one tier");
}

#[test]
fn role_overwrites_are_aggregated_so_any_allow_wins() {
    // Discord's example: role A denies VIEW_CHANNEL, role B allows it.
    let fx = Fixture::new();
    let overwrites = [
        role_overwrite(ROLE_A, none(), p::VIEW_CHANNEL),
        role_overwrite(ROLE_B, p::VIEW_CHANNEL, none()),
    ];
    let computed = known(fx.input(base_everyone()).in_channel(&text(&overwrites)));
    assert!(computed.has(p::VIEW_CHANNEL));
}

#[test]
fn role_overwrite_beats_everyone_overwrite() {
    let fx = Fixture::new();
    let allow = [
        role_overwrite(GUILD, none(), p::SEND_MESSAGES),
        role_overwrite(ROLE_A, p::SEND_MESSAGES, none()),
    ];
    assert!(known(fx.input(base_everyone()).in_channel(&text(&allow))).has(p::SEND_MESSAGES));

    let deny = [
        role_overwrite(GUILD, p::EMBED_LINKS, none()),
        role_overwrite(ROLE_B, none(), p::EMBED_LINKS),
    ];
    assert!(!known(fx.input(base_everyone()).in_channel(&text(&deny))).has(p::EMBED_LINKS));
}

#[test]
fn member_overwrite_beats_role_overwrites() {
    let fx = Fixture::new();
    let deny = [
        role_overwrite(ROLE_A, p::ATTACH_FILES, none()),
        member_overwrite(MEMBER, none(), p::ATTACH_FILES),
    ];
    assert!(!known(fx.input(base_everyone()).in_channel(&text(&deny))).has(p::ATTACH_FILES));

    let allow = [
        role_overwrite(ROLE_A, none(), p::SEND_MESSAGES),
        member_overwrite(MEMBER, p::SEND_MESSAGES, none()),
    ];
    assert!(known(fx.input(base_everyone()).in_channel(&text(&allow))).has(p::SEND_MESSAGES));
}

#[test]
fn overwrite_list_order_does_not_change_tier_order() {
    let fx = Fixture::new();
    let overwrites = [
        member_overwrite(MEMBER, p::SEND_MESSAGES, none()),
        role_overwrite(ROLE_A, none(), p::SEND_MESSAGES),
        role_overwrite(GUILD, none(), p::SEND_MESSAGES),
    ];
    assert!(known(fx.input(base_everyone()).in_channel(&text(&overwrites))).has(p::SEND_MESSAGES));
}

#[test]
fn overwrites_for_other_targets_are_ignored() {
    let fx = Fixture::new();
    let overwrites = [
        role_overwrite(ROLE_C, none(), p::SEND_MESSAGES),
        member_overwrite(OTHER_MEMBER, none(), p::VIEW_CHANNEL),
    ];
    let computed = known(fx.input(base_everyone()).in_channel(&text(&overwrites)));
    assert_eq!(computed.permissions, base_everyone());
}

#[test]
fn overwrite_kind_must_match_target() {
    // A member-type overwrite whose ID equals one of the member's role IDs
    // (and vice versa) targets something else.
    let fx = Fixture::new();
    let overwrites = [
        member_overwrite(ROLE_A, none(), p::SEND_MESSAGES),
        role_overwrite(MEMBER, none(), p::VIEW_CHANNEL),
        member_overwrite(GUILD, none(), p::READ_MESSAGE_HISTORY),
    ];
    let computed = known(fx.input(base_everyone()).in_channel(&text(&overwrites)));
    assert_eq!(computed.permissions, base_everyone());
}

#[test]
fn unknown_overwrite_kinds_are_ignored() {
    let fx = Fixture::new();
    let overwrites = [overwrite(MEMBER, 7, none(), p::VIEW_CHANNEL)];
    let computed = known(fx.input(base_everyone()).in_channel(&text(&overwrites)));
    assert!(computed.has(p::VIEW_CHANNEL));
}

#[test]
fn absent_allow_and_deny_fields_count_as_empty() {
    let fx = Fixture::new();
    let overwrites = [PermissionOverwrite {
        id: sf(GUILD),
        kind: 0,
        allow: None,
        deny: None,
    }];
    let computed = known(fx.input(base_everyone()).in_channel(&text(&overwrites)));
    assert_eq!(computed.permissions, base_everyone());
}

#[test]
fn everyone_listed_as_member_role_is_not_a_role_overwrite() {
    // If @everyone were also aggregated as a role overwrite, its allow would
    // cancel ROLE_A's deny.
    let fx = Fixture::new();
    let input = fx.input(base_everyone()).role(fx.guild, none());
    let overwrites = [
        role_overwrite(GUILD, p::EMBED_LINKS, none()),
        role_overwrite(ROLE_A, none(), p::EMBED_LINKS),
    ];
    assert!(!known(input.in_channel(&text(&overwrites))).has(p::EMBED_LINKS));
}

#[test]
fn duplicate_overwrites_for_one_target_are_merged() {
    let fx = Fixture::new();
    let overwrites = [
        member_overwrite(MEMBER, none(), p::SEND_MESSAGES),
        member_overwrite(MEMBER, p::ADD_REACTIONS, none()),
    ];
    let computed = known(fx.input(base_everyone()).in_channel(&text(&overwrites)));
    assert!(!computed.has(p::SEND_MESSAGES));
    assert!(computed.has(p::ADD_REACTIONS));
}

// ---------------------------------------------------------------------------
// Implicit permissions
// ---------------------------------------------------------------------------

#[test]
fn without_view_channel_only_guild_wide_permissions_remain() {
    let fx = Fixture::new();
    let input = fx
        .input(base_everyone() | p::ATTACH_FILES | p::MANAGE_ROLES)
        .role(
            fx.role_a,
            p::KICK_MEMBERS | p::MODERATE_MEMBERS | p::CONNECT,
        );
    let overwrites = [role_overwrite(GUILD, none(), p::VIEW_CHANNEL)];
    let computed = known(input.in_channel(&text(&overwrites)));
    assert_eq!(computed.permissions, p::KICK_MEMBERS | p::MODERATE_MEMBERS);
}

#[test]
fn without_view_channel_unknown_future_bits_are_dropped() {
    let fx = Fixture::new();
    let input = fx.input(Permissions::from_bits(1 << 60));
    let computed = known(input.in_channel(&text(&[])));
    assert!(computed.permissions.is_empty());
}

#[test]
fn text_channel_without_send_messages_drops_send_dependent_permissions() {
    let fx = Fixture::new();
    let everyone = p::VIEW_CHANNEL
        | p::SEND_MESSAGES
        | p::MENTION_EVERYONE
        | p::SEND_TTS_MESSAGES
        | p::ATTACH_FILES
        | p::EMBED_LINKS
        | p::ADD_REACTIONS
        | p::SEND_POLLS;
    let overwrites = [role_overwrite(GUILD, none(), p::SEND_MESSAGES)];
    let computed = known(fx.input(everyone).in_channel(&text(&overwrites)));
    assert_eq!(
        computed.permissions,
        p::VIEW_CHANNEL | p::ADD_REACTIONS | p::SEND_POLLS,
        "only the four documented permissions depend on SEND_MESSAGES"
    );
    assert_eq!(
        computed.raw,
        everyone - p::SEND_MESSAGES,
        "raw skips implicit rules"
    );
}

#[test]
fn voice_channel_applies_send_and_connect_rules() {
    let fx = Fixture::new();
    let everyone = p::VIEW_CHANNEL | p::ATTACH_FILES | p::MANAGE_CHANNELS | p::SPEAK;
    let channel = ChannelPermissionInput::new(ChannelClass::Voice, &[]);
    let computed = known(fx.input(everyone).in_channel(&channel));
    assert_eq!(computed.permissions, p::VIEW_CHANNEL | p::SPEAK);

    let with_connect = known(
        fx.input(everyone | p::CONNECT | p::SEND_MESSAGES)
            .in_channel(&channel),
    );
    assert!(with_connect.has(p::MANAGE_CHANNELS | p::ATTACH_FILES));
}

#[test]
fn connect_rule_only_applies_to_voice_channels() {
    let fx = Fixture::new();
    let everyone = p::VIEW_CHANNEL | p::MANAGE_CHANNELS;
    assert!(known(fx.input(everyone).in_channel(&text(&[]))).has(p::MANAGE_CHANNELS));
}

#[test]
fn categories_and_other_channels_only_apply_the_view_rule() {
    let fx = Fixture::new();
    let everyone = p::VIEW_CHANNEL | p::ATTACH_FILES | p::MANAGE_CHANNELS;
    for class in [ChannelClass::Category, ChannelClass::Other] {
        let channel = ChannelPermissionInput::new(class, &[]);
        let computed = known(fx.input(everyone).in_channel(&channel));
        assert_eq!(computed.permissions, everyone, "{class:?}");
    }
    let hidden = ChannelPermissionInput::new(ChannelClass::Category, &[]);
    assert!(known(fx.input(p::ATTACH_FILES).in_channel(&hidden))
        .permissions
        .is_empty());
}

#[test]
fn non_applicable_bits_are_kept() {
    let fx = Fixture::new();
    let computed = known(
        fx.input(base_everyone() | p::CONNECT)
            .in_channel(&text(&[])),
    );
    assert!(computed.has(p::CONNECT));
}

// ---------------------------------------------------------------------------
// Threads
// ---------------------------------------------------------------------------

#[test]
fn threads_use_parent_overwrites_and_send_messages_in_threads() {
    let fx = Fixture::new();
    let everyone = p::VIEW_CHANNEL | p::SEND_MESSAGES | p::ATTACH_FILES;
    let parent_overwrites = [role_overwrite(ROLE_A, p::SEND_MESSAGES_IN_THREADS, none())];
    let thread = ChannelPermissionInput::new(ChannelClass::Thread, &parent_overwrites);

    let computed = known(fx.input(everyone).in_channel(&thread));
    assert!(
        !computed.has(p::SEND_MESSAGES),
        "SEND_MESSAGES is not inherited"
    );
    assert!(computed.has(p::SEND_MESSAGES_IN_THREADS | p::ATTACH_FILES));
    // discord.js ThreadChannel.permissionsFor() answers from the parent.
    assert!(computed.raw.contains(p::SEND_MESSAGES.bits()));
}

#[test]
fn thread_without_send_messages_in_threads_drops_send_dependent_permissions() {
    let fx = Fixture::new();
    let everyone = p::VIEW_CHANNEL | p::SEND_MESSAGES | p::ATTACH_FILES | p::EMBED_LINKS;
    let thread = ChannelPermissionInput::new(ChannelClass::Thread, &[]);
    let computed = known(fx.input(everyone).in_channel(&thread));
    assert_eq!(computed.permissions, p::VIEW_CHANNEL);
}

#[test]
fn thread_requires_view_channel_on_parent() {
    let fx = Fixture::new();
    let parent_overwrites = [member_overwrite(MEMBER, none(), p::VIEW_CHANNEL)];
    let thread = ChannelPermissionInput::new(ChannelClass::Thread, &parent_overwrites);
    let computed = known(
        fx.input(base_everyone() | p::SEND_MESSAGES_IN_THREADS)
            .in_channel(&thread),
    );
    assert!(computed.permissions.is_empty());
}

// ---------------------------------------------------------------------------
// Timeouts
// ---------------------------------------------------------------------------

#[test]
fn active_timeout_keeps_only_view_and_history() {
    let fx = Fixture::new();
    let now = UNIX_EPOCH + Duration::from_secs(1_000);
    let input = fx
        .input(base_everyone() | p::ADD_REACTIONS)
        .role(fx.role_a, p::KICK_MEMBERS)
        .communication_disabled_until(now + Duration::from_secs(60), now);

    let guild_level = input.guild_permissions();
    assert_eq!(
        guild_level.permissions,
        p::VIEW_CHANNEL | p::READ_MESSAGE_HISTORY
    );
    assert!(guild_level.timeout_applied);
    assert!(guild_level
        .raw
        .contains(p::KICK_MEMBERS.bits() | p::ADD_REACTIONS.bits()));

    let channel = known(input.in_channel(&text(&[member_overwrite(
        MEMBER,
        p::MANAGE_MESSAGES,
        none(),
    )])));
    assert_eq!(
        channel.permissions,
        p::VIEW_CHANNEL | p::READ_MESSAGE_HISTORY
    );
    assert!(channel.timeout_applied);
}

#[test]
fn expired_timeout_is_ignored() {
    let fx = Fixture::new();
    let now = UNIX_EPOCH + Duration::from_secs(1_000);
    for until in [now, now - Duration::from_secs(1)] {
        let computed = fx
            .input(base_everyone())
            .communication_disabled_until(until, now)
            .guild_permissions();
        assert_eq!(computed.permissions, base_everyone());
        assert!(!computed.timeout_applied);
    }
}

#[test]
fn timed_out_member_without_view_has_nothing() {
    let fx = Fixture::new();
    let now = UNIX_EPOCH;
    let input = fx
        .input(base_everyone())
        .communication_disabled_until(now + Duration::from_secs(1), now);
    let overwrites = [role_overwrite(GUILD, none(), p::VIEW_CHANNEL)];
    let computed = known(input.in_channel(&text(&overwrites)));
    assert!(computed.permissions.is_empty());
    assert!(computed.timeout_applied);
}

// ---------------------------------------------------------------------------
// Obfuscated channels
// ---------------------------------------------------------------------------

#[test]
fn obfuscated_channel_is_unknown_for_regular_members() {
    let fx = Fixture::new();
    let synthetic = [role_overwrite(GUILD, none(), p::VIEW_CHANNEL)];
    let channel = text(&synthetic).obfuscated(true);

    let outcome = fx.input(base_everyone()).in_channel(&channel);
    assert_eq!(outcome, ChannelPermissions::Obfuscated);
    assert!(outcome.is_obfuscated());
    assert_eq!(outcome.known(), None);
    assert_eq!(outcome.has(p::VIEW_CHANNEL), None);
}

#[test]
fn obfuscated_channel_is_still_known_for_owner_and_admin() {
    let fx = Fixture::new();
    let channel = text(&[]).obfuscated(true);
    let owner = MemberPermissionInput::new(fx.guild, fx.owner, fx.owner, none());
    assert_eq!(
        owner.in_channel(&channel).known().map(|c| c.source),
        Some(PermissionSource::Owner)
    );
    let admin = fx.input(p::ADMINISTRATOR);
    assert_eq!(admin.in_channel(&channel).has(p::SEND_MESSAGES), Some(true));
}

#[test]
fn obfuscated_can_be_cleared_again() {
    let fx = Fixture::new();
    let channel = text(&[]).obfuscated(true).obfuscated(false);
    assert!(fx
        .input(base_everyone())
        .in_channel(&channel)
        .known()
        .is_some());
}

// ---------------------------------------------------------------------------
// Result helpers and classification
// ---------------------------------------------------------------------------

#[test]
fn computed_has_and_missing_accept_flags_and_raw_bits() {
    let fx = Fixture::new();
    let computed = fx.input(base_everyone()).guild_permissions();
    assert!(computed.has(p::VIEW_CHANNEL));
    assert!(computed.has(bits(p::SEND_MESSAGES)));
    assert!(!computed.has(p::VIEW_CHANNEL | p::MANAGE_GUILD));
    assert_eq!(
        computed.missing(p::VIEW_CHANNEL | p::MANAGE_GUILD | p::BAN_MEMBERS),
        p::MANAGE_GUILD | p::BAN_MEMBERS
    );
    assert_eq!(computed.bits(), bits(base_everyone()));
    assert_eq!(
        ChannelPermissions::Known(computed).has(p::MANAGE_GUILD),
        Some(false)
    );
    assert_eq!(ChannelPermissions::Known(computed).known(), Some(computed));
    assert!(!ChannelPermissions::Known(computed).is_obfuscated());
}

#[test]
fn channel_types_are_classified() {
    let cases = [
        (0, Some(ChannelClass::Text)),
        (1, None),
        (2, Some(ChannelClass::Voice)),
        (3, None),
        (4, Some(ChannelClass::Category)),
        (5, Some(ChannelClass::Text)),
        (10, Some(ChannelClass::Thread)),
        (11, Some(ChannelClass::Thread)),
        (12, Some(ChannelClass::Thread)),
        (13, Some(ChannelClass::Voice)),
        (14, Some(ChannelClass::Other)),
        (15, Some(ChannelClass::Text)),
        (16, Some(ChannelClass::Text)),
        (99, Some(ChannelClass::Other)),
    ];
    for (kind, expected) in cases {
        assert_eq!(
            ChannelClass::from_channel_type(kind),
            expected,
            "type {kind}"
        );
    }
}

// ---------------------------------------------------------------------------
// Model layer
// ---------------------------------------------------------------------------

fn role(id: u64, permissions: Permissions) -> Role {
    Role {
        id: sf(id),
        name: format!("role-{id}"),
        permissions: Some(PermissionsBitField(permissions.bits())),
        ..Role::fixture()
    }
}

fn guild_model() -> Guild {
    Guild {
        id: sf(GUILD),
        name: "guild".to_string(),
        owner_id: Some(sf(OWNER)),
        roles: vec![
            role(GUILD, base_everyone()),
            role(ROLE_A, p::ATTACH_FILES),
            role(ROLE_B, p::KICK_MEMBERS),
        ],
        ..Guild::fixture()
    }
}

fn user(id: u64) -> User {
    User {
        id: sf(id),
        username: format!("user-{id}"),
        ..User::fixture()
    }
}

fn member_model() -> Member {
    Member {
        user: Some(user(MEMBER)),
        roles: vec![sf(ROLE_A), sf(ROLE_B)],
        ..Member::default()
    }
}

fn channel_model(id: u64, kind: u8, overwrites: Vec<PermissionOverwrite>) -> Channel {
    Channel {
        id: sf(id),
        guild_id: Some(sf(GUILD)),
        kind,
        permission_overwrites: Some(overwrites),
        ..Channel::fixture()
    }
}

fn thread_model(id: u64, parent: u64) -> Channel {
    Channel {
        id: sf(id),
        guild_id: Some(sf(GUILD)),
        kind: PUBLIC_THREAD,
        parent_id: Some(sf(parent)),
        ..Channel::fixture()
    }
}

fn now() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_777_593_600)
}

#[test]
fn permissions_for_resolves_roles_from_the_guild() {
    let channel = channel_model(
        500,
        TEXT,
        vec![role_overwrite(ROLE_A, none(), p::SEND_MESSAGES)],
    );
    let computed =
        known(permissions_for(&guild_model(), &member_model(), &channel, None, now()).unwrap());
    assert_eq!(
        computed.permissions,
        p::VIEW_CHANNEL | p::READ_MESSAGE_HISTORY | p::KICK_MEMBERS
    );

    let guild_level = guild_permissions(&guild_model(), &member_model(), now()).unwrap();
    assert!(guild_level.has(base_everyone() | p::ATTACH_FILES | p::KICK_MEMBERS));
}

#[test]
fn permissions_for_reports_missing_roles() {
    let mut guild = guild_model();
    guild.roles.retain(|role| role.id != sf(GUILD));
    assert_eq!(
        guild_permissions(&guild, &member_model(), now()).unwrap_err(),
        PermissionDataError::MissingRole {
            guild_id: sf(GUILD),
            role_id: sf(GUILD)
        }
    );

    let mut member = member_model();
    member.roles.push(sf(ROLE_C));
    assert_eq!(
        guild_permissions(&guild_model(), &member, now()).unwrap_err(),
        PermissionDataError::MissingRole {
            guild_id: sf(GUILD),
            role_id: sf(ROLE_C)
        }
    );

    let mut guild = guild_model();
    guild.roles[1].permissions = None;
    assert_eq!(
        guild_permissions(&guild, &member_model(), now()).unwrap_err(),
        PermissionDataError::MissingRolePermissions {
            role_id: sf(ROLE_A)
        }
    );
}

#[test]
fn member_listing_everyone_role_is_tolerated() {
    let mut member = member_model();
    member.roles.push(sf(GUILD));
    assert!(guild_permissions(&guild_model(), &member, now()).is_ok());
}

#[test]
fn permissions_for_reports_missing_owner_and_member_user() {
    let mut guild = guild_model();
    guild.owner_id = None;
    assert_eq!(
        guild_permissions(&guild, &member_model(), now()).unwrap_err(),
        PermissionDataError::MissingGuildOwner {
            guild_id: sf(GUILD)
        }
    );

    let mut member = member_model();
    member.user = None;
    assert_eq!(
        guild_permissions(&guild_model(), &member, now()).unwrap_err(),
        PermissionDataError::MissingMemberUser
    );
}

#[test]
fn permissions_for_parses_the_timeout() {
    let mut member = member_model();
    member.communication_disabled_until = Some("2026-05-01T00:10:00.000000+00:00".to_string());
    let computed = guild_permissions(&guild_model(), &member, now()).unwrap();
    assert!(computed.timeout_applied);
    assert_eq!(
        computed.permissions,
        p::VIEW_CHANNEL | p::READ_MESSAGE_HISTORY
    );

    member.communication_disabled_until = Some("2026-04-30T23:59:59Z".to_string());
    assert!(
        !guild_permissions(&guild_model(), &member, now())
            .unwrap()
            .timeout_applied
    );

    member.communication_disabled_until = Some("tomorrow".to_string());
    assert_eq!(
        guild_permissions(&guild_model(), &member, now()).unwrap_err(),
        PermissionDataError::InvalidTimestamp {
            value: "tomorrow".to_string()
        }
    );
}

#[test]
fn owner_model_gets_everything() {
    let mut member = member_model();
    member.user = Some(user(OWNER));
    let computed = guild_permissions(&guild_model(), &member, now()).unwrap();
    assert_eq!(computed.source, PermissionSource::Owner);
}

#[test]
fn from_channel_rejects_dms_and_foreign_channels() {
    let guild = sf(GUILD);
    let dm = channel_model(500, 1, vec![]);
    assert_eq!(
        ChannelPermissionInput::from_channel(guild, &dm, None).unwrap_err(),
        PermissionDataError::NotGuildChannel {
            channel_id: sf(500),
            kind: 1
        }
    );

    let mut foreign = channel_model(501, TEXT, vec![]);
    foreign.guild_id = Some(sf(999));
    assert_eq!(
        ChannelPermissionInput::from_channel(guild, &foreign, None).unwrap_err(),
        PermissionDataError::GuildMismatch {
            channel_id: sf(501),
            expected: sf(GUILD),
            found: sf(999)
        }
    );

    // GUILD_CREATE channels carry no guild_id.
    let mut bare = channel_model(502, TEXT, vec![]);
    bare.guild_id = None;
    assert!(ChannelPermissionInput::from_channel(guild, &bare, None).is_ok());
}

#[test]
fn from_channel_requires_overwrites_unless_obfuscated() {
    let guild = sf(GUILD);
    let mut partial = channel_model(500, TEXT, vec![]);
    partial.permission_overwrites = None;
    assert_eq!(
        ChannelPermissionInput::from_channel(guild, &partial, None).unwrap_err(),
        PermissionDataError::MissingPermissionOverwrites {
            channel_id: sf(500)
        }
    );

    partial.flags = Some(1 << 17);
    let input = ChannelPermissionInput::from_channel(guild, &partial, None).unwrap();
    assert_eq!(known_or_obfuscated(&input), ChannelPermissions::Obfuscated);
}

fn known_or_obfuscated(input: &ChannelPermissionInput<'_>) -> ChannelPermissions {
    let fx = Fixture::new();
    fx.input(base_everyone()).in_channel(input)
}

#[test]
fn from_channel_ignores_parent_for_non_threads() {
    let guild = sf(GUILD);
    let category = channel_model(
        400,
        CATEGORY,
        vec![role_overwrite(GUILD, none(), p::VIEW_CHANNEL)],
    );
    let channel = channel_model(500, TEXT, vec![]);
    let input = ChannelPermissionInput::from_channel(guild, &channel, Some(&category)).unwrap();
    assert!(known(known_or_obfuscated(&input)).has(p::VIEW_CHANNEL));
}

#[test]
fn from_channel_validates_thread_parents() {
    let guild = sf(GUILD);
    let thread = thread_model(600, 500);

    assert_eq!(
        ChannelPermissionInput::from_channel(guild, &thread, None).unwrap_err(),
        PermissionDataError::MissingParentChannel {
            thread_id: sf(600),
            parent_id: Some(sf(500))
        }
    );

    let wrong = channel_model(501, TEXT, vec![]);
    assert_eq!(
        ChannelPermissionInput::from_channel(guild, &thread, Some(&wrong)).unwrap_err(),
        PermissionDataError::ParentMismatch {
            thread_id: sf(600),
            expected: sf(500),
            found: sf(501)
        }
    );

    let nested = Channel {
        id: sf(500),
        ..thread_model(500, 400)
    };
    assert_eq!(
        ChannelPermissionInput::from_channel(guild, &thread, Some(&nested)).unwrap_err(),
        PermissionDataError::InvalidParentChannel {
            thread_id: sf(600),
            parent_id: sf(500),
            kind: PUBLIC_THREAD
        }
    );

    let dm_parent = channel_model(500, 1, vec![]);
    assert!(matches!(
        ChannelPermissionInput::from_channel(guild, &thread, Some(&dm_parent)),
        Err(PermissionDataError::NotGuildChannel { .. })
    ));

    let mut partial_parent = channel_model(500, TEXT, vec![]);
    partial_parent.permission_overwrites = None;
    assert_eq!(
        ChannelPermissionInput::from_channel(guild, &thread, Some(&partial_parent)).unwrap_err(),
        PermissionDataError::MissingPermissionOverwrites {
            channel_id: sf(500)
        }
    );
}

#[test]
fn thread_without_parent_id_accepts_the_supplied_parent() {
    let guild = sf(GUILD);
    let mut thread = thread_model(600, 500);
    thread.parent_id = None;
    assert_eq!(
        ChannelPermissionInput::from_channel(guild, &thread, None).unwrap_err(),
        PermissionDataError::MissingParentChannel {
            thread_id: sf(600),
            parent_id: None
        }
    );
    let parent = channel_model(500, TEXT, vec![]);
    assert!(ChannelPermissionInput::from_channel(guild, &thread, Some(&parent)).is_ok());
}

#[test]
fn permissions_for_thread_uses_parent_and_obfuscation() {
    let parent = channel_model(
        500,
        TEXT,
        vec![role_overwrite(ROLE_A, p::SEND_MESSAGES_IN_THREADS, none())],
    );
    let thread = thread_model(600, 500);
    let computed = known(
        permissions_for(
            &guild_model(),
            &member_model(),
            &thread,
            Some(&parent),
            now(),
        )
        .unwrap(),
    );
    assert!(computed.has(p::SEND_MESSAGES_IN_THREADS | p::ATTACH_FILES));
    assert!(!computed.has(p::SEND_MESSAGES));

    let mut hidden_parent = parent.clone();
    hidden_parent.flags = Some(1 << 17);
    assert!(permissions_for(
        &guild_model(),
        &member_model(),
        &thread,
        Some(&hidden_parent),
        now()
    )
    .unwrap()
    .is_obfuscated());

    let mut hidden_thread = thread.clone();
    hidden_thread.flags = Some(1 << 17);
    assert!(permissions_for(
        &guild_model(),
        &member_model(),
        &hidden_thread,
        Some(&parent),
        now()
    )
    .unwrap()
    .is_obfuscated());
}

#[test]
fn gateway_obfuscated_channel_payload_is_reported_as_obfuscated() {
    // Shape Discord sends from 2026-11-16 for channels the bot cannot view:
    // placeholder name and a single synthetic @everyone VIEW_CHANNEL deny.
    let channel: Channel = serde_json::from_value(serde_json::json!({
        "id": "500",
        "type": 0,
        "guild_id": "100",
        "name": "hidden-channel",
        "position": 3,
        "flags": 1 << 17,
        "permission_overwrites": [
            { "id": "100", "type": 0, "allow": "0", "deny": "1024" }
        ]
    }))
    .unwrap();
    let outcome = permissions_for(&guild_model(), &member_model(), &channel, None, now()).unwrap();
    assert_eq!(outcome, ChannelPermissions::Obfuscated);

    // Without the flag the same overwrite is a real deny.
    let mut visible = channel.clone();
    visible.flags = None;
    let computed =
        known(permissions_for(&guild_model(), &member_model(), &visible, None, now()).unwrap());
    assert_eq!(computed.permissions, p::KICK_MEMBERS);
}

#[test]
fn errors_render_the_missing_object() {
    let cases = [
        (
            PermissionDataError::CacheDisabled,
            "the `cache` feature is disabled",
        ),
        (
            PermissionDataError::MissingGuild { guild_id: sf(1) },
            "guild 1 is not cached",
        ),
        (
            PermissionDataError::MissingGuildOwner { guild_id: sf(1) },
            "guild 1 has no owner_id",
        ),
        (
            PermissionDataError::MissingRole {
                guild_id: sf(1),
                role_id: sf(1),
            },
            "@everyone role of guild 1 is not available",
        ),
        (
            PermissionDataError::MissingRole {
                guild_id: sf(1),
                role_id: sf(2),
            },
            "role 2 of guild 1 is not available",
        ),
        (
            PermissionDataError::MissingRolePermissions { role_id: sf(2) },
            "role 2 has no permissions field",
        ),
        (
            PermissionDataError::MissingMember {
                guild_id: sf(1),
                user_id: sf(2),
            },
            "member 2 of guild 1 is not cached",
        ),
        (
            PermissionDataError::MissingMemberUser,
            "member object has no user",
        ),
        (
            PermissionDataError::MissingChannel { channel_id: sf(5) },
            "channel 5 is not cached",
        ),
        (
            PermissionDataError::MissingParentChannel {
                thread_id: sf(6),
                parent_id: Some(sf(5)),
            },
            "parent channel 5 of thread 6 is not available",
        ),
        (
            PermissionDataError::MissingParentChannel {
                thread_id: sf(6),
                parent_id: None,
            },
            "thread 6 has no parent channel",
        ),
        (
            PermissionDataError::MissingPermissionOverwrites { channel_id: sf(5) },
            "channel 5 has no permission_overwrites",
        ),
        (
            PermissionDataError::NotGuildChannel {
                channel_id: sf(5),
                kind: 1,
            },
            "channel 5 (type 1) is not a guild channel",
        ),
        (
            PermissionDataError::GuildMismatch {
                channel_id: sf(5),
                expected: sf(1),
                found: sf(2),
            },
            "channel 5 belongs to guild 2, not 1",
        ),
        (
            PermissionDataError::ParentMismatch {
                thread_id: sf(6),
                expected: sf(5),
                found: sf(7),
            },
            "thread 6 has parent 5, but channel 7 was supplied",
        ),
        (
            PermissionDataError::InvalidParentChannel {
                thread_id: sf(6),
                parent_id: sf(5),
                kind: 11,
            },
            "channel 5 (type 11) cannot be the parent of thread 6",
        ),
        (
            PermissionDataError::InvalidTimestamp {
                value: "x".to_string(),
            },
            "invalid communication_disabled_until timestamp \"x\"",
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(error.to_string(), expected);
        let as_error: &dyn std::error::Error = &error;
        assert!(as_error.source().is_none());
        match DiscordError::from(error) {
            DiscordError::Model { message } => assert_eq!(message, expected),
            other => panic!("unexpected conversion {other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Timestamp parsing
// ---------------------------------------------------------------------------

fn at(seconds: i64) -> SystemTime {
    if seconds >= 0 {
        UNIX_EPOCH + Duration::from_secs(seconds as u64)
    } else {
        UNIX_EPOCH - Duration::from_secs(seconds.unsigned_abs())
    }
}

#[test]
fn parses_discord_timestamp_shapes() {
    let cases = [
        ("2026-05-01T00:00:00+00:00", at(1_777_593_600)),
        ("2026-05-01T00:00:00Z", at(1_777_593_600)),
        ("2026-05-01t00:00:00z", at(1_777_593_600)),
        ("2026-05-01 00:00:00+0000", at(1_777_593_600)),
        ("2026-05-01T09:00:00+09:00", at(1_777_593_600)),
        ("2026-04-30T18:30:00-05:30", at(1_777_593_600)),
        ("2021-04-20T21:20:30+00:00", at(1_618_953_630)),
        ("2000-02-29T12:00:00Z", at(951_825_600)),
        ("1969-12-31T23:59:59+00:00", at(-1)),
        ("2016-12-31T23:59:60Z", at(1_483_228_800)),
        (
            "2026-05-01T00:00:00.123456+00:00",
            at(1_777_593_600) + Duration::from_micros(123_456),
        ),
        (
            "2026-05-01T00:00:00.1234567891Z",
            at(1_777_593_600) + Duration::from_nanos(123_456_789),
        ),
    ];
    for (value, expected) in cases {
        assert_eq!(parse_timestamp(value), Some(expected), "{value}");
    }
}

#[test]
fn rejects_malformed_timestamps() {
    for value in [
        "",
        "tomorrow",
        "2026-05-01",
        "2026-05-01T00:00:00",
        "2026-13-01T00:00:00Z",
        "2026-00-01T00:00:00Z",
        "2026-02-29T00:00:00Z",
        "1900-02-29T00:00:00Z",
        "2026-04-31T00:00:00Z",
        "2026-05-00T00:00:00Z",
        "2026-05-01T24:00:00Z",
        "2026-05-01T00:60:00Z",
        "2026-05-01T00:00:61Z",
        "2026-05-01T00:00:00.Z",
        "2026-05-01T00:00:00ZZ",
        "2026-05-01T00:00:00+24:00",
        "2026-05-01T00:00:00+00:60",
        "2026-05-01T00:00:00+0",
        "2026-05-01T00:00:00+00-00",
        "2026-05-01T00:00:00 00:00",
        "2026/05/01T00:00:00Z",
        "2026-05-01X00:00:00Z",
        "20a6-05-01T00:00:00Z",
        "2026-05-01T00:00:00+0a:00",
    ] {
        assert_eq!(parse_timestamp(value), None, "{value}");
    }
}

#[test]
fn leap_years_follow_the_gregorian_rule() {
    assert!(parse_timestamp("2024-02-29T00:00:00Z").is_some());
    assert!(parse_timestamp("2000-02-29T00:00:00Z").is_some());
    assert!(parse_timestamp("2100-02-29T00:00:00Z").is_none());
    assert!(parse_timestamp("2026-11-30T00:00:00Z").is_some());
    assert!(parse_timestamp("2026-12-31T00:00:00Z").is_some());
}

// ---------------------------------------------------------------------------
// Cache adapter
// ---------------------------------------------------------------------------

#[cfg(feature = "cache")]
async fn populated_cache() -> CacheHandle {
    let cache = CacheHandle::new();
    let mut guild = guild_model();
    guild.channels = vec![
        channel_model(
            500,
            TEXT,
            vec![role_overwrite(ROLE_A, p::SEND_MESSAGES_IN_THREADS, none())],
        ),
        channel_model(
            501,
            TEXT,
            vec![member_overwrite(MEMBER, none(), p::SEND_MESSAGES)],
        ),
    ];
    guild.threads = vec![thread_model(600, 500)];
    guild.members = vec![member_model()];
    cache.apply_guild_create(&guild).await;
    cache
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn cached_permissions_for_reads_guild_member_channel_and_roles() {
    let cache = populated_cache().await;
    let computed = known(
        cached_permissions_for(&cache, sf(GUILD), sf(MEMBER), sf(501), now())
            .await
            .unwrap(),
    );
    assert_eq!(
        computed.permissions,
        p::VIEW_CHANNEL | p::READ_MESSAGE_HISTORY | p::KICK_MEMBERS
    );

    let thread = known(
        cached_permissions_for(&cache, sf(GUILD), sf(MEMBER), sf(600), now())
            .await
            .unwrap(),
    );
    assert!(thread.has(p::SEND_MESSAGES_IN_THREADS | p::ATTACH_FILES));

    let guild_level = cached_guild_permissions(&cache, sf(GUILD), sf(MEMBER), now())
        .await
        .unwrap();
    assert!(guild_level.has(p::ATTACH_FILES | p::KICK_MEMBERS | p::SEND_MESSAGES));
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn cached_permissions_follow_role_updates_not_the_guild_snapshot() {
    let cache = populated_cache().await;
    cache
        .upsert_role(sf(GUILD), role(ROLE_B, p::BAN_MEMBERS))
        .await;
    let computed = cached_guild_permissions(&cache, sf(GUILD), sf(MEMBER), now())
        .await
        .unwrap();
    assert!(computed.has(p::BAN_MEMBERS));
    assert!(!computed.has(p::KICK_MEMBERS));
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn cached_permissions_report_each_missing_piece() {
    let cache = populated_cache().await;
    let guild = sf(GUILD);
    let member = sf(MEMBER);

    assert_eq!(
        cached_guild_permissions(&cache, sf(999), member, now())
            .await
            .unwrap_err(),
        PermissionDataError::MissingGuild { guild_id: sf(999) }
    );
    // Channel 500 belongs to guild 100: a contradiction, reported first.
    assert_eq!(
        cached_permissions_for(&cache, sf(999), member, sf(500), now())
            .await
            .unwrap_err(),
        PermissionDataError::GuildMismatch {
            channel_id: sf(500),
            expected: sf(999),
            found: guild
        }
    );
    assert_eq!(
        cached_permissions_for(&cache, guild, sf(OTHER_MEMBER), sf(500), now())
            .await
            .unwrap_err(),
        PermissionDataError::MissingMember {
            guild_id: guild,
            user_id: sf(OTHER_MEMBER)
        }
    );
    assert_eq!(
        cached_permissions_for(&cache, guild, member, sf(777), now())
            .await
            .unwrap_err(),
        PermissionDataError::MissingChannel {
            channel_id: sf(777)
        }
    );

    cache.upsert_channel(thread_model(601, 555)).await;
    assert_eq!(
        cached_permissions_for(&cache, guild, member, sf(601), now())
            .await
            .unwrap_err(),
        PermissionDataError::MissingParentChannel {
            thread_id: sf(601),
            parent_id: Some(sf(555))
        }
    );

    let mut orphan = thread_model(602, 555);
    orphan.parent_id = None;
    cache.upsert_channel(orphan).await;
    assert_eq!(
        cached_permissions_for(&cache, guild, member, sf(602), now())
            .await
            .unwrap_err(),
        PermissionDataError::MissingParentChannel {
            thread_id: sf(602),
            parent_id: None
        }
    );

    cache.remove_role(guild, sf(ROLE_B)).await;
    assert_eq!(
        cached_guild_permissions(&cache, guild, member, now())
            .await
            .unwrap_err(),
        PermissionDataError::MissingRole {
            guild_id: guild,
            role_id: sf(ROLE_B)
        }
    );

    cache.remove_role(guild, guild).await;
    assert_eq!(
        cached_guild_permissions(&cache, guild, member, now())
            .await
            .unwrap_err(),
        PermissionDataError::MissingRole {
            guild_id: guild,
            role_id: guild
        }
    );
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn cached_permissions_report_missing_owner_and_accept_userless_members() {
    let cache = populated_cache().await;
    let mut member = member_model();
    member.user = None;
    cache
        .upsert_member(sf(GUILD), sf(OTHER_MEMBER), member)
        .await;
    let computed = cached_guild_permissions(&cache, sf(GUILD), sf(OTHER_MEMBER), now())
        .await
        .unwrap();
    assert!(computed.has(p::ATTACH_FILES));

    let mut guild = guild_model();
    guild.owner_id = None;
    cache.upsert_guild(guild).await;
    assert_eq!(
        cached_guild_permissions(&cache, sf(GUILD), sf(MEMBER), now())
            .await
            .unwrap_err(),
        PermissionDataError::MissingGuildOwner {
            guild_id: sf(GUILD)
        }
    );
}

#[cfg(not(feature = "cache"))]
#[tokio::test]
async fn cached_permissions_need_the_cache_feature() {
    let cache = CacheHandle::new();
    assert_eq!(
        cached_permissions_for(&cache, &sf(GUILD), &sf(MEMBER), &sf(500), now())
            .await
            .unwrap_err(),
        PermissionDataError::CacheDisabled
    );
    assert_eq!(
        cached_guild_permissions(&cache, &sf(GUILD), &sf(MEMBER), now())
            .await
            .unwrap_err(),
        PermissionDataError::CacheDisabled
    );
}

// ---------------------------------------------------------------------------
// model_ext delegates
// ---------------------------------------------------------------------------

#[test]
fn channel_and_member_delegates_compute_permissions() {
    let voice = channel_model(
        700,
        VOICE,
        vec![role_overwrite(GUILD, p::MANAGE_CHANNELS, none())],
    );
    let computed = known(
        voice
            .permissions_for(&guild_model(), &member_model(), None)
            .unwrap(),
    );
    // No CONNECT in this voice channel, so MANAGE_CHANNELS is implicitly
    // denied even though the overwrite allows it.
    assert!(!computed.has(p::MANAGE_CHANNELS));
    assert!(computed.has(p::VIEW_CHANNEL | p::KICK_MEMBERS));

    let guild_level = member_model().guild_permissions(&guild_model()).unwrap();
    assert!(guild_level.has(p::ATTACH_FILES));

    let mut userless = member_model();
    userless.user = None;
    assert_eq!(
        userless.guild_permissions(&guild_model()).unwrap_err(),
        PermissionDataError::MissingMemberUser
    );
}

// ---------------------------------------------------------------------------
// Owners and administrators need only the data that proves their status
// ---------------------------------------------------------------------------

fn owner_member() -> Member {
    Member {
        user: Some(user(OWNER)),
        roles: vec![sf(ROLE_A), sf(ROLE_C)],
        ..Member::default()
    }
}

#[test]
fn known_owner_needs_no_roles_overwrites_parent_or_timeout() {
    let mut guild = guild_model();
    guild.roles.clear();
    let mut member = owner_member();
    member.communication_disabled_until = Some("not a timestamp".to_string());

    let guild_level = guild_permissions(&guild, &member, now()).unwrap();
    assert_eq!(guild_level.source, PermissionSource::Owner);

    let mut partial = channel_model(500, TEXT, vec![]);
    partial.permission_overwrites = None;
    let outcome = permissions_for(&guild, &member, &partial, None, now()).unwrap();
    assert_eq!(
        outcome.known().map(|c| c.source),
        Some(PermissionSource::Owner)
    );

    let thread = thread_model(600, 500);
    let outcome = permissions_for(&guild, &member, &thread, None, now()).unwrap();
    assert_eq!(
        outcome.known().map(|c| c.source),
        Some(PermissionSource::Owner)
    );
}

#[test]
fn owner_still_gets_contradiction_errors() {
    let member = owner_member();
    let mut foreign = channel_model(500, TEXT, vec![]);
    foreign.guild_id = Some(sf(999));
    assert!(matches!(
        permissions_for(&guild_model(), &member, &foreign, None, now()),
        Err(PermissionDataError::GuildMismatch { .. })
    ));

    let dm = channel_model(501, 1, vec![]);
    assert!(matches!(
        permissions_for(&guild_model(), &member, &dm, None, now()),
        Err(PermissionDataError::NotGuildChannel { .. })
    ));

    let thread = thread_model(600, 500);
    let wrong_parent = channel_model(502, TEXT, vec![]);
    assert!(matches!(
        permissions_for(&guild_model(), &member, &thread, Some(&wrong_parent), now()),
        Err(PermissionDataError::ParentMismatch { .. })
    ));
    let voice_parent = channel_model(500, VOICE, vec![]);
    assert!(matches!(
        permissions_for(&guild_model(), &member, &thread, Some(&voice_parent), now()),
        Err(PermissionDataError::InvalidParentChannel { .. })
    ));
}

#[test]
fn administrator_on_everyone_skips_missing_member_roles() {
    let mut guild = guild_model();
    guild.roles[0] = role(GUILD, p::ADMINISTRATOR);
    let mut member = member_model();
    member.roles.push(sf(ROLE_C)); // not in guild.roles
    member.communication_disabled_until = Some("garbage".to_string());

    let computed = guild_permissions(&guild, &member, now()).unwrap();
    assert_eq!(computed.source, PermissionSource::Administrator);

    let mut partial = channel_model(500, TEXT, vec![]);
    partial.permission_overwrites = None;
    assert_eq!(
        permissions_for(&guild, &member, &partial, None, now())
            .unwrap()
            .known()
            .map(|c| c.source),
        Some(PermissionSource::Administrator)
    );
}

#[test]
fn administrator_role_skips_other_missing_data() {
    let mut guild = guild_model();
    guild.roles.retain(|role| role.id != sf(GUILD));
    guild.roles[0] = role(ROLE_A, p::ADMINISTRATOR);
    guild.owner_id = None;
    let mut member = member_model();
    member.user = None;
    member.roles.push(sf(ROLE_C));
    let computed = guild_permissions(&guild, &member, now()).unwrap();
    assert_eq!(computed.source, PermissionSource::Administrator);
}

#[test]
fn without_admin_the_first_missing_piece_is_reported() {
    let mut guild = guild_model();
    guild.owner_id = None;
    let mut member = member_model();
    member.user = None;
    assert_eq!(
        guild_permissions(&guild, &member, now()).unwrap_err(),
        PermissionDataError::MissingMemberUser
    );
    member.user = Some(user(MEMBER));
    assert_eq!(
        guild_permissions(&guild, &member, now()).unwrap_err(),
        PermissionDataError::MissingGuildOwner {
            guild_id: sf(GUILD)
        }
    );
}

#[test]
fn thread_parents_must_be_able_to_hold_threads() {
    let guild = sf(GUILD);
    let thread = thread_model(600, 500);
    for kind in [VOICE, 13, CATEGORY, 14] {
        let parent = channel_model(500, kind, vec![]);
        assert_eq!(
            ChannelPermissionInput::from_channel(guild, &thread, Some(&parent)).unwrap_err(),
            PermissionDataError::InvalidParentChannel {
                thread_id: sf(600),
                parent_id: sf(500),
                kind
            },
            "parent type {kind}"
        );
    }
    // Text, announcement, forum, media, and types this crate does not know.
    for kind in [TEXT, 5, 15, 16, 99] {
        let parent = channel_model(500, kind, vec![]);
        assert!(
            ChannelPermissionInput::from_channel(guild, &thread, Some(&parent)).is_ok(),
            "parent type {kind}"
        );
    }
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn cached_owner_needs_no_member_roles_or_overwrites() {
    let cache = CacheHandle::new();
    cache
        .upsert_guild(Guild {
            roles: Vec::new(),
            ..guild_model()
        })
        .await;
    let mut partial = channel_model(500, TEXT, vec![]);
    partial.permission_overwrites = None;
    cache.upsert_channel(partial).await;
    cache.upsert_channel(thread_model(600, 555)).await;

    let guild_level = cached_guild_permissions(&cache, sf(GUILD), sf(OWNER), now())
        .await
        .unwrap();
    assert_eq!(guild_level.source, PermissionSource::Owner);
    for channel in [500, 600] {
        let outcome = cached_permissions_for(&cache, sf(GUILD), sf(OWNER), sf(channel), now())
            .await
            .unwrap();
        assert_eq!(
            outcome.known().map(|c| c.source),
            Some(PermissionSource::Owner)
        );
    }
    // The channel itself is still required to check it belongs to the guild.
    assert_eq!(
        cached_permissions_for(&cache, sf(GUILD), sf(OWNER), sf(777), now())
            .await
            .unwrap_err(),
        PermissionDataError::MissingChannel {
            channel_id: sf(777)
        }
    );
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn cached_administrator_skips_missing_member_and_roles() {
    let cache = CacheHandle::new();
    cache
        .upsert_role(sf(GUILD), role(GUILD, p::ADMINISTRATOR))
        .await;
    // No guild object, no member: @everyone alone proves ADMINISTRATOR.
    let computed = cached_guild_permissions(&cache, sf(GUILD), sf(MEMBER), now())
        .await
        .unwrap();
    assert_eq!(computed.source, PermissionSource::Administrator);

    let cache = populated_cache().await;
    cache
        .upsert_role(sf(GUILD), role(ROLE_A, p::ADMINISTRATOR))
        .await;
    cache.remove_role(sf(GUILD), sf(ROLE_B)).await;
    let computed = cached_guild_permissions(&cache, sf(GUILD), sf(MEMBER), now())
        .await
        .unwrap();
    assert_eq!(computed.source, PermissionSource::Administrator);
}
