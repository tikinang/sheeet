# SHEEET

Rust and WebAssembly powered spreadsheet.

![Sheeet logo.](/sheeet-baner.png)

---

## Public Demo

> [!WARNING]
> Don't share any sensitive information in the code to be compiled.

Live at: https://sheeet.matejpavlicek.cz

## TODO

### Necessary
- implement spreadsheet functionality
  - [x] copy, cut, paste cell
  - [x] select and manipulate multiple cells
  - [x] deselect cell out when clicking in the editor
- [x] support for range references
- [x] create public lib crate with basic functions in prelude
  - `add`, `sub`, `mul`, `div`, `pow` (match the operators below)
  - `avg`, `sum`, `med`, `concat_with`
  - `http_get` (enabled by a feature, keep the first compile nice and fast)
- [ ] add operators support (`+`,`-`,`*`,`/`,`^`,`%`) in expression parsing
- [x] enhance workspace isolation
  - allow making API access private (`SHEEET_SECRET_API_KEY`)
- [x] workspace management
  - reset workspace (both ID and sheet data)
  - set secret API key
  - workspace status bar (ID, API key, compile status, save status)
- [ ] prepare Zerops Recipe for simple deployment
- [ ] better onboarding (default data and code with comments)

### Nice to Have
- [ ] add and remove columns and rows
- [ ] use `async` instead of spawning threads in `PUT /compile`
- [ ] browser stored environment secrets
- [ ] pre-heat workspaces for demo newcomers
- [ ] on-save formatting support
- [ ] code highlighting ([`highlight.js`](https://highlightjs.org))
- [ ] allow more robust crate structure
- [ ] share-able workspaces
- [ ] add note to `README.md` where which data lives
- [ ] extender on range end

### Fixes
- [ ] self reference
- [x] update unbounded range dependents
- [ ] error handling and displaying (`=add(A1,,)` panics)

## Security / Isolation Brainstorm
I believe there are two main problems:
1. workspaces isolation (attacker shouldn't be able to compromise the backend or other users' data)
2. backend resource draining (e.g. Bitcoin mining in a macro expansion, spambots, ...)

### Ideas
- sanitize user Rust code input (macros, build.rs)

How? Would need also to scan dependencies or have a whitelist of allowed dependencies and/or macros.

- allow macros only on private backends with `MACROS_ALLOWED=1` flag, default is disabled (protects public backends)

Differentiating between public / private instances seems like a good idea anyway.
Still would need to detect and reject the macros. Very restrictive.

- run compile process in isolated environment (own container, VM, ...)
- 1 workspace ~ 1 user with write permissions only for workspace dir, run compile commands under the workspace user

These two are neat, but they don't solve the problem of resource draining.
Spawning isolated instance per workspace is quite complex for deployment.

### Solution
This is a learning project, so I will keep it very simple.
API would be configurable with `SHEEET_SECRET_API_KEY`, which if present would make the backend private
and would require API calls to include the same secret API key.

Public demo instance would be insecure and not guaranteed with huge disclaimer.
And maybe I will implement some basic isolation or sanitization if deemed necessary.
