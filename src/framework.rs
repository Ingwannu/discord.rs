use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use async_trait::async_trait;
use serde_json::json;

use crate::{
    interactions::{InteractionResponse, TypedInteractionHandler},
    model::{Interaction, InteractionContextData, Snowflake},
};

/// Boxed future returned by application framework route handlers.
pub type FrameworkFuture = Pin<Box<dyn Future<Output = InteractionResponse> + Send>>;

/// Route key used by the application framework.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum RouteKey {
    /// Slash, user-context, message-context, or autocomplete command name.
    Command(String),
    /// Message component `custom_id`.
    Component(String),
    /// Modal submit `custom_id`.
    Modal(String),
    /// Route could not be derived from the interaction payload.
    Unknown,
}

/// Owned context passed to a matched application route.
#[derive(Clone, Debug)]
pub struct AppContext {
    /// Parsed interaction context, including user/member and installation data.
    pub context: InteractionContextData,
    /// Typed interaction payload that matched the route.
    pub interaction: Interaction,
    /// Route key resolved from the typed interaction.
    pub route: RouteKey,
}

impl AppContext {
    /// Returns the best available user ID from the interaction context.
    pub fn user_id(&self) -> Option<&Snowflake> {
        self.context.user.as_ref().map(|user| &user.id).or_else(|| {
            self.context
                .member
                .as_ref()
                .and_then(|member| member.user.as_ref())
                .map(|user| &user.id)
        })
    }
}

/// Async route handler used by [`AppFramework`].
pub trait AppRouteHandler: Send + Sync + 'static {
    /// Handles a matched route and returns a Discord interaction response.
    fn handle(&self, ctx: AppContext) -> FrameworkFuture;
}

impl<F, Fut> AppRouteHandler for F
where
    F: Fn(AppContext) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = InteractionResponse> + Send + 'static,
{
    fn handle(&self, ctx: AppContext) -> FrameworkFuture {
        Box::pin((self)(ctx))
    }
}

/// Synchronous guard for rejecting a route before it reaches its handler.
pub trait AppGuard: Send + Sync + 'static {
    /// Allows a route to continue or returns the response that should be sent.
    fn check(&self, ctx: &AppContext) -> Result<(), InteractionResponse>;
}

impl<F> AppGuard for F
where
    F: Fn(&AppContext) -> Result<(), InteractionResponse> + Send + Sync + 'static,
{
    fn check(&self, ctx: &AppContext) -> Result<(), InteractionResponse> {
        (self)(ctx)
    }
}

/// Minimal high-level interaction router for command, component, and modal apps.
///
/// The framework is intentionally small: it composes typed interactions,
/// per-route guards, and per-user cooldowns without replacing lower-level
/// `RestClient`, Gateway, or signed endpoint APIs.
pub struct AppFramework {
    routes: HashMap<RouteKey, Arc<dyn AppRouteHandler>>,
    guards: Vec<Arc<dyn AppGuard>>,
    cooldowns: HashMap<RouteKey, Duration>,
    cooldown_state: Mutex<HashMap<(RouteKey, Snowflake), Instant>>,
    fallback: Arc<dyn AppRouteHandler>,
}

impl AppFramework {
    fn lock_cooldowns(&self) -> MutexGuard<'_, HashMap<(RouteKey, Snowflake), Instant>> {
        self.cooldown_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Starts building a new application framework.
    pub fn builder() -> AppFrameworkBuilder {
        AppFrameworkBuilder::default()
    }

    /// Dispatches a typed interaction through guards, cooldowns, and route handlers.
    pub async fn dispatch(
        &self,
        context: InteractionContextData,
        interaction: Interaction,
    ) -> InteractionResponse {
        let route = route_key(&interaction);
        let app_context = AppContext {
            context,
            interaction,
            route: route.clone(),
        };

        for guard in &self.guards {
            if let Err(response) = guard.check(&app_context) {
                return response;
            }
        }

        if let Some(response) = self.check_cooldown(&app_context) {
            return response;
        }

        self.routes
            .get(&route)
            .unwrap_or(&self.fallback)
            .handle(app_context)
            .await
    }

