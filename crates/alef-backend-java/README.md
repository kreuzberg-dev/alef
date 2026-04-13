<div align="center">

<img width="100%" alt="kreuzberg.dev banner" src="https://github.com/user-attachments/assets/1b6c6ad7-3b6d-4171-b1c9-f2026cc9deb8" />

<a href="https://crates.io/crates/alef-backend-java">
  <img src="https://img.shields.io/crates/v/alef-backend-java?color=007ec6" alt="crates.io">
</a>
<a href="https://discord.gg/xt9WY3GnKR">
  <img src="https://img.shields.io/badge/Discord-Join%20our%20community-7289da?logo=discord&logoColor=white" alt="Discord">
</a>

</div>

# alef-backend-java

Java (Panama FFM) backend for alef

Generates Java source code using the Panama Foreign Function & Memory API (JDK 21+) to call into the C FFI layer. Produces Java records for data types, enum classes, a raw FFI wrapper class with `Linker.downcallHandle` and `FunctionDescriptor` definitions, and a public facade class with idiomatic Java method signatures. Names conflicting with `java.lang.Object` methods are automatically disambiguated. Generates Maven pom.xml scaffolding.

Part of the [alef](https://github.com/kreuzberg-dev/alef) polyglot binding generator.

## License

MIT
