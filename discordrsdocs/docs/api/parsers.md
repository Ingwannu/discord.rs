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
