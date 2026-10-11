use std::future::Future;

use crate::error::DiscordError;
use crate::model::{DiscordModel, Id};

/// Trait for cache-aware entity managers that can read from in-memory state and fall back to HTTP.
///
/// Parallels discord.js's `CachedManager` pattern. The ID parameter is the
/// model's own ID type (`Id<T::Marker>`, e.g. [`GuildId`](crate::GuildId)
/// for a `CachedManager<Guild>`), so a manager cannot be asked for the
/// wrong kind of ID.
///
/// The methods return `Send` futures, so generic code can `tokio::spawn`
/// them; implement them as plain `async fn` (no attribute macro) that hold
/// no non-`Send` value across an `.await`. The trait is the same with and
/// without the `gateway` feature.
// [Decision Log]
// - 목적과 의도: 매니저가 모델 종류에 맞는 ID만 받게 하고, 기능 플래그에 따라 트레이트 모양이
//   바뀌지 않게 한다(설계 §1 "매니저 트레이트의 동기/비동기 차이를 없앤다").
// - 기존 구현 및 제약 조건: gateway 기능이 있으면 Send future를 돌려주는 비동기 트레이트,
//   없으면 동기 트레이트였고 둘 다 GenericId를 받았다. 비-gateway 빌드에는 구현이 없었다.
// - 검토한 주요 대안: (1) 트레이트에 ID 타입 매개변수 추가, (2) DiscordModel의 연관 마커 사용.
// - 선택한 방식: (2). `T: DiscordModel`로 묶고 `Id<T::Marker>`를 받는다. 기능과 무관하게
//   비동기 하나만 둔다(future 자체는 런타임에 의존하지 않는다).
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 `CachedManager<Guild, ChannelId>` 같은 잘못된
//   조합을 허용한다. 비동기 하나로 통일하면 기능을 켜고 끌 때 사용자 코드가 바뀌지 않는다.
// - 장점, 단점 및 영향: 파괴적 변경 — `T`가 DiscordModel이어야 하고, 비-gateway 빌드의 동기
//   트레이트가 사라졌다. 기본 매니저 구현은 이제 모든 빌드에서 쓸 수 있다.
pub trait CachedManager<T: DiscordModel + Clone>: Send + Sync {
    /// Fetch from cache first, falling back to HTTP API.
    fn get(&self, id: Id<T::Marker>) -> impl Future<Output = Result<T, DiscordError>> + Send;

    /// Get from cache only (no HTTP fallback).
    fn cached(&self, id: Id<T::Marker>) -> impl Future<Output = Option<T>> + Send;

    /// Check if the entity exists in cache.
    fn contains(&self, id: Id<T::Marker>) -> impl Future<Output = bool> + Send;

    /// List all cached entries.
    fn list_cached(&self) -> impl Future<Output = Vec<T>> + Send;
}
