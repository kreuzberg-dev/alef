<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-backend-php">
  <img src="https://img.shields.io/crates/v/alef-backend-php?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-backend-php

PHP (ext-php-rs) backend for alef

Generates Rust source code using ext-php-rs to expose a Rust library as a PHP 8.2+ extension. Produces `#[php_class]` structs with `#[php_impl]` method blocks, `#[php_function]` free functions, and serde-based conversion bridges. Async operations are handled via Tokio `block_on` to integrate with PHP's synchronous execution model. Supports `readonly-class` and `array` DTO generation styles and generates Composer package scaffolding.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
