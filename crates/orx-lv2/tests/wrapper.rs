//! The LV2 wrapper driven through its C entry points by the mini host.

use orx_lv2::host::Host;
use orx_plugin::view::StreamContent;
use orx_plugin::{
    Class, Config, Context, EventKind, Meter, Param, Plugin, PluginInfo, Port, StateError, StateSink, StateSource,
    StateValue, Stream, Version,
};

orx_test::install_rt_check!();

/// Stereo gain with a tempo meter, a stream, state, and a panic switch
struct Probe {
    bpm: f32,
    midi: f32,
    name: String,
    calls: u32,
}

const INFO: PluginInfo = PluginInfo {
    uri: "https://openrig.eu/plugins/test-probe",
    name: "Probe",
    description: "Test plugin",
    class: Class::Utility,
    version: Version::parse("0.2"),
    license: "GPL-3.0-or-later",
    ports: &[
        Port::audio_in("in_l", "In L"),
        Port::audio_in("in_r", "In R"),
        Port::audio_out("out_l", "Out L"),
        Port::audio_out("out_r", "Out R"),
        Port::param("gain", "Gain", Param::float(0.5, 0.0, 1.0)),
        Port::param("panic", "Panic", Param::toggle(false)),
        Port::meter("bpm", "BPM", Meter::new(0.0, 300.0)),
        Port::meter("midi", "MIDI", Meter::new(0.0, 127.0)),
        Port::events("events", "Events", true, true),
        Port::stream(
            "scope",
            "Scope",
            Stream { content: StreamContent::LevelHistory, length: 3, channels: 2, max_rate: 30.0 },
        ),
        Port::enabled(),
        Port::latency(),
    ],
    presets: &[],
    view: None,
    face: orx_plugin::view::Face::basic("A test", &[]),
    hard_rt: true,
};

impl Plugin for Probe {
    const INFO: &'static PluginInfo = &INFO;
    const HAS_STATE: bool = true;

    fn new(_: &Config) -> Self {
        Self { bpm: 0.0, midi: 0.0, name: String::from("init"), calls: 0 }
    }

    fn reset(&mut self) {}

    fn process(&mut self, ctx: &mut Context<'_>) {
        assert!(!ctx.params.on(5), "panic switch");
        let g = ctx.params.get(4);
        for k in 0..2 {
            for (o, x) in ctx.outputs[k].iter_mut().zip(ctx.inputs[k]) {
                *o = x * g;
            }
        }
        for e in ctx.events {
            if let EventKind::Midi { data, .. } = e.kind {
                self.midi = f32::from(data[1]);
            }
        }
        self.bpm = ctx.position.bpm.unwrap_or(0.0);
        ctx.meters.set(6, self.bpm);
        ctx.meters.set(7, self.midi);
        self.calls += 1;
        if ctx.streams.due(9) {
            ctx.streams.send(9, 3, 0.25, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        }
    }

    fn save_state(&self, sink: &mut dyn StateSink) {
        sink.put("name", StateValue::Str(&self.name));
        sink.put("calls", StateValue::I64(i64::from(self.calls)));
    }

    fn load_state(&mut self, source: &dyn StateSource) -> Result<(), StateError> {
        match source.get("name") {
            Some(StateValue::Str(s)) => self.name = s.to_string(),
            _ => return Err(StateError::Missing("name")),
        }
        Ok(())
    }
}

fn ramp(n: usize, k: usize) -> impl Iterator<Item = f32> {
    (0..n).map(move |i| ((i * (k + 1)) as f32 * 0.01).sin())
}

#[test]
fn processes_through_the_c_abi() {
    let d = orx_lv2::descriptor::<Probe>();
    let host = Host::new(48_000.0, 256);
    // SAFETY: descriptor of Probe, described by INFO
    let mut p = unsafe { host.instantiate(&d.raw, &INFO, false) }.unwrap();
    p.activate();
    for k in 0..2 {
        p.input(k).iter_mut().zip(ramp(256, k)).for_each(|(b, x)| *b = x);
    }
    p.push_position(10, 133.0, 1.0);
    p.push_midi(20, &[0xb0, 42, 127]);
    orx_test::rt::no_alloc(|| p.run(256));
    for k in 0..2 {
        let out = p.output(k).to_vec();
        for (y, x) in out.iter().zip(ramp(256, k)) {
            assert_eq!(*y, x * 0.5);
        }
    }
    assert_eq!(p.get(6), 133.0);
    assert_eq!(p.get(7), 42.0);
    assert_eq!(p.get(11), 0.0);
    let frames = p.frames(9);
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].time, 3);
    assert_eq!(frames[0].channels, 2);
    assert_eq!(frames[0].position, 0.25);
    assert_eq!(frames[0].values, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);

    // Rate limit: no new frame on the next run
    orx_test::rt::no_alloc(|| p.run(256));
    assert!(p.frames(9).is_empty());
    // The transport persists
    assert_eq!(p.get(6), 133.0);
}

