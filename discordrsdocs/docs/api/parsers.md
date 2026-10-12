# Parsers API

Parsers now serve two roles: migration-friendly raw helpers and typed interaction decoding.

## Typed Interaction Parser

Functions:

- `decode_interaction(&str) -> Result<Interaction, Error>` (3.0): decodes the interaction object straight from its JSON text, the gateway `INTERACTION_CREATE` `d` or an interactions-endpoint body. This is the path the gateway and the typed endpoint use.
- `parse_interaction(&Value) -> Result<Interaction, Error>`: the same decoder on a `serde_json::Value` you already hold.
- `parse_raw_interaction(&Value) -> Result<RawInteraction, Error>`
- `parse_interaction_context(&Value) -> Result<InteractionContext, Error>`

`Interaction` implements `Serialize`/`Deserialize` in Discord's wire format (3.0; 2.x derived an externally tagged `{"ChatInputCommand": {...}}` form), so `serde_json::from_str::<Interaction>(text)` and `Interaction::deserialize(&*raw_value)` work too. The gateway, the interactions endpoint, and REST all decode from text; another reader (`from_slice`, a `Value`) works but compiles a second copy of every model decoder, which is why `parse_interaction(&Value)` writes the value back to text first. The decoder reads the payload once: no `Value` tree and no copy of `data`, `resolved`, or `member`. On a 3.9 KB chat-input payload with resolved users, members, channels, roles, and attachments it makes 60 allocations, where 2.x's `decode_event` made 652.

How it handles key order: JSON objects are unordered and `data`'s shape depends on `type`. When `type` comes first (as Discord's gateway sends it), `data` of a component, autocomplete, or modal submit interaction is decoded in place. An application command's `data` is always kept as JSON text first and decoded when the object ends, because its variant depends on the command `type` inside it and an unknown command type must keep `data` exactly as sent; that costs one copy of `data`. `data` that comes before `type` is handled the same way. Either way the result is the same.

What round-trips: every field of Discord's Interaction Structure is read and written back (`guild` and `channel` as partial objects, `message`, `version`, `attachment_size_limit` included). Left out: the undocumented, deprecated `entitlement_sku_ids`; the label and action row wrappers of a modal submit and the components' numeric `id`s, whose inputs are written back as a flat list; `null` values, which read and write as absent; and the keys named in the duplicate-key paragraph below that the models do not have yet. The documented examples are checked in both directions, so any other dropped key fails the tests. `Interaction::Unknown` keeps `data` byte for byte.

