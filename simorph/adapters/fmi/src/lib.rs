//! # simorph-adapter-fmi
//!
//! Functional Mock-up Interface support.
//!
//! * [`parse_model_description`] reads `modelDescription.xml` (FMI 2.0 or
//!   3.0) and produces a [`ComponentManifest`]: input/output ports from
//!   variable causality, units resolved through `simorph-units` (unit
//!   definitions with `BaseUnit` exponents are converted to SI expressions),
//!   capability `CosimulationSlave`, execution interface `Fmu`.
//! * [`FmuComponent`] exposes an FMU file as a component.  Stepping is
//!   delegated to [FMPy](https://github.com/CATIA-Systems/FMPy) through the
//!   Python bridge: a small driver script is generated and run under the
//!   sandbox policy.  When `fmpy` is not importable the component returns
//!   `SimorphError::NotImplemented` with the exact reason; SIMORPH never
//!   fabricates FMU results.
//!
//! Native (in-process, `dlopen`) FMU execution is a documented follow-up;
//! it needs the platform binaries inside the FMU and loses the process
//! isolation that the bridge provides.

use indexmap::IndexMap;
use simorph_adapter_python_bridge::{python_interpreter, PythonComponent};
use simorph_core::{
    Capability, Component, ComponentManifest, DeterministicMode, ExecutionControl,
    ExecutionInterface, Port, Result, SimorphError, TaskRequest, TaskResult, UncertaintyModel,
};
use simorph_security::SandboxPolicy;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Variable extracted from the model description.
#[derive(Debug, Clone, PartialEq)]
pub struct FmiVariable {
    pub name: String,
    pub value_reference: u32,
    pub causality: String,
    pub variability: String,
    pub type_name: String,
    pub unit: Option<String>,
    pub start: Option<String>,
    pub description: Option<String>,
}

/// Parsed model description.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelDescription {
    pub fmi_version: String,
    pub model_name: String,
    pub guid: String,
    pub generation_tool: Option<String>,
    pub cosimulation: bool,
    pub model_exchange: bool,
    pub can_get_set_state: bool,
    pub variables: Vec<FmiVariable>,
    /// FMI unit name -> simorph-units expression.
    pub units: IndexMap<String, String>,
    /// Units that could not be mapped.
    pub unmapped_units: Vec<String>,
}

/// Read `modelDescription.xml` from an FMU archive.
pub fn read_model_description_xml(fmu: &Path) -> Result<String> {
    let file = std::fs::File::open(fmu)?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| SimorphError::InvalidInput(format!("not a zip/FMU: {e}")))?;
    let mut entry = archive
        .by_name("modelDescription.xml")
        .map_err(|_| SimorphError::InvalidInput("FMU has no modelDescription.xml".into()))?;
    let mut s = String::new();
    entry.read_to_string(&mut s)?;
    Ok(s)
}

/// Build a simorph-units expression from FMI `BaseUnit` exponents and factor.
fn base_unit_expression(node: roxmltree::Node) -> Option<String> {
    let mut parts = vec![];
    for (attr, sym) in [
        ("kg", "kg"),
        ("m", "m"),
        ("s", "s"),
        ("A", "A"),
        ("K", "K"),
        ("mol", "mol"),
        ("cd", "cd"),
    ] {
        if let Some(v) = node.attribute(attr) {
            let e: i32 = v.parse().ok()?;
            if e == 1 {
                parts.push(sym.to_string());
            } else if e != 0 {
                parts.push(format!("{sym}^{e}"));
            }
        }
    }
    let factor: f64 = node
        .attribute("factor")
        .and_then(|f| f.parse().ok())
        .unwrap_or(1.0);
    let offset: f64 = node
        .attribute("offset")
        .and_then(|f| f.parse().ok())
        .unwrap_or(0.0);
    if offset != 0.0 {
        // affine units (°C, °F) are matched by name only
        return None;
    }
    let mut expr = if parts.is_empty() {
        "1".to_string()
    } else {
        parts.join("*")
    };
    if (factor - 1.0).abs() > 1e-15 {
        expr = format!("{factor}*{expr}");
    }
    Some(expr)
}

