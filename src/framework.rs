use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use crate::{
    interactions::TypedInteractionHandler,
    model::{Interaction, InteractionContextData, UserId},
    request::{EndpointReply, InteractionMessage, InteractionResponse},
    response::InteractionResponder,
};

/// Boxed future returned by application framework route handlers.
pub type FrameworkFuture = Pin<Box<dyn Future<Output = EndpointReply> + Send>>;

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
///
/// Implements [`InteractionResponder`], so a route can respond over REST and
/// keep (or move into a spawned task) a clone to send follow-ups later. All
/// clones share one response state with the interaction: once the endpoint
/// returns the route's inline response, `follow_up` and `edit_reply` on a
/// retained clone are accepted. A route that sends its initial response over
/// REST should return [`EndpointReply::Accepted`], which the endpoint
/// answers with `202` and no body. `Debug` output never contains the token.
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
    pub fn user_id(&self) -> Option<UserId> {
        self.context.user.as_ref().map(|user| user.id).or_else(|| {
            self.context
                .member
                .as_ref()
                .and_then(|member| member.user.as_ref())
                .map(|user| user.id)
        })
    }
}

impl InteractionResponder for AppContext {
    fn responder_context(&self) -> &InteractionContextData {
        &self.context
    }
}

/// Async route handler used by [`AppFramework`].
///
/// Any `Fn(AppContext) -> impl Future` whose output converts into an
/// [`EndpointReply`] is a handler, so a route can return an
/// [`InteractionResponse`] (answered inline) or [`EndpointReply::Accepted`].
pub trait AppRouteHandler: Send + Sync + 'static {
    /// Handles a matched route and returns how the endpoint answers.
    fn handle(&self, ctx: AppContext) -> FrameworkFuture;
}

impl<F, Fut, R> AppRouteHandler for F
where
    F: Fn(AppContext) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = R> + Send + 'static,
    R: Into<EndpointReply>,
{
    fn handle(&self, ctx: AppContext) -> FrameworkFuture {
        let future = (self)(ctx);
        Box::pin(async move { future.await.into() })
    }
}

/// Synchronous guard for rejecting a route before it reaches its handler.
pub trait AppGuard: Send + Sync + 'static {
    /// Allows a route to continue or returns how the endpoint answers
    /// instead.
    fn check(&self, ctx: &AppContext) -> Result<(), EndpointReply>;
}

impl<F> AppGuard for F
where
    F: Fn(&AppContext) -> Result<(), EndpointReply> + Send + Sync + 'static,
{
    fn check(&self, ctx: &AppContext) -> Result<(), EndpointReply> {
        (self)(ctx)
    }
}

/// An ephemeral inline reply with this text, for the framework's own
/// cooldown and unknown-route answers.
fn ephemeral_notice(content: &str) -> EndpointReply {
    InteractionResponse::reply(InteractionMessage::from(content).ephemeral(true)).into()
}

/// Minimal high-level interaction router for command, component, and modal apps.
///
/// The framework is intentionally small: it composes typed interactions,
/// per-route guards, and per-user cooldowns without replacing lower-level
/// `RestClient`, Gateway, or signed endpoint APIs.
///
/// Cloning is cheap and clones share routes and cooldown state. The typed
/// endpoint constructors require `Clone` handlers, so this is what lets an
/// `AppFramework` be passed to [`crate::try_typed_interactions_endpoint`].
#[derive(Clone)]
pub struct AppFramework {
    routes: HashMap<RouteKey, Arc<dyn AppRouteHandler>>,
    guards: Vec<Arc<dyn AppGuard>>,
    cooldowns: HashMap<RouteKey, Duration>,
    cooldown_state: Arc<Mutex<HashMap<(RouteKey, UserId), Instant>>>,
    fallback: Arc<dyn AppRouteHandler>,
}

impl AppFramework {
    fn lock_cooldowns(&self) -> MutexGuard<'_, HashMap<(RouteKey, UserId), Instant>> {
        self.cooldown_state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Starts building a new application framework.
    pub fn builder() -> AppFrameworkBuilder {
        AppFrameworkBuilder::default()
    }

    /// Dispatches a typed interaction through guards, cooldowns, and route handlers.
    ///
    /// Returns the route's inline response without sending it. Through the
    /// typed interactions endpoint, the endpoint sends it and records it in
    /// the interaction's shared response state, or answers `202` with no body
    /// when the route already responded over REST.
    pub async fn dispatch(
        &self,
        context: InteractionContextData,
        interaction: Interaction,
    ) -> EndpointReply {
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

    fn check_cooldown(&self, ctx: &AppContext) -> Option<EndpointReply> {
        let cooldown = self.cooldowns.get(&ctx.route)?;
        let user_id = ctx.user_id()?;
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
                return Some(ephemeral_notice("This action is on cooldown."));
            }
        }

