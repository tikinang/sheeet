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
- [ ] implement spreadsheet functionality
- [ ] create public lib crate with basic functions in prelude
  - `add`, `sub`, `mul`, `div`, `pow`, `mod` (match the operators below)
  - `avg`, `sum`, `med`
  - `http_get` (enabled by a feature, keep the first compile nice and fast)
- [ ] add operators support (`+`,`-`,`*`,`/`,`^`,`%`) in expression parsing
- [ ] enhance workspace isolation
- [ ] workspace management
  - reset workspace
  - set secret API key
  - workspace status bar (ID, API key set, status - compiling, computing, idling)
- [ ] prepare Zerops Recipe for simple deployment

### Nice to Have 
- [ ] use `async` instead of spawning threads in `PUT /compile`
- [ ] browser stored environment secrets
- [ ] pre-heat workspaces for demo newcomers
- [ ] on-save formatting support
- [ ] code highlighting ([`highlight.js`](https://highlightjs.org))
- [ ] allow more robust crate structure
- [ ] share-able workspaces

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

# How does the algorithm work?
## Load from serialized data
1. set all cells `raw_value` and parse `expression`
2. iterate cells and compute `cached_value`, recursively for all dependencies from `expression`
OR
save even `cached_value` and then it should be enough to just load it

## Update cell value

If A1 depends on A2 and A3 (are in expression as reference),

```
A1=add(A2,A3)

A1 -> [ A2, A3 ]
```

then A1 is dependent of both the A2 and A3,

```
A2 <- [ A1 ], A3 <- [ A1 ]
```

and when A2 or A3 is changed, the A1 should be recomputed.

That means, when I update `A2`:
- update `A2` cell raw value and parse new `expression`
- all dependencies from the new `expression` must be registered as dependents in the reversed index
- all dependencies from the old `expression` must be removed from the reversed dependents index
- and then all dependent cells' `cached_value` must be recomputed, recursively 

```
A1=1
A2=2
A3=3
B1=A1+A2
B2=B1

B2 -> B1 -> [ A1, A2 ]
A1 <- [ B1 ], A2 <- [ B1 ], B1 <- [ B2 ]

# update A1 value must update B1 and B2
1. update A1
2. find B1 in reverse index (of A1)
3. update B1
4. find B2 in reverse index (of B1)
5. update B2

steps 2-5 are done recursively 

# update of B1=A2+A3 must update reverse index (add B1 to A3 and remove B1 from A1)
1. update B1
2. remove B1 from A1 reverse index
3. add B1 to A3 reverse index
4: find B2 in reverse index (of B1)
5. update B2

steps 1-3 are done once
steps 4-5 are done recursively
```
