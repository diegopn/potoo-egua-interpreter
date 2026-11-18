# Security Policy

## Supported versions

The `develop` branch is the active development line. Reports affecting `develop` and the latest published release will be considered; older releases may no longer receive security updates.

## Report a vulnerability

Do not publish vulnerability details in issues, pull requests, discussions, or public comments. If GitHub provides the private **Report a vulnerability** form on the repository’s Security page, use it. Otherwise, use the [Potoo contact page](https://diegopn.github.io/#contato) only to request a private reporting channel. Do not include technical details or a proof of concept in that first message.

After arranging a private channel, include the affected version or commit, Rust version, operating system, reproduction steps, impact, and a minimal `Égua` example. Remove credentials, personal information, and unrelated data.

For non-security issues, see [CONTRIBUTING.md](CONTRIBUTING.md). For conduct concerns, see [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md).

## Security boundaries

Égua source runs in the process that calls the interpreter. The interpreter does not create an operating-system sandbox or enforce a general CPU or memory quota. The CLI imports source files from paths used by the program, and its `requisição` service invokes `curl` for the HTTP or HTTPS URL supplied by the program, with a 30-second per-request timeout. These operations use the process's filesystem and network permissions. Embedders should restrict `Host::load_module` to permitted files and `Host::request` to permitted destinations. Run untrusted source in a restricted process with operating-system access and resource limits.
