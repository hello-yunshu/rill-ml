# RillML and Performance Manager integration boundary

RillML v1.5.2 owns the generic `rill-ml` library, runtime, protocol, model,
and handler release surfaces. The OpenWrt Performance Manager adapter is not
an active RillML workspace member or release artifact: it is owned, built,
packaged, and released by
`hello-yunshu/luci-app-performance-manager`.

The v1.5.1 `pm-adapter` artifact kind remains readable by the release-index
protocol parser, and `tests/fixtures/legacy/rill-v1.5.1-pm-adapter-index.json`
is retained as an immutable compatibility fixture. This is read compatibility
only. The v1.5.2 release plan, publisher, signed index builder, asset verifier,
and post-release smoke jobs must not build, publish, or require a PM adapter.

The PM-owned consumer pins the generic crates.io dependency `rill-ml = 1.5.1`
and carries its own adapter source and lockfile. RillML v1.5.2 therefore does
not claim ownership of the PM adapter binary or its OpenWrt runtime behavior.
