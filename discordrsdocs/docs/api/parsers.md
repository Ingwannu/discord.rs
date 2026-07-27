# Parsers API

Parsers now serve two roles: migration-friendly raw helpers and typed interaction decoding.

## Typed Interaction Parser

Functions:

- `parse_interaction(&Value) -> Result<Interaction, Error>`
- `parse_raw_interaction(&Value) -> Result<RawInteraction, Error>`
- `parse_interaction_context(&Value) -> Result<InteractionContext, Error>`

Typed `Interaction` variants include:

- `Ping`
- `ChatInputCommand`
- `UserContextMenu`
- `MessageContextMenu`
- `Autocomplete`
- `Component`
- `ModalSubmit`
- `Unknown`

`RawInteraction` stays available for compatibility and low-level routing.

Since `2.2.0` the parsed variants also carry a shared response state, so the `discordrs::response::InteractionResponder` methods (`reply`, `defer`, `edit_reply`, `follow_up`, ...) work directly on parsed interactions, and double acknowledgements fail locally.

## Typed Resolved Data (2.2.0)

Interaction `resolved` payloads decode to the typed `ResolvedData` container instead of raw JSON: six lookup maps keyed by `Snowflake` (`users`, `members`, `roles`, `channels`, `messages`, `attachments`), each defaulting to empty when Discord omits it, with by-ID accessors:

```rust
use discordrs::Interaction;

if let Interaction::ChatInputCommand(command) = parse_interaction(payload)? {
    if let Some(resolved) = &command.data.resolved {
        if let Some(target) = command.data.target_id.as_ref().and_then(|id| resolved.user(id.clone())) {
            println!("resolved user: {}", target.username);
        }
    }
}
```

Resolved members are partial (no `user`, `deaf`, or `mute` fields) and resolved channels carry only id, name, type, permissions, and thread fields, matching Discord's documented shapes.

## Modal Parser

Function:

- `parse_modal_submission(&Value) -> Result<V2ModalSubmission, Error>`

`V2ModalSubmission` preserves V2 component fidelity, including:

- `Label`
- `RadioGroup`
- `CheckboxGroup`
- `Checkbox`
- text/select variants

## Example

```rust
match parse_interaction(payload)? {
    Interaction::ModalSubmit(modal) => {
        let value = modal
            .submission
            .get_radio_value("theme")
            .unwrap_or("Not selected");
        println!("Theme = {value}");
    }
    _ => {}
}
```

## Why Use Parsers

- Less brittle routing than raw JSON indexing
- Typed interaction variants for new code
- Common context extraction (`id`, `token`, `application_id`)
- Full-fidelity modal parsing for advanced workflows
