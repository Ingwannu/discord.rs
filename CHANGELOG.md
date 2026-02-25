# Changelog

## Unreleased

- Added `SlashCommandScope` and `register_slash_commands(...)` for unified global/guild slash registration.
- Extended `SlashCommandSet` ergonomics with scope-based `register[_ref]`, name-based upsert/remove (`set_command`, `with_set_command`, `remove`, `contains`), lookup/edit helpers (`get`, `get_mut`), plus standard `Extend`/`IntoIterator` support.
- Improved `InteractionRouter` robustness with per-kind fallback handlers (`*_fallback`) and clearer `contains_*` semantics (registered routes only; fallback excluded).
- Expanded `SlashCommandSet` ergonomics with `iter`/`iter_mut`, `merge`/`with_merged`, and `dedup_by_name` for safer incremental command assembly.
- Added generic exact-route APIs to `InteractionRouter`: `insert(kind, ...)`, `set(kind, ...)`, `remove(kind, ...)`, and `contains(kind, ...)`.
- Expanded tests and docs for the updated slash registration and interaction routing APIs.

## 0.1.3

- Added modal interaction components:
  - `RadioGroupBuilder` for single-choice selection.
  - `CheckboxGroupBuilder` for multi-choice selection.
  - `CheckboxBuilder` for yes/no style toggles.
- Updated package version to `0.1.3` in `Cargo.toml`.