    fn check_cooldown(&self, ctx: &AppContext) -> Option<InteractionResponse> {
        let cooldown = self.cooldowns.get(&ctx.route)?;
        let user_id = ctx.user_id()?.clone();
        let now = Instant::now();
        let mut state = self.lock_cooldowns();
        state.retain(|(route, _), last_seen| {
            self.cooldowns
                .get(route)
                .map(|duration| now.saturating_duration_since(*last_seen) < *duration)
                .unwrap_or(false)
        });
        let key = (ctx.route.clone(), user_id);

        if let Some(last_seen) = state.get(&key) {
            let elapsed = now.saturating_duration_since(*last_seen);
            if elapsed < *cooldown {
                return Some(InteractionResponse::ChannelMessage(json!({
                    "content": "This action is on cooldown.",
                    "flags": 64
                })));
            }
        }

        state.insert(key, now);
        None
    }
}

#[async_trait]
impl TypedInteractionHandler for AppFramework {
    async fn handle_typed(
        &self,
        ctx: InteractionContextData,
        interaction: Interaction,
    ) -> InteractionResponse {
        self.dispatch(ctx, interaction).await
    }
}

/// Builder for [`AppFramework`].
#[derive(Default)]
pub struct AppFrameworkBuilder {
    routes: HashMap<RouteKey, Arc<dyn AppRouteHandler>>,
    guards: Vec<Arc<dyn AppGuard>>,
    cooldowns: HashMap<RouteKey, Duration>,
    fallback: Option<Arc<dyn AppRouteHandler>>,
}

impl AppFrameworkBuilder {
    /// Registers a command route by command name.
    pub fn command<H>(mut self, name: impl Into<String>, handler: H) -> Self
    where
        H: AppRouteHandler,
    {
        self.routes
            .insert(RouteKey::Command(name.into()), Arc::new(handler));
        self
    }

    /// Registers a message component route by `custom_id`.
    pub fn component<H>(mut self, custom_id: impl Into<String>, handler: H) -> Self
    where
        H: AppRouteHandler,
    {
        self.routes
            .insert(RouteKey::Component(custom_id.into()), Arc::new(handler));
        self
    }

    /// Registers a modal submit route by `custom_id`.
    pub fn modal<H>(mut self, custom_id: impl Into<String>, handler: H) -> Self
    where
        H: AppRouteHandler,
    {
        self.routes
            .insert(RouteKey::Modal(custom_id.into()), Arc::new(handler));
        self
    }

    /// Adds a guard that runs before every route handler.
    pub fn guard<G>(mut self, guard: G) -> Self
    where
        G: AppGuard,
    {
        self.guards.push(Arc::new(guard));
        self
    }

    /// Adds a per-user cooldown for one route.
    pub fn cooldown(mut self, route: RouteKey, duration: Duration) -> Self {
        self.cooldowns.insert(route, duration);
        self
    }

    /// Sets the response handler for unmatched interactions.
    pub fn fallback<H>(mut self, handler: H) -> Self
    where
        H: AppRouteHandler,
    {
        self.fallback = Some(Arc::new(handler));
        self
    }

    /// Builds the framework.
    pub fn build(self) -> AppFramework {
        AppFramework {
            routes: self.routes,
            guards: self.guards,
            cooldowns: self.cooldowns,
            cooldown_state: Mutex::new(HashMap::new()),
            fallback: self.fallback.unwrap_or_else(|| {
                Arc::new(|_ctx: AppContext| async {
                    InteractionResponse::ChannelMessage(json!({
                        "content": "Unknown interaction.",
                        "flags": 64
                    }))
                })
            }),
        }
    }
}

