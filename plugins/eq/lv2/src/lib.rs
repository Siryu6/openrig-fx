//! `or_eq.so`, the binary of `or-eq.lv2`. The Turtle files come from
//! `cargo xtask bundle`; the order here is the one of `orx_suite::BUNDLES`.

orx_lv2::export_lv2!(orx_eq::Parametric, orx_eq::Graphic);
