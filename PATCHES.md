# GanDesktop patches for gpui-fast-linux 0.1.13

The base commit imports the unmodified crates.io gpui-fast-linux 0.1.13 archive,
including its published Cargo manifests and Apache-2.0 license.
Upstream: https://github.com/longbridge/gpui-fast
Branch: `gds-0.1.13`.

The patch is ported from GanDesktop's `gpui-pre` / `gpui-pre-linux`
`gds-0.3.11` branches. It adds external Wayland/X11 parent identifiers and
explicit startup / activation tokens. Platform protocol objects and dispatch
remain inside GPUI. No rendering or layout optimization is changed.

Apply the `gpui-fast` and `gpui-fast-linux` overrides together and enable
`gpui-kit/gpui-fast`. The original package names and version are preserved.

## Standard Wayland clipboard MIME transport

Implements explicit clipboard MIME reads/writes through the window connection's
`wl_data_device`, `wl_data_source` and `wl_data_offer`. It uses the focused seat
and its input serial, without requiring data-control extensions. Reads capture
one offer, run bounded pipe I/O on the background executor, and return its revision
for conditional cleanup. Outgoing data belongs to its source, including when a
new selection replaces it while a transfer is still in progress.

Protocol: https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_data_device

## External modal parent lifetime

Keep the imported Wayland parent alive until the dialog and its toplevel have
been destroyed. `zxdg_imported_v2.destroy` immediately invalidates the parent
relationship. Releasing it first leaves KWin's `dialogparent` effect without a
parent to restore when the chooser closes (GanDesktop/gds#1).

The remaining teardown dependencies are preserved: stop frame scheduling;
destroy composition renderers before their subsurfaces; release the window
renderer; destroy decorations before the toplevel; destroy the dialog before
the toplevel and the toplevel/popup before its xdg_surface; release the external
parent; destroy the viewport before the wl_surface. Internal parent `children`
bookkeeping does not issue a Wayland request or detach the compositor's parent.

Regression validation uses KWin 6.8.2 with `dialogparent` loaded and a native
Wayland Qt parent invoking the GDS portal. Software-rendered KWin may disable
this effect; use `KWIN_EFFECTS_FORCE_ANIMATIONS=1` in an isolated nested KWin
session to exercise it. Verify that cancel, window close, and successful file
selection all restore the parent's original colors without minimizing it.