/// Parse a model description (FMI 2.0 or 3.0).
pub fn parse_model_description(xml: &str) -> Result<ModelDescription> {
    let doc = roxmltree::Document::parse(xml)
        .map_err(|e| SimorphError::InvalidInput(format!("invalid modelDescription.xml: {e}")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "fmiModelDescription" {
        return Err(SimorphError::InvalidInput(format!(
            "root element is `{}`, expected fmiModelDescription",
            root.tag_name().name()
        )));
    }
    let fmi_version = root.attribute("fmiVersion").unwrap_or("").to_string();
    let major = fmi_version.chars().next().unwrap_or('0');
    if major != '2' && major != '3' {
        return Err(SimorphError::InvalidInput(format!(
            "unsupported fmiVersion `{fmi_version}` (2.x and 3.x supported)"
        )));
    }
    let model_name = root.attribute("modelName").unwrap_or("unnamed").to_string();
    let guid = root
        .attribute("guid")
        .or_else(|| root.attribute("instantiationToken"))
        .unwrap_or("")
        .to_string();
    let generation_tool = root.attribute("generationTool").map(str::to_string);
    let cs = root
        .children()
        .find(|n| n.tag_name().name() == "CoSimulation");
    let me = root
        .children()
        .find(|n| n.tag_name().name() == "ModelExchange");
    let can_get_set_state = cs
        .or(me)
        .and_then(|n| {
            n.attribute("canGetAndSetFMUstate")
                .or_else(|| n.attribute("canGetAndSetFMUState"))
        })
        .map(|v| v == "true")
        .unwrap_or(false);

    // Units
    let mut units = IndexMap::new();
    let mut unmapped = vec![];
    if let Some(defs) = root
        .children()
        .find(|n| n.tag_name().name() == "UnitDefinitions")
    {
        for u in defs.children().filter(|n| n.tag_name().name() == "Unit") {
            let name = u.attribute("name").unwrap_or("").to_string();
            if name.is_empty() {
                continue;
            }
            let expr = if simorph_units::parse_unit(&name).is_ok() {
                Some(name.clone())
            } else {
                u.children()
                    .find(|n| n.tag_name().name() == "BaseUnit")
                    .and_then(base_unit_expression)
                    .filter(|e| simorph_units::parse_unit(e).is_ok())
            };
            match expr {
                Some(e) => {
                    units.insert(name, e);
                }
                None => unmapped.push(name),
            }
        }
    }

    // Variables
    let mut variables = vec![];
    if let Some(mv) = root
        .children()
        .find(|n| n.tag_name().name() == "ModelVariables")
    {
        for v in mv.children().filter(|n| n.is_element()) {
            let (type_name, unit, start) = if major == '2' {
                if v.tag_name().name() != "ScalarVariable" {
                    continue;
                }
                let t = v.children().find(|c| c.is_element());
                match t {
                    Some(t) => (
                        t.tag_name().name().to_string(),
                        t.attribute("unit").map(str::to_string),
                        t.attribute("start").map(str::to_string),
                    ),
                    None => continue,
                }
            } else {
                (
                    v.tag_name().name().to_string(),
                    v.attribute("unit").map(str::to_string),
                    v.attribute("start").map(str::to_string),
                )
            };
            let name = match v.attribute("name") {
                Some(n) => n.to_string(),
                None => continue,
            };
            let value_reference = v
                .attribute("valueReference")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            variables.push(FmiVariable {
                name,
                value_reference,
                causality: v.attribute("causality").unwrap_or("local").to_string(),
                variability: v
                    .attribute("variability")
                    .unwrap_or("continuous")
                    .to_string(),
                type_name,
                unit,
                start,
                description: v.attribute("description").map(str::to_string),
            });
        }
    }
    // unit names used directly by variables but not declared
    for v in &variables {
        if let Some(u) = &v.unit {
            if !units.contains_key(u) && !unmapped.contains(u) {
                if simorph_units::parse_unit(u).is_ok() {
                    units.insert(u.clone(), u.clone());
                } else {
                    unmapped.push(u.clone());
                }
            }
        }
    }
    Ok(ModelDescription {
        fmi_version,
        model_name,
        guid,
        generation_tool,
        cosimulation: cs.is_some(),
        model_exchange: me.is_some(),
        can_get_set_state,
        variables,
        units,
        unmapped_units: unmapped,
    })
}

fn is_numeric(t: &str) -> bool {
    matches!(
        t,
        "Real"
            | "Integer"
            | "Float64"
            | "Float32"
            | "Int8"
            | "Int16"
            | "Int32"
            | "Int64"
            | "UInt8"
            | "UInt16"
            | "UInt32"
            | "UInt64"
            | "Boolean"
            | "Enumeration"
    )
}

impl ModelDescription {
    fn port(&self, v: &FmiVariable) -> Port {
        let unit = v
            .unit
            .as_ref()
            .and_then(|u| self.units.get(u).cloned())
            .unwrap_or_else(|| "1".to_string());
        let mut p = Port::scalar(&v.name, unit);
        p.description = v.description.clone();
        p
    }

    /// Manifest for an FMU at `path`.
    pub fn to_manifest(&self, path: &Path) -> ComponentManifest {
        let inputs: Vec<Port> = self
            .variables
            .iter()
            .filter(|v| {
                (v.causality == "input" || v.causality == "parameter") && is_numeric(&v.type_name)
            })
            .map(|v| self.port(v))
            .collect();
        let outputs: Vec<Port> = self
            .variables
            .iter()
            .filter(|v| v.causality == "output" && is_numeric(&v.type_name))
            .map(|v| self.port(v))
            .collect();
        let mut caps = vec![];
        if self.cosimulation {
            caps.push(Capability::CosimulationSlave);
        }
        if self.model_exchange {
            caps.push(Capability::OdeIntegration);
        }
        let mut assumptions = vec![format!(
            "FMI {} model `{}` (guid {})",
            self.fmi_version, self.model_name, self.guid
        )];
        if let Some(t) = &self.generation_tool {
            assumptions.push(format!("generated by {t}"));
        }
        if !self.unmapped_units.is_empty() {
            assumptions.push(format!(
                "units not mapped to SI (ports declared dimensionless): {}",
                self.unmapped_units.join(", ")
            ));
        }
        let mut m = ComponentManifest::in_process(
            &format!("fmi.{}", self.model_name),
            &self.fmi_version,
            "fmu",
        )
        .with_capabilities(caps)
        .with_domains(&["cosimulation"])
        .with_inputs(inputs)
        .with_outputs(outputs);
        m.assumptions = assumptions;
        m.license = "see FMU".into();
        m.uncertainty_model = UncertaintyModel::None;
        m.deterministic_mode = DeterministicMode::Conditional {
            description: "deterministic when the FMU binary is".into(),
        };
        m.execution_interface = ExecutionInterface::Fmu {
            path: path.to_string_lossy().to_string(),
            fmi_version: self.fmi_version.clone(),
        };
        m.provenance.source_repository = Some(path.to_string_lossy().to_string());
        m.provenance.generated_by = self.generation_tool.clone();
        m.dependencies = vec!["fmpy".into()];
        // canGetAndSetFMUstate is what iterative co-simulation masters need
        if self.can_get_set_state {
            m.assumptions
                .push("supports get/set FMU state (rollback-capable)".into());
        }
        m
    }
}

/// Python driver speaking the SIMORPH protocol around `fmpy.simulate_fmu`.
/// It echoes the manifest written by Rust (argv[2]) and runs the FMU
/// (argv[1]) with the request inputs as start values.
const DRIVER: &str = r#"
import json, sys
fmu_path, manifest_path = sys.argv[1], sys.argv[2]
manifest = json.load(open(manifest_path))
try:
    import fmpy
    HAVE = True
except Exception as e:
    HAVE = False
    REASON = repr(e)
outputs = [p["name"] for p in manifest["output_schema"]]
for line in sys.stdin:
    msg = json.loads(line)
    t = msg["type"]
    if t == "manifest":
        print(json.dumps({"type": "manifest", "manifest": manifest}), flush=True)
    elif t == "execute":
        r = msg["request"]
        if not HAVE:
            print(json.dumps({"type": "error", "message": "fmpy not importable: " + REASON, "run_id": r["run_id"]}), flush=True)
            continue
        cfg = r.get("config") or {}
        inputs = {k: v for k, v in r["inputs"].items() if isinstance(v, (int, float))}
        try:
            res = fmpy.simulate_fmu(fmu_path, start_values=inputs,
                                    start_time=float(cfg.get("start_time", 0.0)),
                                    stop_time=float(cfg.get("stop_time", 1.0)),
                                    output_interval=cfg.get("output_interval"),
                                    output=outputs or None)
            out = {}
            names = res.dtype.names
            out["time"] = res["time"].tolist()
            for n in outputs:
                if n in names:
                    out[n] = float(res[n][-1])
                    out[n + "_series"] = res[n].tolist()
            print(json.dumps({"type": "result", "result": {"run_id": r["run_id"], "component": r["component"],
                "outputs": out, "metrics": {"samples": float(len(res))}, "evidence": "simulated",
                "request_hash": "", "warnings": [], "wall_time_ms": 0}}), flush=True)
        except Exception as e:
            print(json.dumps({"type": "error", "message": repr(e), "run_id": r["run_id"]}), flush=True)
    elif t == "cancel":
        print(json.dumps({"type": "ack"}), flush=True)
    elif t == "shutdown":
        print(json.dumps({"type": "ack"}), flush=True)
        break
"#;

/// Returns Ok(()) when `fmpy` is importable by the configured interpreter.
pub fn fmpy_available() -> std::result::Result<(), String> {
    let out = std::process::Command::new(python_interpreter())
        .args(["-I", "-c", "import fmpy"])
        .output()
        .map_err(|e| format!("cannot run {}: {e}", python_interpreter()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "fmpy is not installed for {} (pip install fmpy): {}",
            python_interpreter(),
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .last()
                .unwrap_or("")
        ))
    }
}

/// An FMU exposed as a component.
pub struct FmuComponent {
    path: PathBuf,
    description: ModelDescription,
    manifest: ComponentManifest,
    policy: SandboxPolicy,
    driver: OnceLock<std::result::Result<PythonComponent, String>>,
}

impl std::fmt::Debug for FmuComponent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FmuComponent")
            .field("path", &self.path)
            .field("model", &self.description.model_name)
            .finish()
    }
}

