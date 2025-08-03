## Security / Isolation Brainstorm
I believe there are two main problems:
1. workspaces isolation (attacker shouldn't be able to compromise the backend or other users' data)
2. backend resource draining (e.g. Bitcoin mining in a macro expansion, spambots, ...)

### Ideas
- sanitize user Rust code input (macros, build.rs)

How? Would also need to scan dependencies or have a whitelist of allowed dependencies and/or macros.

- allow macros only on private backends with `MACROS_ALLOWED=1` flag, default is disabled (protects public backends)

Differentiating between public / private instances seems like a good idea anyway.
Still would need to detect and reject the macros. Very restrictive.

- run compile process in isolated environment (own container, VM, ...)
- 1 workspace ~ 1 user with write permissions only for workspace dir, run compile commands under the workspace user

These two are neat, but they don't solve the problem of resource draining.
Spawning isolated instance per workspace is quite complex for deployment.

### Solution
This is a learning project, so I will keep it very simple.
The API would be configurable with `SHEEET_SECRET_API_KEY`, which, if present, would make the backend private
and would require API calls to include the same secret API key.

Public demo instance would be insecure and not guaranteed with a huge disclaimer.
And maybe I will implement some basic isolation or sanitization if deemed necessary.