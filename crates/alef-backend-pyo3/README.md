<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-backend-pyo3">
  <img src="https://img.shields.io/crates/v/alef-backend-pyo3?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-backend-pyo3

Python (PyO3) backend for alef

Generates Rust source code using PyO3 to expose a Rust library as a native Python extension module. Produces `#[pyclass]` structs with `#[pyo3(get)]` accessors, `#[pymethods]` blocks with constructors and static methods, and `#[pyfunction]` free functions. Supports dataclass, typeddict, pydantic, and msgspec DTO styles for flexible Python-side data representation. Generates `.pyi` type stub files for IDE autocompletion and static type checking, and wraps async functions using `pyo3_async_runtimes` to return Python awaitables.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
