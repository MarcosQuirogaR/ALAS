#![doc = "Print the four conceptual payload-range points for each registered preset."]

use alas_config::{presets, AlasConfig};
use alas_pipeline::FullAnalysis;
use alas_report::families::performance::figure_payload_range;
use alas_report::scene::SceneElement;

fn main() {
    println!("preset,point");
    for name in presets::available() {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
            .unwrap_or_else(|error| panic!("{name}: configuration: {error}"));
        let preset = presets::get(name).unwrap_or_else(|error| panic!("{name}: {error}"));
        let report = FullAnalysis::new(config.clone())
            .run(&preset.design_vector, true)
            .unwrap_or_else(|error| panic!("{name}: full analysis: {error}"));
        let scene = figure_payload_range(&report, &config, None);
        for element in scene.elements {
            let SceneElement::Text { text, .. } = element else {
                continue;
            };
            if text.contains(" nm | ") {
                println!("{name},{}", text.replace('\n', "; ").replace(',', " "));
            }
        }
    }
}
