//! Every analysis exposed as a `simorph_core::Component`.

use simorph_core::*;
use simorph_plugin_sdk::{ClosureComponent, ComponentBuilder};

fn s_input<'a>(req: &'a TaskRequest, name: &str) -> Result<&'a str> {
    req.inputs
        .get(name)
        .and_then(|v| v.as_str())
        .ok_or_else(|| SimorphError::InvalidInput(format!("missing string input `{name}`")))
}

fn result_from(req: &TaskRequest, a: crate::asset::Analysis) -> Result<TaskResult> {
    let mut r = TaskResult::new(req, EvidenceLevel::Computed);
    for q in &a.quantities {
        r.outputs.insert(q.name.clone(), serde_json::json!(q.value));
    }
    r.outputs
        .insert("quantities".into(), serde_json::to_value(&a.quantities)?);
    r.outputs
        .insert("asset".into(), serde_json::to_value(&a.asset)?);
    r.outputs.insert("details".into(), a.details);
    Ok(r)
}

fn base(name: &str, domains: &[&str]) -> ComponentBuilder {
    let mut b = ComponentBuilder::new(name, "0.1.0", "simorph-multimodal")
        .capability(Capability::Other("media-analysis".into()))
        .manifest(|m| m.with_permissions(vec![Permission::ReadWorkspace]));
    for d in domains {
        b = b.domain(d);
    }
    b
}

pub fn image_component() -> Result<ClosureComponent> {
    base(
        "multimodal.image.analyse",
        &["imaging", "metrology", "multimodal"],
    )
    .input(Port::scalar("path", "1"))
    .input(Port::scalar("threshold", "1").with_range(0.0, 255.0))
    .input(Port::scalar("pixel_scale", "m"))
    .output(Port::scalar("width", "1"))
    .output(Port::scalar("height", "1"))
    .output(Port::scalar("object_count", "1"))
    .output(Port::scalar("largest_object_area", "m^2"))
    .output(Port::scalar("edge_fraction", "1"))
    .assumption("objects are segmented by a global luma threshold (8-connectivity)")
    .assumption("area uncertainty bounded by one pixel along the bounding-box perimeter")
    .build_raw(|req, _| {
        let opts = crate::image::ImageOptions {
            threshold: req.get_f64_or("threshold", 128.0),
            pixel_scale_m: Some(req.get_f64("pixel_scale")?),
            dark_objects: req
                .inputs
                .get("dark_objects")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            ..Default::default()
        };
        result_from(req, crate::image::analyse(s_input(req, "path")?, &opts)?)
    })
}

pub fn audio_component() -> Result<ClosureComponent> {
    base(
        "multimodal.audio.analyse",
        &["acoustics", "vibration", "multimodal"],
    )
    .input(Port::scalar("path", "1"))
    .output(Port::scalar("duration", "s"))
    .output(Port::scalar("rms", "1"))
    .output(Port::scalar("dominant_frequency", "Hz"))
    .assumption("full-scale normalised amplitude; dominant tone resolved within half an FFT bin")
    .build_raw(|req, _| result_from(req, crate::audio::analyse(s_input(req, "path")?)?))
}

pub fn video_component() -> Result<ClosureComponent> {
    base("multimodal.video.probe", &["imaging", "multimodal"])
        .input(Port::scalar("path", "1"))
        .output(Port::scalar("duration", "s"))
        .output(Port::scalar("frame_rate", "Hz"))
        .output(Port::scalar("width", "1"))
        .output(Port::scalar("height", "1"))
        .assumption("metadata as reported by ffprobe (external process)")
        .manifest(|m| m.with_permissions(vec![Permission::ReadWorkspace, Permission::SpawnProcess]))
        .build_raw(|req, _| result_from(req, crate::video::analyse(s_input(req, "path")?)?))
}

pub fn geometry_component() -> Result<ClosureComponent> {
    base(
        "multimodal.geometry.mass_properties",
        &["mechanical-engineering", "cad", "multimodal"],
    )
    .input(Port::scalar("path", "1"))
    .input(Port::scalar("density", "kg/m^3").with_range(0.0, 1e6))
    .output(Port::scalar("volume", "m^3"))
    .output(Port::scalar("surface_area", "m^2"))
    .output(Port::scalar("mass", "kg"))
    .output(Port::scalar("inertia_zz", "kg*m^2"))
    .output(Port::scalar("watertight", "1"))
    .assumption("homogeneous solid; meaningful only if the mesh is watertight (reported)")
    .build_raw(|req, _| {
        let unit = req
            .inputs
            .get("length_unit")
            .and_then(|v| v.as_str())
            .unwrap_or("m")
            .to_string();
        result_from(
            req,
            crate::geometry::analyse(s_input(req, "path")?, &unit, req.get_f64("density")?)?,
        )
    })
}

pub fn text_component() -> Result<ClosureComponent> {
    base("multimodal.text.extract_quantities", &["nlp", "multimodal"])
        .input(Port::scalar("text", "1"))
        .output(Port::scalar("count", "1"))
        .assumption(
            "rule-based extraction: numbers + validated unit expressions; no semantic parsing",
        )
        .build(|req, _| {
            let ex = crate::text::extract_quantities(s_input(req, "text")?, None);
            Ok([
                ("count".to_string(), serde_json::json!(ex.len())),
                ("quantities".to_string(), serde_json::to_value(&ex)?),
            ]
            .into_iter()
            .collect())
        })
}

/// All multimodal components.
pub fn all() -> Result<Vec<ClosureComponent>> {
    Ok(vec![
        image_component()?,
        audio_component()?,
        video_component()?,
        geometry_component()?,
        text_component()?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn components_run_through_the_contract() {
        let all = all().unwrap();
        for c in &all {
            c.manifest().validate_structure().unwrap();
        }
        let d = std::env::temp_dir().join(format!("simorph-mm-comp-{}", RunId::new()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("plate.stl");
        std::fs::write(
            &p,
            crate::geometry::box_mesh(0.1, 0.2, 0.01).to_ascii_stl("plate"),
        )
        .unwrap();
        let g = geometry_component().unwrap();
        let r = run_component(
            &g,
            &TaskRequest::new(g.manifest().id)
                .input("path", p.to_string_lossy().to_string())
                .input("density", 1000.0),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert!((r.get_f64("mass").unwrap() - 0.2).abs() < 1e-9);
        let t = text_component().unwrap();
        let r = run_component(
            &t,
            &TaskRequest::new(t.manifest().id)
                .input("text", "Une charge de 1,5 kN et une portée de 2 m."),
            &ExecutionControl::default(),
        )
        .unwrap();
        assert_eq!(r.get_f64("count").unwrap(), 2.0);
        // missing input is refused by the contract
        assert!(run_component(
            &g,
            &TaskRequest::new(g.manifest().id).input("density", 1.0),
            &ExecutionControl::default()
        )
        .is_err());
    }
}
