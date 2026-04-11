# Voice

Voice support is a planned optional layer and not part of the current core runtime.

## Current Status

- the docs site now reserves a dedicated section for future voice APIs
- the expected direction is a feature-gated manager + connection state layer
- core `Client` work should stay independent from voice transport concerns

## Intended Scope

- voice connection lifecycle
- audio player / receiver primitives
- loose coupling with the Gateway and REST core
