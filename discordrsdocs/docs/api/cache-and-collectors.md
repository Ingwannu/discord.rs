# Cache and Collectors

These layers are optional. They are meant to improve runtime ergonomics without making the base crate heavy.

## Cache

Enable the `cache` feature when the bot needs in-memory state for common lookups.

Main types:

- `CacheHandle`
- `GuildManager`
- `ChannelManager`
- `MemberManager`
- `MessageManager`
- `RoleManager`

The managers prefer cache hits and fall back to `RestClient` fetches.

## Collectors

Enable the `collectors` feature when the bot needs event-driven waiting flows.

Main types:

- `CollectorHub`
- `MessageCollector`
- `InteractionCollector`
- `ComponentCollector`
- `ModalCollector`

Collectors subscribe to typed `Event` values and let handlers wait for the next matching runtime event.

## Typical Use

- cache for hot-path lookups
- collectors for button, modal, or follow-up message flows
- both together for stateful multi-step bots
