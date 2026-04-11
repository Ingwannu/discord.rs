# HTTP and Helpers API

## `RestClient`

`RestClient` is the primary Discord REST v10 surface. It keeps shared route/global rate-limit state and also keeps `DiscordHttpClient` as a compatibility alias.

Common operations include:

- typed message create/update/get
- typed guild/channel/member/role lookups
- typed application command overwrite
- interaction responses and follow-up webhook helpers

## Helper Functions

For Components V2 and interaction response workflows, use:

- `send_container_message(...)`
- `respond_with_container(...)`
- `respond_component_with_container(...)`
- `respond_modal_with_container(...)`
- `respond_with_modal(...)`

## Example

```rust
let c = create_container("Notice", "Done", vec![], None);
respond_with_container(http, &ctx.id, &ctx.token, c, true).await?;
```

## Recommended Pattern

- Use `RestClient` or context managers for typed fetch/send flows.
- Use helper functions for common interaction acknowledgment paths.
- Keep Components V2 payload generation inside the builders.
- Fall back to low-level raw request helpers only when the typed surface does not yet cover the route.
