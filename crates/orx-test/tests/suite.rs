//! The kit tested on a tiny plugin.

use orx_dsp::Smoother;
use orx_dsp::Smoothing;
use orx_plugin::{Class, Config, Context, Param, Plugin, PluginInfo, Port, Preset, Unit, Version};

orx_test::install_rt_check!();

struct Gain {
    gain: Smoother,
}

const INFO: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/test-gain",
    name: "Test gain",
    description: "",
    class: Class::Utility,
    version: Version::parse("0.2"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in", "In"),
        Port::audio_out("out", "Out"),
        Port::param(
            "gain",
            "Gain",
            Param::float(0.0, -24.0, 12.0).unit(Unit::Db).smooth(Smoothing::Linear { ms: 20.0 }),
        ),
        Port::enabled(),
    ],
    presets: &[Preset { name: "Boost", id: "boost", values: &[("gain", 6.0)] }],
    view: None,
    face: orx_plugin::view::Face::basic("A test", &[]),
    hard_rt: true,
};

impl Plugin for Gain {
    const INFO: &'static PluginInfo = &INFO;

    fn new(config: &Config) -> Self {
        Self { gain: Smoother::new(Smoothing::Linear { ms: 20.0 }, config.sample_rate as f32, 1.0) }
    }

    fn reset(&mut self) {
        let target = self.gain.target();
        self.gain.reset(target);
    }

    fn process(&mut self, ctx: &mut Context<'_>) {
        self.gain.set_target(orx_dsp::math::db_to_gain(ctx.params.get(2)));
        for (o, x) in ctx.outputs[0].iter_mut().zip(ctx.inputs[0]) {
            *o = x * self.gain.next();
        }
    }
}

#[test]
fn conformance() {
    orx_test::conformance::check::<Gain>();
}

#[test]
fn ports_lock() {
    let path = std::env::temp_dir().join(format!("orx-test-ports-{}.lock", std::process::id()));
    std::fs::write(&path, orx_test::ports_lock::render(&INFO)).unwrap();
    orx_test::ports_lock::check(&INFO, &path);
    // Renaming a locked port is caught
    let renamed = orx_test::ports_lock::render(&INFO).replace("2 gain", "2 level");
    std::fs::write(&path, renamed).unwrap();
    let caught = std::panic::catch_unwind(|| orx_test::ports_lock::check(&INFO, &path)).is_err();
    let _ = std::fs::remove_file(&path);
    assert!(caught);
}

#[test]
fn bench_reports() {
    let report = orx_test::bench::run::<Gain>(48_000.0, 128, &[]);
    assert!(report.ns_per_frame > 0.0 && report.realtime_percent < 100.0);
}

/// Allocates under the trap: run by `trap_catches_allocation` in a child
/// process, since it aborts
#[test]
#[ignore = "aborts on purpose; run by trap_catches_allocation"]
fn child_allocates() {
    orx_test::rt::no_alloc(|| std::hint::black_box(vec![1u8; 64]).len());
}

#[test]
fn trap_catches_allocation() {
    assert!(orx_test::rt::installed());
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_allocates", "--ignored", "--nocapture", "--test-threads=1"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("memory allocation on the audio path"), "{stderr}");
}
