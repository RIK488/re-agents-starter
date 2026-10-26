//! Runs the nanosatellite co-simulation demo for one orbit and prints signal ranges.
use simorph_cosimulation::{run, MasterAlgorithm, MasterOptions};
use simorph_plugin_physics::demo;
use simorph_plugin_physics::orbit::{TwoBody, MU_EARTH, R_EARTH};
fn main() {
    let np = demo::NanosatParams::default();
    let mut c = demo::nanosat_coupling(&np).unwrap();
    println!(
        "order: {:?} loops: {:?}",
        c.order
            .iter()
            .map(|&i| c.slaves[i].name.clone())
            .collect::<Vec<_>>(),
        c.algebraic_loops
    );
    let period = TwoBody::circular_period(R_EARTH + np.altitude, MU_EARTH);
    let tr = run(
        &mut c,
        &MasterOptions::new(MasterAlgorithm::GaussSeidel, period, 10.0),
        &Default::default(),
    )
    .unwrap();
    for name in [
        "solar.power",
        "thermal.temperature_0",
        "thermal.temperature_1",
        "battery.soc",
        "battery.voltage",
    ] {
        let s = tr.signal(name).unwrap();
        let (mn, mx) = s
            .iter()
            .fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        println!(
            "{name}: min {mn:.3} max {mx:.3} last {:.3}",
            s.last().unwrap()
        );
    }
}
