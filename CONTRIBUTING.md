
# Contributing to zwasm-rust-sdk

Thank you for your interest in contributing to zwasm-rust-sdk!

This repository contains multiple crates, including low-level FFI bindings (zwasm-sys) and potentially higher-level libraries or tools.
We welcome bug reports, feature requests, documentation improvements, and pull requests for any part of this project.

## Development Setup

This project requires **Zig 0.17.0**, exactly, to build the underlying `zwasm` library: zwasm pins one Zig minor and moves it only in a zwasm minor release.

1. **Clone the repository with submodules:**
   ```bash
   git clone --recursive https://github.com/zwasm/zwasm-rust-sdk.git
   ```
   If you've already cloned without submodules, run:
   ```
   git submodule update --init --recursive
   ```

2. **Install Zig:**

   Ensure zig (version 0.17.0) is in your PATH.

## How to Contribute

- **Bug Reports & Feature Requests:**
  - Please open an issue on GitHub with a clear description and, if possible, steps to reproduce.
  - For feature requests, describe your use case and proposed solution.

- **Pull Requests:**
  - Fork the repository and create your branch from `main`.
  - Add tests for new features or bug fixes when possible.
  - Ensure your code passes `cargo test` and builds without warnings.
  - Update documentation and README as needed.
  - Open a pull request with a clear description of your changes.

- **Code Style:**
  - Follow standard Rust formatting (`cargo fmt`).
  - Use clear, descriptive commit messages.
  - Keep changes focused and minimal per pull request.

- **Discussion:**
  - For major changes or design discussions, please open an issue first to discuss your ideas.


## Project Scope

- The repository provides Rust bindings and tools for the zwasm WebAssembly runtime.
- The `zwasm-sys` crate offers low-level, unsafe FFI bindings to the zwasm C API.
- Safe wrappers, high-level APIs, or SDKs should be developed in separate crates within this repository or as external projects.
- Platform support is currently limited to Unix-like systems (Linux, macOS).


## License

By contributing, you agree that your contributions will be licensed under the MIT License (for Rust code), and you must also comply with the license of the zwasm C library and any other relevant components.

---

If you have any questions, feel free to open an issue or start a discussion on GitHub.
