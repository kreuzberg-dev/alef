<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-backend-extendr">
  <img src="https://img.shields.io/crates/v/alef-backend-extendr?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-backend-extendr

R (extendr) backend for alef

Generates Rust source code using the extendr framework to expose a Rust library as an R package. Produces `#[extendr]`-annotated functions and R-compatible type wrappers, where integer types narrower than 64 bits map to `i32` and all 64-bit and floating-point types map to `f64` to match R's numeric representation. Supports `list` and `r6` DTO generation styles and generates NAMESPACE/DESCRIPTION scaffolding.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
