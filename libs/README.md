# Vendored agent sources

Five upstream agent implementations, pinned as shallow git submodules. Their contents are
not this project's code: each keeps its own license and release rhythm. To change one, fork
it and point the submodule at the fork instead of committing inside these trees.

| Directory | Upstream | Pinned | License | Toolchain |
| --- | --- | --- | --- | --- |
| `codex/` | https://github.com/openai/codex | `a0a30c4` | Apache-2.0 | pnpm + cargo |
| `gemini-cli/` | https://github.com/google-gemini/gemini-cli | `d4cb171` | Apache-2.0 | npm |
| `qwen-code/` | https://github.com/QwenLM/qwen-code | `58a8dcdf` | Apache-2.0 | pnpm |
| `omp/` | https://github.com/can1357/oh-my-pi | `34962a6` | MIT | bun + cargo |
| `dsh/` | https://github.com/deepseek-ai/deepseek-harness | `2e39e704` | MIT | pnpm |

## Installing

```sh
git submodule update --init --depth 1

(cd libs/codex && pnpm install --frozen-lockfile)
(cd libs/gemini-cli && "$(dirname "$(command -v node)")/npm" ci)
(cd libs/qwen-code && pnpm install --frozen-lockfile)
(cd libs/omp && bun install --frozen-lockfile)
(cd libs/dsh && pnpm install --frozen-lockfile)

(cd libs/omp && cargo fetch --locked)
```

`npm` on this host is a shim that forwards to bun; gemini-cli must be installed with the real
npm (its `package-lock.json` does not survive bun's lockfile migration). npm 11 gates
postinstall scripts by default, so `esbuild`/`node-pty` native builds were skipped; run
`npm rebuild` inside `libs/gemini-cli` if you need them.

## Exceptions

- **codex Rust dependencies are not fetched.** The `codex-rs` workspace includes
  `code-mode-runtime` and `v8-poc`, which depend on `openai-oss-forks/rusty_v8` at a pinned
  git revision. `cargo fetch` therefore clones `denoland/v8.git` (gigabytes, slow). Fetch
  only if you intend to build codex from source. Use the published CLI instead.
- **dsh bundles proprietary payloads.** Its own source is MIT, but
  `THIRD_PARTY_NOTICES.md:187` discloses official Claude Code platform packages pulled through
  `@anthropic-ai/claude-agent-sdk`; those are not open source and not modifiable.
- **codex vendors LGPL-2.1 bubblewrap** at `codex-rs/vendor/bubblewrap`. Modifying and
  distributing it carries LGPL obligations.
- Trademarks are not licensed by Apache-2.0 or MIT. `dsh/BRAND_GUIDELINES.md` restricts use of
  the DeepSeek Harness name.