fn route_key(interaction: &Interaction) -> RouteKey {
    match interaction {
        Interaction::ChatInputCommand(interaction) => interaction
            .data
            .name
            .clone()
            .map(RouteKey::Command)
            .unwrap_or(RouteKey::Unknown),
        Interaction::UserContextMenu(interaction) => interaction
            .data
            .name
            .clone()
            .map(RouteKey::Command)
            .unwrap_or(RouteKey::Unknown),
        Interaction::MessageContextMenu(interaction) => interaction
            .data
            .name
            .clone()
            .map(RouteKey::Command)
            .unwrap_or(RouteKey::Unknown),
        Interaction::Autocomplete(interaction) => interaction
            .data
            .name
            .clone()
            .map(RouteKey::Command)
            .unwrap_or(RouteKey::Unknown),
        Interaction::Component(interaction) => {
            RouteKey::Component(interaction.data.custom_id.clone())
        }
        Interaction::ModalSubmit(interaction) => {
            RouteKey::Modal(interaction.submission.custom_id.clone())
        }
        _ => RouteKey::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        AutocompleteInteraction, ChatInputCommandInteraction, CommandInteractionData,
        ComponentInteraction, ComponentInteractionData, MessageContextMenuInteraction,
        ModalSubmitInteraction, PingInteraction, User, UserContextMenuInteraction,
    };
    use crate::parsers::modal::V2ModalSubmission;

    fn context(user_id: &str) -> InteractionContextData {
        InteractionContextData {
            id: Snowflake::from("1"),
            application_id: Snowflake::from("2"),
            token: "token".to_string(),
            user: Some(User {
                id: Snowflake::from(user_id),
                username: "user".to_string(),
                ..User::default()
            }),
            ..InteractionContextData::default()
        }
    }

    fn command(name: &str, user_id: &str) -> (InteractionContextData, Interaction) {
        let ctx = context(user_id);
        (
            ctx.clone(),
            Interaction::ChatInputCommand(ChatInputCommandInteraction {
                context: ctx,
                data: CommandInteractionData {
                    name: Some(name.to_string()),
                    ..CommandInteractionData::default()
                },
            }),
        )
    }

    fn command_data(name: &str) -> CommandInteractionData {
        CommandInteractionData {
            name: Some(name.to_string()),
            ..CommandInteractionData::default()
        }
    }

    #[tokio::test]
    async fn app_framework_routes_commands_and_fallbacks() {
        let framework = AppFramework::builder()
            .command("ping", |_ctx| async {
                InteractionResponse::ChannelMessage(json!({ "content": "pong" }))
            })
            .build();

        let (ctx, interaction) = command("ping", "10");
        match framework.dispatch(ctx, interaction).await {
            InteractionResponse::ChannelMessage(value) => {
                assert_eq!(value["content"], json!("pong"));
            }
            _ => panic!("unexpected response"),
        }

        let unknown_ctx = context("10");
        match framework
            .dispatch(
                unknown_ctx.clone(),
                Interaction::Ping(PingInteraction {
                    context: unknown_ctx,
                }),
            )
            .await
        {
            InteractionResponse::ChannelMessage(value) => {
                assert_eq!(value["content"], json!("Unknown interaction."));
            }
            _ => panic!("unexpected response"),
        }
    }

    #[tokio::test]
    async fn app_framework_applies_guards_and_cooldowns() {
        let framework = AppFramework::builder()
            .guard(|ctx: &AppContext| {
                if ctx.context.guild_id.is_none() {
                    return Err(InteractionResponse::ChannelMessage(json!({
                        "content": "guild only",
                        "flags": 64
                    })));
                }
                Ok(())
            })
            .command("secure", |_ctx| async {
                InteractionResponse::ChannelMessage(json!({ "content": "ok" }))
            })
            .cooldown(
                RouteKey::Command("secure".to_string()),
                Duration::from_secs(30),
            )
            .build();

        let (ctx, interaction) = command("secure", "11");
        match framework.dispatch(ctx, interaction).await {
            InteractionResponse::ChannelMessage(value) => {
                assert_eq!(value["content"], json!("guild only"));
            }
            _ => panic!("unexpected response"),
        }

        let (mut ctx, interaction) = command("secure", "11");
        ctx.guild_id = Some(Snowflake::from("200"));
        assert!(matches!(
            framework.dispatch(ctx, interaction).await,
            InteractionResponse::ChannelMessage(_)
        ));

        let (mut ctx, interaction) = command("secure", "11");
        ctx.guild_id = Some(Snowflake::from("200"));
        match framework.dispatch(ctx, interaction).await {
            InteractionResponse::ChannelMessage(value) => {
                assert_eq!(value["content"], json!("This action is on cooldown."));
            }
            _ => panic!("unexpected response"),
        }

        let framework = AppFramework::builder()
            .command("short", |_ctx| async {
                InteractionResponse::ChannelMessage(json!({ "content": "ok" }))
            })
            .cooldown(
                RouteKey::Command("short".to_string()),
                Duration::from_millis(1),
            )
            .build();

        let (mut ctx, interaction) = command("short", "12");
        ctx.guild_id = Some(Snowflake::from("200"));
        assert!(matches!(
            framework.dispatch(ctx, interaction).await,
            InteractionResponse::ChannelMessage(_)
        ));
        tokio::time::sleep(Duration::from_millis(5)).await;
        let (mut ctx, interaction) = command("short", "12");
        ctx.guild_id = Some(Snowflake::from("200"));
        match framework.dispatch(ctx, interaction).await {
            InteractionResponse::ChannelMessage(value) => {
                assert_eq!(value["content"], json!("ok"));
            }
            _ => panic!("unexpected response"),
        }
        assert_eq!(framework.lock_cooldowns().len(), 1);
    }

    #[tokio::test]
    async fn app_framework_routes_all_interaction_key_types() {
        let framework = AppFramework::builder()
            .command("inspect", |ctx: AppContext| async move {
                let content = match ctx.route {
                    RouteKey::Command(name) => format!("command:{name}"),
                    _ => "wrong".to_string(),
                };
                InteractionResponse::ChannelMessage(json!({ "content": content }))
            })
            .component("button:ok", |_ctx| async {
                InteractionResponse::ChannelMessage(json!({ "content": "component" }))
            })
            .modal("modal:ok", |_ctx| async {
                InteractionResponse::ChannelMessage(json!({ "content": "modal" }))
            })
            .fallback(|ctx: AppContext| async move {
                InteractionResponse::ChannelMessage(json!({
                    "content": format!("{:?}", ctx.route)
                }))
            })
            .build();

        for interaction in [
            Interaction::UserContextMenu(UserContextMenuInteraction {
                context: context("12"),
                data: command_data("inspect"),
            }),
            Interaction::MessageContextMenu(MessageContextMenuInteraction {
                context: context("12"),
                data: command_data("inspect"),
            }),
            Interaction::Autocomplete(AutocompleteInteraction {
                context: context("12"),
                data: command_data("inspect"),
            }),
        ] {
            match framework.dispatch(context("12"), interaction).await {
                InteractionResponse::ChannelMessage(value) => {
                    assert_eq!(value["content"], json!("command:inspect"));
                }
                _ => panic!("unexpected command response"),
            }
        }

        match framework
            .dispatch(
                context("12"),
                Interaction::Component(ComponentInteraction {
                    context: context("12"),
                    data: ComponentInteractionData {
                        custom_id: "button:ok".to_string(),
                        component_type: 2,
                        values: Vec::new(),
                    },
                }),
            )
            .await
        {
            InteractionResponse::ChannelMessage(value) => {
                assert_eq!(value["content"], json!("component"));
            }
            _ => panic!("unexpected component response"),
        }

        match framework
            .dispatch(
                context("12"),
                Interaction::ModalSubmit(ModalSubmitInteraction {
                    context: context("12"),
                    submission: V2ModalSubmission {
                        custom_id: "modal:ok".to_string(),
                        components: Vec::new(),
                    },
                }),
            )
            .await
        {
            InteractionResponse::ChannelMessage(value) => {
                assert_eq!(value["content"], json!("modal"));
            }
            _ => panic!("unexpected modal response"),
        }

        match framework
            .dispatch(
                context("12"),
                Interaction::ChatInputCommand(ChatInputCommandInteraction {
                    context: context("12"),
                    data: CommandInteractionData::default(),
                }),
            )
            .await
        {
            InteractionResponse::ChannelMessage(value) => {
                assert_eq!(value["content"], json!("Unknown"));
            }
            _ => panic!("unexpected fallback response"),
        }
    }
}
