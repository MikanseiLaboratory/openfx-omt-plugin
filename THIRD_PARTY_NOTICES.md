# Third-Party Notices

This file lists third-party software included with OpenFX OMT Plugin.
Crate versions are pinned by `Cargo.lock`.

## OpenFX headers

Vendored from [AcademySoftwareFoundation/openfx](https://github.com/AcademySoftwareFoundation/openfx)
commit `3de640d6f645fe6e346acd57e568d8b0a5ae4574`.

BSD 3-Clause License. The full text is in `crates/openfx/vendor/LICENSE.md`.

```text
Copyright (c) 2025, OpenFX and contributors to the OpenFX project
SPDX-License-Identifier: BSD-3-Clause
```

## openmediatransport-rs

Pinned git revision: `55ffd08ab899f8017056886157fb0d130ab36d5c`

MIT License. Copyright (c) 2026 Open Media Transport Contributors and MikanseiLaboratory.

## vmx-rs

Transitive dependency of `openmediatransport-rs`, pinned by `Cargo.lock`.

MIT License. Copyright (c) 2026 Open Media Transport Contributors and MikanseiLaboratory.

## Design references (no source copied)

Resolve host compatibility notes (Filter + General, Create/Destroy, tiles off, avoiding `IsIdentity`) follow the public behavior of [ntsc-rs](https://github.com/valadaptive/ntsc-rs). ntsc-rs is GPL-3.0-or-later; this plugin does not include its source.

RAII patterns for OpenFX suites, images, and instance data were informed by [kreantio/openfx-rs](https://github.com/kreantio/openfx-rs) examples.

## Remaining crates

See `Cargo.lock` for the complete, version-pinned dependency graph. Runtime and build crates include:

bindgen, bitflags, bytes, cexpr, clang-sys, crossbeam-deque, crossbeam-epoch, crossbeam-utils, either, fastrand, flume, futures-core, futures-sink, glob, if-addrs, itertools, libloading, lock_api, log, mdns-sd, memchr, mio, nom, once_cell, openfx, openfx-omt-plugin, openmediatransport, pin-project-lite, prettyplease, proc-macro2, quote, rayon, rayon-core, regex, regex-automata, regex-syntax, rustc-hash, scopeguard, shlex, socket-pktinfo, socket2, spin, syn, thiserror, thiserror-impl, tracing, tracing-attributes, tracing-core, unicode-ident, vmx, windows-link, windows-sys

License texts for crates.io packages can be regenerated with `cargo about generate` when `cargo-about` is available.