#[test]
fn in_place_matches_separate_buffers() {
    let d = orx_lv2::descriptor::<Probe>();
    let host = Host::new(48_000.0, 128);
    // SAFETY: as above
    let mut a = unsafe { host.instantiate(&d.raw, &INFO, false) }.unwrap();
    // SAFETY: as above
    let mut b = unsafe { host.instantiate(&d.raw, &INFO, true) }.unwrap();
    for p in [&mut a, &mut b] {
        p.activate();
        p.set(4, 0.8);
        for k in 0..2 {
            p.input(k).iter_mut().zip(ramp(128, k)).for_each(|(b, x)| *b = x);
        }
        orx_test::rt::no_alloc(|| p.run(128));
    }
    for k in 0..2 {
        assert_eq!(a.output(k).to_vec(), b.output(k).to_vec());
    }
}

#[test]
fn hostile_control_values_are_sanitised() {
    let d = orx_lv2::descriptor::<Probe>();
    let host = Host::new(48_000.0, 64);
    // SAFETY: as above
    let mut p = unsafe { host.instantiate(&d.raw, &INFO, false) }.unwrap();
    p.activate();
    p.input(0).fill(1.0);
    for v in [f32::NAN, f32::INFINITY, -1e30, 7.0] {
        p.set(4, v);
        p.run(64);
        assert!(p.output(0).iter().all(|y| y.is_finite() && (0.0..=1.0).contains(y)), "gain {v}");
    }
}

#[test]
fn a_panic_passes_the_signal_through() {
    let d = orx_lv2::descriptor::<Probe>();
    let host = Host::new(48_000.0, 64);
    // SAFETY: as above
    let mut p = unsafe { host.instantiate(&d.raw, &INFO, false) }.unwrap();
    p.activate();
    p.input(0).fill(0.3);
    p.set(5, 1.0);
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    p.run(64);
    std::panic::set_hook(hook);
    assert!(p.output(0).iter().all(|y| *y == 0.3));
    // Stays in pass-through
    p.set(5, 0.0);
    p.run(64);
    assert!(p.output(0).iter().all(|y| *y == 0.3));
}

#[test]
fn state_round_trip() {
    let d = orx_lv2::descriptor::<Probe>();
    let host = Host::new(48_000.0, 64);
    // SAFETY: as above
    let mut p = unsafe { host.instantiate(&d.raw, &INFO, false) }.unwrap();
    p.activate();
    p.run(64);
    let saved = p.save().unwrap();
    let uri = |k: &str| format!("{}#{k}", INFO.uri);
    assert!(saved.contains(&(uri("name"), "http://lv2plug.in/ns/ext/atom#String".into(), b"init\0".to_vec())));
    assert!(saved.iter().any(|(k, t, b)| *k == uri("calls") && t.ends_with("#Long") && b.len() == 8));
    let mut other = saved.clone();
    other[0].2 = b"renamed\0".to_vec();
    assert_eq!(p.restore(&other), Some(0));
    assert!(p.save().unwrap().iter().any(|(_, _, b)| b == b"renamed\0"));
    // A missing key is reported
    assert_ne!(p.restore(&[]), Some(0));
}

#[test]
fn no_urid_map_no_instance() {
    let d = orx_lv2::descriptor::<Probe>();
    let instantiate = d.raw.instantiate.unwrap();
    // SAFETY: LV2 instantiate with an empty (null) feature list
    let h = unsafe { instantiate(&d.raw, 48_000.0, c"/".as_ptr(), std::ptr::null()) };
    assert!(h.is_null());
}

#[test]
fn turtle_mentions_every_port() {
    let ttl = orx_lv2::ttl::plugin(&orx_plugin::dynamic::Entry::of::<Probe>());
    for (i, port) in INFO.ports.iter().enumerate() {
        assert!(ttl.contains(&format!("lv2:index {i} ;\n        lv2:symbol \"{}\"", port.symbol)));
    }
    assert!(ttl.contains("lv2:hardRTCapable"));
    assert!(ttl.contains("lv2:requiredFeature urid:map"));
    assert!(ttl.contains("lv2:extensionData state:interface"));
    assert!(ttl.contains("lv2:designation lv2:latency"));
    assert!(orx_lv2::vocab::undefined_terms(&ttl).is_empty());
}
