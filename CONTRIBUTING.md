# Contributing to OpenRig FX

Thanks for helping! Bug reports, sound reports, ideas, documentation and
code are all welcome. OpenRig FX is part of [OpenRig](https://openrig.eu);
the project's general guide is on the docs site:
[docs.openrig.eu/dev/contributing](https://docs.openrig.eu/dev/contributing/).

By taking part, you agree to follow the [code of conduct](CODE_OF_CONDUCT.md).

## Issues

[Open an issue](https://github.com/Siryu6/openrig-fx/issues/new/choose) and
pick the form that fits: bug report or feature request. Give the plugin, the
OpenRig FX version, the host (OpenRig, Ardour, Carla, jalv…) and your
machine. For a sound problem, say what you played and, if you can, attach a
short recording.

Security problems: see [SECURITY.md](SECURITY.md), not a public issue.

## Pull requests

1. Fork, branch from `main`, and open a pull request against `main`.
2. Run the checks that CI runs:

   ```sh
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   cargo xtask bundle --strict
   ```

   `--strict` needs the LV2 tools listed in the [README](README.md#lv2-bundles);
   without them, `cargo xtask bundle` reports the checks it skipped.
3. Update the docs with the change: `README.md`, `docs/DESIGN.md`, and
   `docs/OPENRIG-VIEWS.md` for the `openrig:` vocabulary.

For a large change or a new plugin, open an issue first so we can agree on
the approach.

## Commits and releases

Commit messages follow [conventional commits](https://www.conventionalcommits.org):
`feat:`, `fix:`, `perf:`, `docs:`, `ci:`, `chore:`… with an optional short
scope (`feat(echo):`). Pull requests are squash-merged, so the PR title
becomes the commit and follows the same format. Versions follow
[semantic versioning](https://semver.org).

A release is a `vX.Y.Z` tag on `main`. The release workflow builds and
validates the bundles on x86_64 and aarch64, and publishes one archive per
architecture with its `SHA256SUMS`. OpenRig's plugin store then points at
those files.

## Real-time code

Code that runs in a plugin's `process` must not allocate, lock, log or do
I/O. Allocate in constructors and `activate`; the test kit traps
allocations on the audio path (`orx-test`), and every plugin runs the
conformance suite.

## Sound changes

Golden files pin the sound of every preset. After an intended change of
sound, rewrite them with `ORX_BLESS=1 cargo test` and say why in the pull
request. A change of a plugin's ports breaks saved presets: `ports.lock`
catches it, and it needs a good reason.

## Names

Plugin, preset and control names are our own. Do not name brands, models,
bands or artists. Describe what a circuit does ("the classic mid-hump
op-amp overdrive"), not whose it is.

## License

OpenRig FX is GPL-3.0-or-later. By contributing, you agree to license your
contribution under the same terms.
