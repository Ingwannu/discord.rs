//! Marker types that tell [`Id`](super::Id) values apart at compile time.
//!
//! Each marker names one kind of Discord resource. Markers are uninhabited
//! enums because they exist only as type parameters; no marker value can
//! ever be constructed. `Id<M>` holds them as `PhantomData<fn() -> M>`, so
//! the marker adds no bytes and imposes no `Send`/`Sync`/`Copy` bounds.
//!
//! There is deliberately no `Member`, `Presence`, or `Thread` marker: members
//! and presences are keyed by `(GuildId, UserId)`, and threads are channels
//! (`ChannelId`).

// [Decision Log]
// - 목적과 의도: 길드 ID 자리에 채널 ID를 넘기는 실수를 컴파일 시점에 막는다.
// - 기존 구현 및 제약 조건: 2.x의 Snowflake는 종류 구분이 없는 String이다.
// - 검토한 주요 대안: (1) 리소스마다 별도 newtype 구조체, (2) 공개 마커 트레이트,
//   (3) 봉인된 마커 트레이트 + 제네릭 Id<M>.
// - 선택한 방식: (3). 마커는 빈 enum, IdMarker는 봉인한다.
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 메서드·트레이트 구현을 30여 번 복제한다.
//   (2)는 사용자가 마커를 추가할 수 있어 "Discord 리소스 종류"라는 닫힌 집합이 깨지고,
//   3.x에서 IdMarker에 항목을 더하는 것이 호환성 파괴가 된다.
// - 장점, 단점 및 영향: 새 리소스 종류는 이 파일에 마커 한 줄과 별칭 한 줄을 더하면 된다.
//   사용자 정의 마커는 만들 수 없다(필요하면 GenericId를 쓴다).

mod private {
    /// Seals [`IdMarker`](super::IdMarker) and carries the per-marker
    /// metadata that should not become public API.
    pub trait Sealed {
        /// Name printed by `Debug`, e.g. `GuildId` so that `{:?}` reads
        /// `GuildId(123)` and matches the alias users write.
        const DEBUG_NAME: &'static str;
    }
}

/// A Discord resource kind that can parameterize [`Id`](super::Id).
///
/// This trait is sealed: the set of markers is defined by discord.rs and
/// mirrors Discord's resource types. Generic code can still be written over
/// any marker (`fn log<M: IdMarker>(id: Id<M>)`).
pub trait IdMarker: private::Sealed + 'static {}

macro_rules! id_markers {
    ($($(#[$meta:meta])* $marker:ident => $alias:literal;)+) => {
        $(
            $(#[$meta])*
            #[derive(Debug)]
            pub enum $marker {}

            impl private::Sealed for $marker {
                const DEBUG_NAME: &'static str = $alias;
            }

            impl IdMarker for $marker {}
        )+
    };
}

id_markers! {
    /// Marker for guild IDs ([`GuildId`](super::GuildId)).
    Guild => "GuildId";
    /// Marker for channel and thread IDs ([`ChannelId`](super::ChannelId)).
    Channel => "ChannelId";
    /// Marker for user IDs ([`UserId`](super::UserId)); members and
    /// presences are addressed by `(GuildId, UserId)`.
    User => "UserId";
    /// Marker for role IDs ([`RoleId`](super::RoleId)).
    Role => "RoleId";
    /// Marker for message IDs ([`MessageId`](super::MessageId)).
    Message => "MessageId";
    /// Marker for application IDs ([`ApplicationId`](super::ApplicationId)).
    Application => "ApplicationId";
    /// Marker for application command IDs ([`CommandId`](super::CommandId)).
    Command => "CommandId";
    /// Marker for application command version IDs
    /// ([`CommandVersionId`](super::CommandVersionId)).
    CommandVersion => "CommandVersionId";
    /// Marker for interaction IDs ([`InteractionId`](super::InteractionId)).
    Interaction => "InteractionId";
    /// Marker for webhook IDs ([`WebhookId`](super::WebhookId)).
    Webhook => "WebhookId";
    /// Marker for custom emoji IDs ([`EmojiId`](super::EmojiId)).
    Emoji => "EmojiId";
    /// Marker for stored attachment IDs ([`AttachmentId`](super::AttachmentId)).
    /// Upload indexes in request bodies use [`UploadId`](super::UploadId).
    Attachment => "AttachmentId";
    /// Marker for sticker IDs ([`StickerId`](super::StickerId)).
    Sticker => "StickerId";
    /// Marker for sticker pack IDs ([`StickerPackId`](super::StickerPackId)).
    StickerPack => "StickerPackId";
    /// Marker for soundboard sound IDs
    /// ([`SoundboardSoundId`](super::SoundboardSoundId)).
    SoundboardSound => "SoundboardSoundId";
    /// Marker for guild scheduled event IDs
    /// ([`ScheduledEventId`](super::ScheduledEventId)).
    ScheduledEvent => "ScheduledEventId";
    /// Marker for stage instance IDs ([`StageInstanceId`](super::StageInstanceId)).
    StageInstance => "StageInstanceId";
    /// Marker for guild integration IDs ([`IntegrationId`](super::IntegrationId)).
    Integration => "IntegrationId";
    /// Marker for audit log entry IDs ([`AuditLogEntryId`](super::AuditLogEntryId)).
    AuditLogEntry => "AuditLogEntryId";
    /// Marker for auto moderation rule IDs
    /// ([`AutoModerationRuleId`](super::AutoModerationRuleId)).
    AutoModerationRule => "AutoModerationRuleId";
    /// Marker for forum tag IDs ([`ForumTagId`](super::ForumTagId)).
    ForumTag => "ForumTagId";
    /// Marker for SKU IDs ([`SkuId`](super::SkuId)).
    Sku => "SkuId";
    /// Marker for entitlement IDs ([`EntitlementId`](super::EntitlementId)).
    Entitlement => "EntitlementId";
    /// Marker for subscription IDs ([`SubscriptionId`](super::SubscriptionId)).
    Subscription => "SubscriptionId";
    /// Marker for promotion IDs ([`PromotionId`](super::PromotionId)).
    Promotion => "PromotionId";
    /// Marker for role subscription listing IDs
    /// ([`RoleSubscriptionListingId`](super::RoleSubscriptionListingId)).
    RoleSubscriptionListing => "RoleSubscriptionListingId";
    /// Marker for Social SDK lobby IDs ([`LobbyId`](super::LobbyId)).
    Lobby => "LobbyId";
    /// Marker for activity launch IDs ([`ActivityLaunchId`](super::ActivityLaunchId)).
    ActivityLaunch => "ActivityLaunchId";
    /// Marker for developer team IDs ([`TeamId`](super::TeamId)).
    Team => "TeamId";
    /// Marker for guild onboarding prompt IDs
    /// ([`OnboardingPromptId`](super::OnboardingPromptId)).
    OnboardingPrompt => "OnboardingPromptId";
    /// Marker for guild onboarding prompt option IDs
    /// ([`OnboardingOptionId`](super::OnboardingOptionId)).
    OnboardingOption => "OnboardingOptionId";
    /// Marker for IDs whose resource kind is not statically known
    /// ([`GenericId`](super::GenericId)), such as an audit log `target_id`.
    Generic => "GenericId";
}
