# Release Artifacts

This folder is used by the CI workflow to store built release binaries:

- `falconasm-x86_64-unknown-linux-gnu`
- `falconasm-aarch64-unknown-linux-gnu`
- `falconasm-x86_64-pc-windows-msvc.exe`
- `falconasm-aarch64-pc-windows-msvc.exe`
- `falconasm-x86_64-apple-darwin`
- `falconasm-aarch64-apple-darwin`

Run the workflow from GitHub Actions or tag a release (e.g. `v1.0.0`) to generate the artifacts and upload them for download.

Note: the Linux aarch64 build uses `cross` with a `Cross.toml` pre-build step to install `libwayland-dev`, which is required by `wayland-sys` during cross-compilation.
