<!--
🤖 AI-RULEZ :: GENERATED FILE — DO NOT EDIT DIRECTLY
Project: Alef
Generated: 2026-04-20 09:50:19
Source: .ai-rulez/config.yaml
Target: GEMINI.md
Content: rules=27, sections=0, agents=1

WHAT IS AI-RULEZ
AI-Rulez is a directory-based AI governance tool. All configuration lives in
the .ai-rulez/ directory. This file is auto-generated from source files.

.AI-RULEZ FOLDER ORGANIZATION
Root content (always included):
  .ai-rulez/config.yaml    Main configuration (presets, profiles)
  .ai-rulez/rules/         Mandatory rules for AI assistants
  .ai-rulez/context/       Reference documentation
  .ai-rulez/skills/        Specialized AI prompts
  .ai-rulez/agents/        Agent definitions

Domain content (profile-specific):
  .ai-rulez/domains/{name}/rules/    Domain-specific rules
  .ai-rulez/domains/{name}/context/  Domain-specific documentation
  .ai-rulez/domains/{name}/skills/   Domain-specific AI prompts

Profiles in config.yaml control which domains are included.

INSTRUCTIONS FOR AI AGENTS
1. NEVER edit this file (GEMINI.md) - it is auto-generated

2. ALWAYS edit files in .ai-rulez/ instead:
   - Add/modify rules: .ai-rulez/rules/*.md
   - Add/modify context: .ai-rulez/context/*.md
   - Update config: .ai-rulez/config.yaml
   - Domain-specific: .ai-rulez/domains/{name}/rules/*.md

3. PREFER using the MCP Server (if available):
   Command: npx -y ai-rulez@latest mcp
   Provides safe CRUD tools for reading and modifying .ai-rulez/ content

4. After making changes: ai-rulez generate

5. Complete workflow:
   a. Edit source files in .ai-rulez/
   b. Run: ai-rulez generate
   c. Commit both .ai-rulez/ and generated files

Documentation: https://github.com/Goldziher/ai-rulez
Content-Hash: blake3:e490aa1d92293d12d4925e4018ed674e8a1fe007e3f38a5c6c2b1a2b2d2db661
-->

# Alef

Polyglot binding generator that produces type-safe language bindings for Rust libraries (Python/TypeScript/Ruby/Java/Go/Elixir/C#/PHP/R/WASM)

## Rules

### avoid-duplication

**Priority:** medium

Extract shared logic after the third repetition, not before. Three similar lines of code are better than a premature abstraction. When extracting, ensure the shared code has a single reason to change — if two callers would evolve the logic differently, keep them separate. Premature abstraction creates worse coupling than duplication.

### complexity-limits

**Priority:** medium

Enforce concrete limits: max 20 cyclomatic complexity per function, max 4 levels of nesting depth, max 50 lines per function. Use early returns to flatten conditionals. Break complex functions into well-named helpers that each do one thing.

### dead-code

**Priority:** low

Remove dead code instead of commenting it out. Version control preserves history. Commented-out code creates confusion and maintenance burden.

### error-handling

**Priority:** high

Always wrap errors with context describing what operation failed. Never swallow errors silently — either handle, propagate, or log them. Use language-idiomatic patterns: `Result<T, E>` in Rust, `if err != nil` with `fmt.Errorf("doing X: %w", err)` in Go, typed exceptions in Python/Java. Fail fast on unrecoverable errors.

### readability-first

**Priority:** high

Max 120 character line width. Prefer explicit code over clever tricks — if it needs a comment to explain what it does, rewrite it. No abbreviations in public API names (`context` not `ctx` in public signatures, `repository` not `repo`). Keep functions short and focused on a single responsibility.

### meaningful-assertions

**Priority:** medium

Assert exact expected values, not just truthiness (`assert result == 42`, not `assert result`). Use snapshot testing for complex structured output. Consider property-based testing for functions with wide input ranges. Include descriptive failure messages. Always test error paths and edge cases, not just the happy path.

### test-alongside-code

**Priority:** high

Write tests when writing code, update tests when modifying behavior. When fixing bugs, write a failing test first (TDD). Use integration tests for the public API surface and unit tests for complex internal logic. Run the full test suite before committing.

### test-independence

**Priority:** high

Tests must be independent and idempotent — runnable in any order, in parallel. No shared mutable state between tests. Use factories or fixtures for setup. Clean up created resources (files, DB rows, env vars) after each test. Never rely on test execution order.

### test-naming

**Priority:** medium

Name tests to describe behavior: `should_return_error_when_input_is_empty`, `test_parse_handles_nested_objects`. Use `describe`/`it` blocks for grouping in languages that support them. Follow `given_when_then` or `should_when` patterns. Test names are specifications — a reader should understand the expected behavior without reading the test body.

### atomic-commits

**Priority:** high

Each commit represents one logical change. Don't mix unrelated changes. Use conventional commits format (`feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`). Keep commits small and focused for easier review and bisection.

### branch-hygiene

**Priority:** medium

Use descriptive branch names. Keep branches short-lived. Delete merged branches. Rebase or merge from main regularly to avoid drift.

### commit-messages

**Priority:** high

Use conventional commits: `feat: add user auth`, `fix: handle null input`, `chore: update deps`, `refactor: extract parser`, `docs: add API guide`, `test: cover edge case`. First line under 72 chars, imperative mood. Body explains *why*, not *what*. Add scope when useful: `feat(api): add pagination`.

### safe-git-operations

**Priority:** critical

Never force-push to shared branches. Always pull before pushing. Use `--force-with-lease` instead of `--force` when necessary. Confirm destructive operations with the user.

### batch-operations

**Priority:** medium

Group related file reads and writes into single operations. Combine independent tool calls in parallel rather than sequentially. When making multiple edits to the same file, batch them into one edit operation. Prefer multi-file search tools over individual file reads when exploring.

### context-preservation

**Priority:** medium

Record key findings (file paths, function signatures, patterns discovered) before they scroll out of context. Summarize investigation results before acting on them. When working on multi-step tasks, note intermediate decisions and their rationale to avoid re-deriving them later.

### incremental-approach

**Priority:** medium

Start with the smallest viable change, verify it works, then extend. Avoid generating large blocks of speculative code. Build iteratively: implement one piece, test, then move to the next. When uncertain about an approach, prototype the critical part first before committing to the full implementation.

### output-awareness

**Priority:** medium

Limit explanations to 1-3 sentences unless asked for detail. Use code blocks for code, not prose. Omit unchanged code when showing diffs — use comments like `// ... existing code ...` to indicate skipped sections. Never repeat information already visible in context. Prefer short, direct answers over comprehensive walkthroughs.

### task-runner

**Priority:** high

Prefer `task` commands over raw build/test/lint commands when a Taskfile.yaml exists. Task runners provide consistent, documented workflows. Use `task --list` to discover available tasks. Always check for a Taskfile before running manual commands.

### explain-reasoning

**Priority:** medium

Briefly explain your reasoning for non-obvious decisions. State trade-offs when multiple approaches exist. Be transparent about uncertainty.

### minimal-changes

**Priority:** high

Make the smallest change that achieves the goal. Avoid unnecessary refactoring, reformatting, or scope creep. Don't fix what isn't broken.

### read-before-write

**Priority:** critical

Read and understand existing files before editing them. Understand the codebase conventions, patterns, and architecture before making changes. Check imports, naming styles, and project structure to ensure new code fits the existing codebase.

### verify-before-acting

**Priority:** critical

Verify assumptions before taking action. Check current state (branch, working directory, running processes) before making changes. Confirm file existence before editing. Test that build passes before committing. Never assume — confirm.

### rust-conventions

**Priority:** high

- Rust 2024 edition, `cargo fmt` + `clippy -D warnings`, zero warnings policy.
- `Result<T, E>` with `thiserror` for library errors, `anyhow` for applications. `?` for propagation — never `.unwrap()` in library code.
- Minimize `unsafe` — every block needs `// SAFETY:` comment explaining invariants.
- Prefer `&str` over `String` in params, `Cow<'_, str>` for conditional ownership, `Arc` for shared ownership.
- `impl Trait` in argument position for static dispatch, `dyn Trait` for dynamic dispatch when heterogeneous collections needed.
- Small, focused modules. Use `pub(crate)` for internal visibility. Workspace inheritance for multi-crate repos.
- `#[cfg(test)]` for unit tests, `tests/` for integration, `cargo-llvm-cov` for coverage.
- Benchmarking: `criterion` for microbenchmarks, profile with `cargo flamegraph`.
- Async: `tokio` runtime, `'static + Send + Sync` bounds, `tokio::spawn` for concurrency.
- Security: `cargo audit` for CVE scanning, `cargo deny` for license and advisory policies.
- Dependencies: pin versions, commit `Cargo.lock`, prefer well-maintained crates.
- Structured logging with `tracing` crate — use spans and events, not `println!`.
- API naming: follow `as_`/`to_`/`into_` conventions for conversions, `iter()`/`iter_mut()`/`into_iter()` for iterators. Getters are `field()` not `get_field()`. See [Rust API Guidelines](https://rust-lang.github.io/api-guidelines).
- Eagerly implement common traits: `Clone`, `Debug`, `Default`, `Eq`, `PartialEq`, `Hash`, `Send`, `Sync`. Use `From`/`AsRef`/`AsMut` for conversions, `FromIterator`/`Extend` for collections.
- Type safety: newtypes for static distinctions, builder pattern for complex construction, `bitflags` over enums for flag sets. Avoid `bool` params — use custom types or enums.
- Constructors: `new()` as static inherent methods. No out-parameters. Only smart pointers implement `Deref`/`DerefMut`.
- API flexibility: minimize parameter assumptions via generics, make traits object-safe when trait objects may be useful. Let callers decide where to copy and place data.
- Rustdoc: all public items have doc examples using `?` (not `unwrap`). Document errors, panics, and safety invariants. Hyperlink related items.
- Future-proofing: seal traits to prevent downstream implementations, keep struct fields private, don't duplicate derived trait bounds on structs. See [Rust Design Patterns](https://rust-unofficial.github.io/patterns).
- Anti-patterns: `unwrap()`, unguarded `unsafe`, panics in libraries, `Vec`/`HashMap` across FFI.

### dependency-awareness

**Priority:** high

Audit dependencies before adding them. Prefer well-maintained, widely-used packages with active maintenance. Pin versions and commit lock files. Use language-specific audit tools in CI:
- Rust: `cargo audit`, `cargo deny` (license + advisory policies)
- Python: `pip-audit`, `bandit` (SAST)
- JavaScript/TypeScript: `npm audit`, `pnpm audit`
- Go: `govulncheck`
- Ruby: `bundler-audit`
- PHP: `composer audit`
- Java: OWASP `dependency-check` Maven/Gradle plugin
- C#: `dotnet list package --vulnerable`
- Elixir: `mix_audit`
Zero tolerance for critical/high CVEs. Automate dependency update PRs where possible.

### input-validation

**Priority:** high

Validate and sanitize all external input at system boundaries. Use allowlists over denylists. Validate types, ranges, and formats. Never trust user input.

### least-privilege

**Priority:** medium

Request only necessary permissions. Minimize file system access, network access, and API scopes. Run processes with minimal required privileges.

### secrets-handling

**Priority:** critical

Never hardcode secrets, API keys, tokens, or passwords. Use environment variables or secret management systems. Never log or expose sensitive values. Reject commits containing secrets.

## Context

### architecture

## Code Generation Pipeline

`alef-extract` → `alef-core` → `alef-codegen` → `alef-backend-*` → `alef-cli`

- `alef-extract` — parses Rust source into IR (`ApiSurface`); uses `syn` for AST traversal
- `alef-core` — IR types (`ApiSurface`), config schema (`AlefConfig`), `Backend` trait, `AlefError`
- `alef-codegen` — shared generation utilities: type mapping, naming, struct/enum/function generators, Jinja templates
- `alef-backend-*` — one crate per target language; each implements `Backend` trait
- `alef-cli` — entry point: `alef build`, `alef scaffold`, `alef readme`
- `alef-adapters` — framework-specific adapters (e.g., PyO3 async, NAPI async)
- `alef-docs` — generates language-native doc comments from Rust rustdoc
- `alef-e2e` — end-to-end integration tests

## Adding a New Target Language

1. Create `crates/alef-backend-{lang}` crate
2. Add to workspace `Cargo.toml` members and `[workspace.dependencies]`
3. Implement `Backend` trait; use `alef-codegen` for shared helpers
4. Set `depends_on_ffi: true` in `BuildConfig` if binding via C FFI (Go, Java, C#)
5. Register in `alef-cli`'s backend dispatch table

## Generated vs User-Maintained Boundary

- `generated_header: true` — prepended with `// DO NOT EDIT`; overwritten by `alef build`
- `generated_header: false` — written once by `alef scaffold`; user-owned after that
- Binding glue code and type stubs are generated; package manifests are scaffolded once

### owasp-quick-reference

1. **Broken Access Control** — enforce authorization checks on every request, deny by default.
2. **Cryptographic Failures** — use strong standard algorithms, never roll your own crypto.
3. **Injection** — parameterize all queries, sanitize and validate all inputs.
4. **Insecure Design** — threat model early, validate business logic at every layer.
5. **Security Misconfiguration** — harden defaults, disable unnecessary features and endpoints.
6. **Vulnerable Components** — keep dependencies updated, audit regularly with language-specific tools.
7. **Authentication Failures** — require MFA, enforce strong passwords, implement rate limiting.
8. **Data Integrity Failures** — verify software updates, use signed artifacts and checksums.
9. **Logging Failures** — log all security events with context, protect log data from tampering.
10. **SSRF** — validate and allowlist URLs, restrict outbound network requests.

## Agents

When a task aligns with a specialized agent listed below, delegate to that agent instead of handling it directly. Launch multiple independent agent calls in parallel when possible.

- **alef-developer**: Alef binding generator development and code generation

