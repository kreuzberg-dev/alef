<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-backend-napi">
  <img src="https://img.shields.io/crates/v/alef-backend-napi?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-backend-napi

Node.js (NAPI-RS) backend for alef

Generates Rust source code using the NAPI-RS framework to expose a Rust library as a native Node.js addon. Produces `#[napi]` structs and functions, `#[napi(constructor)]` constructors, `#[napi(string_enum)]` enums, and native async functions returning JavaScript Promises. Supports `interface` and `zod` DTO generation styles and includes a post-build step that patches generated `.d.ts` TypeScript declaration files for correct type definitions.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