impl FmuComponent {
    pub fn open(path: impl AsRef<Path>, policy: SandboxPolicy) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let xml = read_model_description_xml(&path)?;
        let description = parse_model_description(&xml)?;
        let manifest = description.to_manifest(&path);
        manifest.validate_structure()?;
        Ok(Self {
            path,
            description,
            manifest,
            policy,
            driver: OnceLock::new(),
        })
    }

    pub fn description(&self) -> &ModelDescription {
        &self.description
    }

    fn driver(&self) -> std::result::Result<&PythonComponent, String> {
        self.driver
            .get_or_init(|| {
                fmpy_available()?;
                let dir =
                    std::env::temp_dir().join(format!("simorph-fmu-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let script = dir.join("fmu_driver.py");
                let manifest_path = dir.join("manifest.json");
                std::fs::write(&script, DRIVER).map_err(|e| e.to_string())?;
                std::fs::write(
                    &manifest_path,
                    serde_json::to_vec(&self.manifest).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                let policy = self.policy.clone().with_working_dir(&dir);
                PythonComponent::from_script_with(
                    python_interpreter(),
                    &script,
                    &[
                        &self.path.to_string_lossy(),
                        &manifest_path.to_string_lossy(),
                    ],
                    policy,
                )
                .map_err(|e| e.to_string())
            })
            .as_ref()
            .map_err(|e| e.clone())
    }
}

impl Component for FmuComponent {
    fn manifest(&self) -> &ComponentManifest {
        &self.manifest
    }
    fn execute(&self, req: &TaskRequest, ctl: &ExecutionControl) -> Result<TaskResult> {
        let driver = self.driver().map_err(|reason| {
            SimorphError::NotImplemented(format!(
                "FMU stepping for `{}` requires FMPy through the Python bridge: {reason}",
                self.path.display()
            ))
        })?;
        driver.execute(req, ctl)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const FMI2: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<fmiModelDescription fmiVersion="2.0" modelName="ThermalMass" guid="{abc}" generationTool="handwritten">
  <CoSimulation modelIdentifier="ThermalMass" canGetAndSetFMUstate="true"/>
  <UnitDefinitions>
    <Unit name="K"><BaseUnit K="1"/></Unit>
    <Unit name="W"><BaseUnit kg="1" m="2" s="-3"/></Unit>
    <Unit name="kWh"><BaseUnit kg="1" m="2" s="-2" factor="3600000"/></Unit>
    <Unit name="degC"><BaseUnit K="1" offset="273.15"/></Unit>
    <Unit name="widgets"/>
    <Unit name="J/K"><BaseUnit kg="1" m="2" s="-2" K="-1"/></Unit>
  </UnitDefinitions>
  <ModelVariables>
    <ScalarVariable name="heat_in" valueReference="0" causality="input"><Real unit="W" start="0"/></ScalarVariable>
    <ScalarVariable name="capacity" valueReference="1" causality="parameter" variability="fixed"><Real unit="J/K" start="1000"/></ScalarVariable>
    <ScalarVariable name="T" valueReference="2" causality="output"><Real unit="K"/></ScalarVariable>
    <ScalarVariable name="T_c" valueReference="3" causality="output"><Real unit="degC"/></ScalarVariable>
    <ScalarVariable name="label" valueReference="4" causality="output"><String/></ScalarVariable>
    <ScalarVariable name="internal" valueReference="5" causality="local"><Real/></ScalarVariable>
  </ModelVariables>
</fmiModelDescription>"#;

    const FMI3: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<fmiModelDescription fmiVersion="3.0" modelName="Battery" instantiationToken="{tok}">
  <CoSimulation modelIdentifier="Battery" canGetAndSetFMUState="false"/>
  <ModelExchange modelIdentifier="Battery"/>
  <UnitDefinitions>
    <Unit name="A"><BaseUnit A="1"/></Unit>
    <Unit name="V"><BaseUnit kg="1" m="2" s="-3" A="-1"/></Unit>
  </UnitDefinitions>
  <ModelVariables>
    <Float64 name="current" valueReference="1" causality="input" unit="A" start="0"/>
    <Float64 name="voltage" valueReference="2" causality="output" unit="V"/>
    <Float64 name="soc" valueReference="3" causality="output"/>
    <Int32 name="cycles" valueReference="4" causality="output"/>
  </ModelVariables>
</fmiModelDescription>"#;

    fn write_fmu(xml: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("simorph-fmi-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("model.fmu");
        let f = std::fs::File::create(&path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        z.start_file("modelDescription.xml", opts).unwrap();
        z.write_all(xml.as_bytes()).unwrap();
        z.start_file("binaries/linux64/model.so", opts).unwrap();
        z.write_all(b"not a real binary").unwrap();
        z.finish().unwrap();
        path
    }

    #[test]
    fn fmi2_ports_and_units() {
        let path = write_fmu(FMI2);
        let c = FmuComponent::open(&path, SandboxPolicy::strict()).unwrap();
        let d = c.description();
        assert_eq!(d.fmi_version, "2.0");
        assert!(d.cosimulation && !d.model_exchange && d.can_get_set_state);
        assert_eq!(d.units["W"], "W");
        assert_eq!(d.units["J/K"], "J/K");
        assert_eq!(d.units["kWh"], "kWh");
        assert_eq!(d.unmapped_units, vec!["widgets".to_string()]);
        let m = c.manifest();
        assert_eq!(m.name, "fmi.ThermalMass");
        let in_names: Vec<&str> = m.input_schema.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(in_names, vec!["heat_in", "capacity"]);
        let out_names: Vec<&str> = m.output_schema.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            out_names,
            vec!["T", "T_c"],
            "string and local variables are not ports"
        );
        assert_eq!(m.units["heat_in"], "W");
        assert_eq!(m.units["capacity"], "J/K");
        assert_eq!(m.units["T_c"], "degC");
        simorph_units::check_same_dimension(&m.units["T"], &m.units["T_c"]).unwrap();
        assert!(m.has_capability(&Capability::CosimulationSlave));
        assert!(
            matches!(m.execution_interface, ExecutionInterface::Fmu { ref fmi_version, .. } if fmi_version == "2.0")
        );
    }

    #[test]
    fn fmi3_ports_and_units() {
        let path = write_fmu(FMI3);
        let c = FmuComponent::open(&path, SandboxPolicy::strict()).unwrap();
        let d = c.description();
        assert_eq!(d.guid, "{tok}");
        assert!(d.cosimulation && d.model_exchange);
        let m = c.manifest();
        assert_eq!(m.units["current"], "A");
        assert_eq!(m.units["voltage"], "V");
        assert_eq!(m.units["soc"], "1");
        assert!(m.output_schema.iter().any(|p| p.name == "cycles"));
        assert!(m.has_capability(&Capability::OdeIntegration));
        // BaseUnit-derived V is dimensionally a volt
        simorph_units::check_same_dimension(&d.units["V"], "V").unwrap();
    }

    #[test]
    fn invalid_inputs_rejected() {
        assert!(parse_model_description("<nope>").is_err());
        assert!(parse_model_description("<root/>").is_err());
        assert!(parse_model_description(
            r#"<fmiModelDescription fmiVersion="1.0" modelName="x"/>"#
        )
        .is_err());
        let dir = std::env::temp_dir().join(format!("simorph-fmi-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let not_zip = dir.join("x.fmu");
        std::fs::write(&not_zip, b"hello").unwrap();
        assert!(FmuComponent::open(&not_zip, SandboxPolicy::strict()).is_err());
    }

    #[test]
    fn execution_without_fmpy_is_not_implemented_with_reason() {
        let path = write_fmu(FMI3);
        let c = FmuComponent::open(&path, SandboxPolicy::strict().with_timeout(30.0)).unwrap();
        let req = TaskRequest::new(c.manifest().id).input("current", 1.0);
        let r = simorph_core::run_component(&c, &req, &ExecutionControl::default());
        match fmpy_available() {
            Err(_) => match r {
                Err(SimorphError::NotImplemented(m)) => assert!(m.contains("fmpy"), "{m}"),
                other => panic!("expected NotImplemented, got {other:?}"),
            },
            Ok(()) => {
                // fmpy present but the FMU binary is fake: a component error, never a fabricated result
                assert!(r.is_err());
            }
        }
    }
}