A repeated key that the decoder reads is an error everywhere: the interaction, its `data`, modal components (including their numeric `id`), and the models inside. Keys it does not read are skipped without that check: undocumented ones (such as `entitlement_sku_ids` or a member's `is_pending`) and the few documented ones the models do not have yet (a user's `display_name_styles`, a reaction's `burst_colors` and `me_burst`). `data` kept as text for an unknown type is not checked at all. Interaction `data` must be a JSON object (an array is an error, not read by position).

For a guild interaction Discord sends `member` (with `user`) and no `user`; the decoder fills `context.user` with a copy of `member.user`, so handlers find the invoker in one place. That copy is the one clone left in decoding, and it is not written back.

Typed `Interaction` variants include:

- `Ping`
- `ChatInputCommand`
- `UserContextMenu`
- `MessageContextMenu`
- `PrimaryEntryPoint` (3.0): an Entry Point command (command type `PRIMARY_ENTRY_POINT`); answer it with a `LAUNCH_ACTIVITY` callback or any command response
- `Autocomplete`
- `Component`
- `ModalSubmit`
- `Unknown`

`RawInteraction` stays available for compatibility and low-level routing.

`Interaction::Unknown { context, kind, raw_data }` covers an interaction `type` this version does not know and an application command whose command `type` it does not know. `raw_data` is the `data` object as received, byte for byte (`Option<Box<RawValue>>`, `None` without `data`); read it with `serde_json::from_str(raw.get())`. An unknown command type is never forced through the typed command model, so shapes it would reject (an object option value, say) still arrive here.

Fields every variant shares are on `context` (`InteractionContextData`): IDs, token, `user`/`member`, permissions, locales, entitlements, and since 3.0 also `guild` (`InteractionGuild`: id, locale, features), `channel` (partial `Channel`), `message` (for a component, the message it was attached to), `attachment_size_limit`, and `version`.

Message component interactions keep `data.resolved` too: `component.data.get_users()`, `get_members()`, `get_roles()`, and `get_channels()` look the selected values of a user, role, mentionable, or channel select up in it.

Command option values (`CommandInteractionOption::value`) are a `CommandOptionValue`: `String`, `Integer(i64)`, `Number(f64)`, or `Boolean`, as Discord sent them. Read them with `as_str()`, `as_i64()`, `as_f64()` (also accepts an integer, since a `NUMBER` option can arrive as `5`), `as_bool()`, or `as_id()` for user, channel, role, mentionable, and attachment options:

```rust
use discordrs::{Interaction, UserId};

if let Interaction::ChatInputCommand(command) = decode_interaction(body)? {
    for option in &command.data.options {
        let player: Option<UserId> = option.value.as_ref().and_then(|value| value.as_id());
        let slots = option.value.as_ref().and_then(|value| value.as_i64());
        println!("{}: {player:?} {slots:?}", option.name);
    }
}
```

Since `2.2.0` the parsed variants also carry a shared response state, so the `discordrs::response::InteractionResponder` methods (`reply`, `defer`, `edit_reply`, `follow_up`, ...) work directly on parsed interactions, and double acknowledgements fail locally.

## Typed Resolved Data (2.2.0)

Interaction `resolved` payloads decode to the typed `ResolvedData` container instead of raw JSON: six lookup maps keyed by typed IDs (`users` and `members` by `UserId`, `roles` by `RoleId`, `channels` by `ChannelId`, `messages` by `MessageId`, `attachments` by `AttachmentId`), each defaulting to empty when Discord omits it, with by-ID accessors:

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

`V2ModalSubmission` and `V2ModalComponent` also implement `Deserialize`/`Serialize` in Discord's wire format (3.0), so `serde_json::from_str::<V2ModalSubmission>(data)` reads a modal submit `data` object without a `Value` tree. A submission is written back as its inputs without their label wrappers, which reads back to the same value.

### Resolved data of a modal submit (3.0)

A File Upload submits attachment IDs, and user, role, mentionable, and channel selects submit IDs; Discord sends the objects behind them in `data.resolved`. `ModalSubmitInteraction` keeps it (`resolved: Option<ResolvedData>`) and resolves a component's values for you:

```rust
if let Interaction::ModalSubmit(modal) = decode_interaction(body)? {
    for file in modal.get_attachments("screenshot") {
        println!("{} ({} bytes): {:?}", file.filename, file.size, file.url);
    }
    let reviewers: Vec<_> = modal.get_users("reviewers").collect();
    let teams: Vec<_> = modal.get_roles("reviewers").collect(); // a mentionable select
}
```

`get_attachments`, `get_users`, `get_members`, `get_roles`, and `get_channels` return the resolved objects in submission order and skip an ID Discord did not resolve. `parse_modal_submission` returns the submitted values only.

`V2ModalSubmission` preserves V2 component fidelity, including:

- `Label` (its child component is read; action rows still work for older modals)
- `RadioGroup { value: Option<String> }`: `None` when nothing is selected (Discord sends `null`)
- `CheckboxGroup { values }`: empty when nothing is checked
- `Checkbox { checked: bool }`: read from Discord's `value` boolean
- text input (`value`), string/user/role/mentionable/channel selects and file uploads (`values`)

Lookup helpers: `get_text`, `get_select_values` (selects and checkbox groups), `get_file_values`, `get_radio_value` (`None` also when nothing was selected), and `get_checkbox`.

The same submitted values are typed on `MessageComponent` when you decode a component tree yourself: `value` is a `MessageComponentValue` (`String`, `Bool`, or `Other` for a JSON type this version does not know, kept as its JSON text in a `Box<RawValue>`; read it with `value_str()` / `value_bool()`), `values` holds the lists, and `default` is a checkbox's initial state in create payloads. `"value": null` and an absent `value` both read as `None`: Discord sends `null` only for a radio group submitted with nothing selected, which means the same as no value.

## Example

```rust
match parse_interaction(payload)? {
    Interaction::ModalSubmit(modal) => {
        let value = modal
            .submission
            .get_radio_value("theme")
            .unwrap_or("Not selected");
        let agreed = modal.submission.get_checkbox("updates").unwrap_or(false);
        println!("Theme = {value}, updates = {agreed}");
    }
    _ => {}
}
```

## Why Use Parsers

- Less brittle routing than raw JSON indexing
- Typed interaction variants for new code
- Common context extraction (`id`, `token`, `application_id`)
- Full-fidelity modal parsing for advanced workflows
