# Dependency audit scope

The 2026-09-05 workspace scan reports zero vulnerability-class entries and 19 warning-class advisories. This is not a warning-free audit: `cargo audit --deny warnings` fails for Pulse, and no advisory ignore list was added.

The warnings are inherited from the existing Tauri dependency graph. The dependency versions were not changed by the OpenCode/Astra feature work.

## Windows release

The Windows normal/runtime dependency tree does not contain `glib 0.18.5` or `rand 0.7.3`.

`rand 0.7.3` is a build dependency through `phf_generator`, `phf_codegen` and `selectors`, used by Tauri's HTML tooling. [RUSTSEC-2026-0097](https://rustsec.org/advisories/RUSTSEC-2026-0097) describes re-entrant custom logging through the thread RNG. This project does not define that logger pattern. The advisory remains recorded rather than suppressed.

GTK/GLib warnings concern the non-Windows Tauri platform graph. Other maintenance notices include `fxhash`, the `unic` family and `proc-macro-error`. These require upstream dependency work; this release does not claim to resolve them.

## Reproduce

```powershell
cargo audit --json
cargo tree -p pulse --target x86_64-pc-windows-msvc --edges normal
cargo tree --workspace --target all -i rand@0.7.3
```

The standalone Codex Discord Rich Presence runtime has a separate dependency graph and passes its warning-denying RustSec gate.