        state.insert(key, now);
        None
    }
}

impl TypedInteractionHandler for AppFramework {
    async fn handle_typed(
        &self,
        ctx: InteractionContextData,
        interaction: Interaction,
    ) -> EndpointReply {
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
            cooldown_state: Arc::new(Mutex::new(HashMap::new())),
            fallback: self.fallback.unwrap_or_else(|| {
                Arc::new(|_ctx: AppContext| async { ephemeral_notice("Unknown interaction.") })
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
        Interaction::PrimaryEntryPoint(interaction) => interaction
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
    use crate::model::{test_id, ComponentType};
    use crate::model::{
        AutocompleteInteraction, ChatInputCommandInteraction, CommandInteractionData,
        ComponentInteraction, ComponentInteractionData, MessageContextMenuInteraction,
        ModalSubmitInteraction, PingInteraction, User, UserContextMenuInteraction,
    };
    use crate::parsers::modal::V2ModalSubmission;
    use serde_json::Value;

    /// The `content` of an inline channel-message reply, read from its wire
    /// JSON (the stub-independent way to look inside it).
    fn reply_content(reply: EndpointReply) -> String {
        let EndpointReply::Inline(response) = reply else {
            panic!("expected an inline reply");
        };
        assert_eq!(
            response.kind(),
            crate::model::InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE
        );
        let wire: Value = serde_json::to_value(&response).unwrap();
        wire["data"]["content"].as_str().unwrap().to_owned()
    }

    fn context(user_id: &str) -> InteractionContextData {
        InteractionContextData {
            id: test_id(1),
            application_id: test_id(2),
            token: "token".to_string(),
            user: Some(User {
                id: user_id.parse().unwrap(),
                username: "user".to_string(),
                ..User::fixture()
            }),
            ..InteractionContextData::fixture()
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
            .command("ping", |_ctx| async { InteractionResponse::reply("pong") })
            .build();

        let (ctx, interaction) = command("ping", "10");
        assert_eq!(
            reply_content(framework.dispatch(ctx, interaction).await),
            "pong"
        );

        let unknown_ctx = context("10");
        assert_eq!(
            reply_content(
                framework
                    .dispatch(
                        unknown_ctx.clone(),
                        Interaction::Ping(PingInteraction {
                            context: unknown_ctx,
                        }),
                    )
                    .await
            ),
            "Unknown interaction."
        );
    }

    #[tokio::test]
    async fn app_framework_applies_guards_and_cooldowns() {
        let framework = AppFramework::builder()
            .guard(|ctx: &AppContext| {
                if ctx.context.guild_id.is_none() {
                    return Err(InteractionResponse::reply("guild only").into());
                }
                Ok(())
            })
            .command("secure", |_ctx| async { InteractionResponse::reply("ok") })
            .cooldown(
                RouteKey::Command("secure".to_string()),
                Duration::from_secs(30),
            )
            .build();

        let (ctx, interaction) = command("secure", "11");
        assert_eq!(
            reply_content(framework.dispatch(ctx, interaction).await),
            "guild only"
        );

        let (mut ctx, interaction) = command("secure", "11");
        ctx.guild_id = Some(test_id(200));
        assert_eq!(
            reply_content(framework.dispatch(ctx, interaction).await),
            "ok"
        );

        let (mut ctx, interaction) = command("secure", "11");
        ctx.guild_id = Some(test_id(200));
        assert_eq!(
            reply_content(framework.dispatch(ctx, interaction).await),
            "This action is on cooldown."
        );

        let framework = AppFramework::builder()
            .command("short", |_ctx| async { InteractionResponse::reply("ok") })
            .cooldown(
                RouteKey::Command("short".to_string()),
                Duration::from_millis(1),
            )
            .build();

        let (mut ctx, interaction) = command("short", "12");
        ctx.guild_id = Some(test_id(200));
        assert_eq!(
            reply_content(framework.dispatch(ctx, interaction).await),
            "ok"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
        let (mut ctx, interaction) = command("short", "12");
        ctx.guild_id = Some(test_id(200));
        assert_eq!(
            reply_content(framework.dispatch(ctx, interaction).await),
            "ok"
        );
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
                InteractionResponse::reply(content.as_str())
            })
            .component("button:ok", |_ctx| async {
                InteractionResponse::reply("component")
            })
            .modal("modal:ok", |_ctx| async {
                InteractionResponse::reply("modal")
            })
            .fallback(|ctx: AppContext| async move {
                InteractionResponse::reply(format!("{:?}", ctx.route).as_str())
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
            assert_eq!(
                reply_content(framework.dispatch(context("12"), interaction).await),
                "command:inspect"
            );
        }

        assert_eq!(
            reply_content(
                framework
                    .dispatch(
                        context("12"),
                        Interaction::Component(ComponentInteraction {
                            context: context("12"),
                            data: ComponentInteractionData {
                                custom_id: "button:ok".to_string(),
                                component_type: ComponentType::BUTTON,
                                id: None,
                                values: Vec::new(),
                                resolved: None,
                            },
                        }),
                    )
                    .await
            ),
            "component"
        );

        assert_eq!(
            reply_content(
                framework
                    .dispatch(
                        context("12"),
                        Interaction::ModalSubmit(ModalSubmitInteraction {
                            context: context("12"),
                            submission: V2ModalSubmission {
                                custom_id: "modal:ok".to_string(),
                                components: Vec::new(),
                            },
                            resolved: None,
                        }),
                    )
                    .await
            ),
            "modal"
        );

        assert_eq!(
            reply_content(
                framework
                    .dispatch(
                        context("12"),
                        Interaction::ChatInputCommand(ChatInputCommandInteraction {
                            context: context("12"),
                            data: CommandInteractionData::default(),
                        }),
                    )
                    .await
            ),
            "Unknown"
        );
    }

    #[test]
    fn app_framework_is_accepted_by_the_typed_endpoint_and_clones_share_cooldowns() {
        let framework = AppFramework::builder()
            .command("limited", |_ctx| async { InteractionResponse::reply("ok") })
            .cooldown(
                RouteKey::Command("limited".to_string()),
                Duration::from_secs(30),
            )
            .build();
        let clone = framework.clone();

        let public_key = "0000000000000000000000000000000000000000000000000000000000000000";
        assert!(
            crate::interactions::try_typed_interactions_endpoint(public_key, clone.clone()).is_ok()
        );

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let (ctx, interaction) = command("limited", "13");
        assert_eq!(
            reply_content(runtime.block_on(framework.dispatch(ctx, interaction))),
            "ok"
        );
        let (ctx, interaction) = command("limited", "13");
        assert_eq!(
            reply_content(runtime.block_on(clone.dispatch(ctx, interaction))),
            "This action is on cooldown."
        );
    }

    #[tokio::test]
    async fn app_context_responds_through_the_shared_state_and_hides_the_token() {
        let (context, interaction) = command("inspect", "14");
        let context = InteractionContextData {
            token: "app-context-secret-token".to_string(),
            ..context
        };
        let ctx = AppContext {
            context,
            interaction,
            route: RouteKey::Command("inspect".to_string()),
        };

        let debug = format!("{ctx:?}");
        assert!(!debug.contains("app-context-secret-token"), "{debug}");
        assert!(debug.contains("[redacted]"), "{debug}");

        assert!(!ctx.is_acknowledged());
        let error = ctx
            .follow_up(
                &crate::http::RestClient::new("bot-token", Some(test_id(2))),
                "early",
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("has not been acknowledged"));
    }

    #[tokio::test]
    async fn entry_point_routes_and_member_users_resolve() {
        // A guild interaction carries the user on the member.
        let mut ctx = context("16");
        let user = ctx.user.take();
        ctx.member = Some(crate::model::Member {
            user: user.map(std::sync::Arc::new),
            ..crate::model::Member::default()
        });
        let interaction =
            Interaction::PrimaryEntryPoint(crate::model::PrimaryEntryPointInteraction {
                context: ctx.clone(),
                data: command_data("launch"),
            });
        let app_context = AppContext {
            context: ctx.clone(),
            interaction: interaction.clone(),
            route: route_key(&interaction),
        };
        assert_eq!(app_context.user_id(), Some(test_id(16)));
        assert_eq!(app_context.route, RouteKey::Command("launch".to_string()));

        let framework = AppFramework::builder()
            .command("launch", |_ctx| async {
                InteractionResponse::LaunchActivity
            })
            .build();
        let reply = framework.dispatch(ctx, interaction).await;
        assert!(matches!(
            reply,
            EndpointReply::Inline(response) if response.kind()
                == crate::model::InteractionResponseType::LAUNCH_ACTIVITY
        ));
    }
}
