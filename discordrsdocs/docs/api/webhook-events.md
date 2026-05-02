# Webhook Events API

Discord Webhook Events are HTTP event payloads sent to an application's configured Events URL. They are separate from Gateway dispatches and incoming webhooks.

Use `verify_discord_signature(...)` or the same Ed25519 header verification path used by the interactions endpoint before trusting the request body. Then parse the JSON body with `parse_webhook_event_payload(...)`.

```rust
use discordrs::{parse_webhook_event_payload, WebhookEvent, WebhookPayloadType};

let payload = parse_webhook_event_payload(body_json)?;

if payload.kind == WebhookPayloadType::PING {
    // Acknowledge Discord's URL verification probe with HTTP 204.
}

if let Some(event) = payload.event {
    match event.event {
        WebhookEvent::ApplicationAuthorized(data) => {
            println!("{} authorized {:?}", data.user.username, data.scopes);
        }
        WebhookEvent::EntitlementCreate(entitlement) => {
            println!("entitlement {}", entitlement.id);
        }
        WebhookEvent::LobbyMessageCreate(message) => {
            println!("lobby message {}", message.id);
        }
        WebhookEvent::Unknown { kind, data } => {
            println!("unhandled webhook event {kind}: {data:?}");
        }
        _ => {}
    }
}
```

## Parsed Event Families

- `APPLICATION_AUTHORIZED`
- `APPLICATION_DEAUTHORIZED`
- `ENTITLEMENT_CREATE`
- `ENTITLEMENT_UPDATE`
- `ENTITLEMENT_DELETE`
- `QUEST_USER_ENROLLMENT`
- `LOBBY_MESSAGE_CREATE`
- `LOBBY_MESSAGE_UPDATE`
- `LOBBY_MESSAGE_DELETE`
- `GAME_DIRECT_MESSAGE_CREATE`
- `GAME_DIRECT_MESSAGE_UPDATE`
- `GAME_DIRECT_MESSAGE_DELETE`

Unknown event types are preserved as `WebhookEvent::Unknown` with the raw `data` object so newly released Discord events do not break parsing.
