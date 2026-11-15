//! Registry of the OpenRig FX plugins, for the tools that handle all of
//! them: the bundle generator (`cargo xtask bundle`) and the JACK
//! standalone. Adding a plugin means adding its entry to a [`BUNDLES`]
//! row and its type to that bundle's `export_lv2!` line.

use orx_plugin::dynamic::Entry;

/// An LV2 bundle: one shared library holding one or more plugins
#[derive(Debug, Clone, Copy)]
pub struct BundleDef {
    /// Bundle directory without `.lv2`, e.g. `or-tremolo`
    pub name: &'static str,
    /// Cargo package of the cdylib
    pub package: &'static str,
    /// Library name of the cdylib (`lib<lib>.so` as built, `<lib>.so` in
    /// the bundle)
    pub lib: &'static str,
    /// Cargo packages whose tests must pass before the bundle is built
    /// (they hold the conformance and real-time checks backing
    /// `lv2:hardRTCapable`)
    pub tests: &'static [&'static str],
    /// The plugins, in the cdylib's descriptor order
    pub plugins: &'static [Entry],
}

pub const BUNDLES: &[BundleDef] = &[
    BundleDef {
        name: "or-tremolo",
        package: "orx-tremolo-lv2",
        lib: "or_tremolo",
        tests: &["orx-tremolo"],
        plugins: &[Entry::of::<orx_tremolo::Mono>(), Entry::of::<orx_tremolo::Stereo>()],
    },
    BundleDef {
        name: "or-echo",
        package: "orx-echo-lv2",
        lib: "or_echo",
        tests: &["orx-echo"],
        plugins: &[Entry::of::<orx_echo::Mono>(), Entry::of::<orx_echo::Stereo>()],
    },
    BundleDef {
        name: "or-plate",
        package: "orx-plate-lv2",
        lib: "or_plate",
        tests: &["orx-plate"],
        plugins: &[Entry::of::<orx_plate::Plate>()],
    },
    BundleDef {
        name: "or-eq",
        package: "orx-eq-lv2",
        lib: "or_eq",
        tests: &["orx-eq"],
        plugins: &[Entry::of::<orx_eq::Parametric>(), Entry::of::<orx_eq::Graphic>()],
    },
    BundleDef {
        name: "or-drive",
        package: "orx-drive-lv2",
        lib: "or_drive",
        tests: &["orx-drive"],
        plugins: &[Entry::of::<orx_drive::Drive>()],
    },
    BundleDef {
        name: "or-gate",
        package: "orx-gate-lv2",
        lib: "or_gate",
        tests: &["orx-gate"],
        plugins: &[Entry::of::<orx_gate::Gate>()],
    },
    BundleDef {
        name: "or-comp",
        package: "orx-comp-lv2",
        lib: "or_comp",
        tests: &["orx-comp"],
        plugins: &[Entry::of::<orx_comp::Comp>()],
    },
    BundleDef {
        name: "or-green-drive",
        package: "orx-green-drive-lv2",
        lib: "or_green_drive",
        tests: &["orx-green-drive"],
        plugins: &[Entry::of::<orx_green_drive::GreenDrive>()],
    },
    BundleDef {
        name: "or-black-dist",
        package: "orx-black-dist-lv2",
        lib: "or_black_dist",
        tests: &["orx-black-dist"],
        plugins: &[Entry::of::<orx_black_dist::BlackDist>()],
    },
    BundleDef {
        name: "or-drop",
        package: "orx-drop-lv2",
        lib: "or_drop",
        tests: &["orx-drop"],
        plugins: &[Entry::of::<orx_drop::DropPedal>()],
    },
    BundleDef {
        name: "or-splinter",
        package: "orx-splinter-lv2",
        lib: "or_splinter",
        tests: &["orx-splinter"],
        plugins: &[Entry::of::<orx_splinter::Splinter>()],
    },
];

/// Every plugin of the suite
pub fn plugins() -> impl Iterator<Item = &'static Entry> {
    BUNDLES.iter().flat_map(|b| b.plugins)
}

/// A plugin by URI or by slug (last URI segment, e.g. `tremolo-stereo`)
pub fn find(name: &str) -> Option<&'static Entry> {
    plugins().find(|e| e.info.uri == name || e.slug() == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn plugins_are_valid_and_unique() {
        let mut uris = HashSet::new();
        let mut slugs = HashSet::new();
        for e in plugins() {
            orx_plugin::validate::assert_valid(e.info);
            assert!(uris.insert(e.info.uri), "{} twice", e.info.uri);
            assert!(slugs.insert(e.slug()), "slug {} twice", e.slug());
        }
        assert_eq!(find("tremolo-stereo").map(|e| e.info.uri), Some("https://openrig.eu/plugins/tremolo-stereo"));
    }

    #[test]
    fn bundle_names_follow_the_convention() {
        for b in BUNDLES {
            assert!(b.name.starts_with("or-") && b.lib.starts_with("or_"), "{}", b.name);
            assert_eq!(b.name.replace('-', "_"), b.lib);
            assert!(!b.tests.is_empty());
        }
    }
}
