// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the embedded Spanish catalog against `alas.translations.es.CATALOG`,
//! dumped verbatim by `golden/generators/gen_i18n.py`.
//!
//! Keys are matched by exact English text, so this is `exact` tier rather
//! than a tolerance comparison: a translation one character off from the
//! Python source is exactly as wrong as one that is unrelated. Checked in
//! both directions -- every fixture entry present with the right value in the
//! shipped catalog, and nothing extra in the shipped catalog the fixture
//! doesn't know about -- because zipping the two together would let a length
//! mismatch (a key dropped from one side, or a stray extra key) hide behind
//! however many pairs happened to still agree.
//!
//! Ten source strings intentionally changed when the native mission replaced
//! the retired SUAVE runtime boundary. Their corrected Rust keys and values,
//! together with the frozen reference keys and values, are kept in the named
//! allowlist below. This keeps the exact tier while making the textual
//! deviation visible in both directions.

use std::collections::HashMap;

use alas_testkit::{Comparison, Tier};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Fixture {
    catalog: HashMap<String, String>,
}

#[derive(Clone, Copy)]
struct CatalogCorrection {
    corrected_key: &'static str,
    corrected_value: &'static str,
    frozen_key: &'static str,
    frozen_value: &'static str,
}

const SOURCE_CORRECTED_CATALOG: &[CatalogCorrection] = &[
    CatalogCorrection {
        corrected_key: "Chordwise panel resolution for the once-per-run final/reported analysis. A supercritical/cambered section needs ~8 chordwise panels for the VLM to resolve its camber line; at the coarse in-loop resolution the camber (and hence the zero-lift alpha) is under-captured, which inflates the reported cruise alpha by several degrees and under-predicts L/D by ~7%. Kept high here so the REPORTED cruise alpha (~1-4 deg) and L/D are physically accurate.",
        corrected_value: "Resoluci\u{00f3}n de paneles en cuerda para el an\u{00e1}lisis final que se ejecuta una vez por ejecuci\u{00f3}n. Un perfil supercr\u{00ed}tico o con curvatura necesita ~8 paneles en cuerda para que el VLM resuelva su l\u{00ed}nea de curvatura; con la resoluci\u{00f3}n gruesa del bucle la curvatura (y por tanto el alfa de sustentaci\u{00f3}n nula) queda infracapturada, lo que infla el alfa de crucero reportado en varios grados y subestima el L/D en torno a un 7 %. Se mantiene alta aqu\u{00ed} para que el alfa de crucero y el L/D REPORTADOS sean f\u{00ed}sicamente correctos.",
        frozen_key: "Chordwise panel resolution for the once-per-run final/reported analysis. A supercritical/cambered section needs ~8 chordwise panels for the VLM to resolve its camber line; at the coarse in-loop resolution the camber (and hence the zero-lift alpha) is under-captured, which inflates the reported cruise alpha by several degrees and under-predicts L/D by ~7%. Kept high here so the REPORTED cruise alpha (~1-4 deg, matching SUAVE) and L/D are physically accurate.",
        frozen_value: "Resoluci\u{00f3}n de paneles en cuerda para el an\u{00e1}lisis final que se ejecuta una vez por ejecuci\u{00f3}n. Un perfil supercr\u{00ed}tico o con curvatura necesita ~8 paneles en cuerda para que el VLM resuelva su l\u{00ed}nea de curvatura; con la resoluci\u{00f3}n gruesa del bucle la curvatura (y por tanto el alfa de sustentaci\u{00f3}n nula) queda infracapturada, lo que infla el alfa de crucero reportado en varios grados y subestima el L/D en torno a un 7 %. Se mantiene alta aqu\u{00ed} para que el alfa de crucero y el L/D REPORTADOS sean f\u{00ed}sicamente correctos.",
    },
    CatalogCorrection {
        corrected_key: "Fixed low-pressure-compressor (booster) pressure ratio; the high-pressure compressor makes up the rest of the overall (core) pressure ratio (HPC = OPR / this value). Matches the fixed mission LPC split.",
        corrected_value: "Relaci\u{00f3}n de presiones fija del compresor de baja presi\u{00f3}n (booster); el compresor de alta aporta el resto de la relaci\u{00f3}n global del n\u{00fa}cleo (HPC = OPR / este valor). Coincide con el reparto fijo de LPC de la misi\u{00f3}n.",
        frozen_key: "Fixed low-pressure-compressor (booster) pressure ratio; the high-pressure compressor makes up the rest of the overall (core) pressure ratio (HPC = OPR / this value). Matches SUAVE's fixed LPC split.",
        frozen_value: "Relaci\u{00f3}n de presiones fija del compresor de baja presi\u{00f3}n (booster); el compresor de alta aporta el resto de la relaci\u{00f3}n global del n\u{00fa}cleo (HPC = OPR / este valor). Coincide con el reparto fijo de LPC de SUAVE.",
    },
    CatalogCorrection {
        corrected_key: "Maximum rated sea-level-static take-off thrust, per engine. Drives propulsion mass, the Matching Chart T/W lookup, and the Native mission model turbofan sizing target.",
        corrected_value: "Empuje m\u{00e1}ximo de despegue a nivel del mar en est\u{00e1}tico, por motor. Determina la masa de propulsi\u{00f3}n, el valor de T/W del diagrama de adaptaci\u{00f3}n y el objetivo de dimensionado del turbof\u{00e1}n en Native mission model.",
        frozen_key: "Maximum rated sea-level-static take-off thrust, per engine. Drives propulsion mass, the Matching Chart T/W lookup, and the SUAVE turbofan sizing target.",
        frozen_value: "Empuje m\u{00e1}ximo de despegue a nivel del mar en est\u{00e1}tico, por motor. Determina la masa de propulsi\u{00f3}n, el valor de T/W del diagrama de adaptaci\u{00f3}n y el objetivo de dimensionado del turbof\u{00e1}n en SUAVE.",
    },
    CatalogCorrection {
        corrected_key: "Model Comparison -- shared variables across Native analysis / Native mission model / MSES",
        corrected_value: "Comparaci\u{00f3}n de modelos: variables comunes entre Native analysis, Native mission model y MSES",
        frozen_key: "Model Comparison -- shared variables across AeroSandbox / SUAVE / MSES",
        frozen_value: "Comparaci\u{00f3}n de modelos: variables comunes entre AeroSandbox, SUAVE y MSES",
    },
    CatalogCorrection {
        corrected_key: "Number of alpha points in the MSES polar sweep. Kept small relative to the native VLM sweep (analysis.sweep_n_points) since each MSES point is a real viscous-compressible solve (~1-2s) rather than a linear-algebra VLM solve.",
        corrected_value: "N\u{00fa}mero de puntos de alfa en el barrido de polar de MSES. Se mantiene peque\u{00f1}o frente al barrido VLM nativo, ya que cada punto de MSES es una resoluci\u{00f3}n viscosa y compresible real (~1-2 s) y no un c\u{00e1}lculo VLM de \u{00e1}lgebra lineal.",
        frozen_key: "Number of alpha points in the MSES polar sweep. Kept small relative to AeroSandbox's own VLM sweep (analysis.sweep_n_points) since each MSES point is a real viscous-compressible solve (~1-2s) rather than a linear-algebra VLM solve.",
        frozen_value: "N\u{00fa}mero de puntos de alfa en el barrido de polar de MSES. Se mantiene peque\u{00f1}o frente al barrido VLM propio de AeroSandbox, ya que cada punto de MSES es una resoluci\u{00f3}n viscosa y compresible real (~1-2 s) y no un c\u{00e1}lculo VLM de \u{00e1}lgebra lineal.",
    },
    CatalogCorrection {
        corrected_key: "Ratio of bypass (fan duct) to core mass flow. Feeds the Native mission model turbofan network and the Propulsion Analysis on-design cycle.",
        corrected_value: "Relaci\u{00f3}n entre el gasto m\u{00e1}sico derivado (conducto del fan) y el del n\u{00fa}cleo. Alimenta la red de turbof\u{00e1}n de Native mission model y el ciclo de dise\u{00f1}o del An\u{00e1}lisis de propulsi\u{00f3}n.",
        frozen_key: "Ratio of bypass (fan duct) to core mass flow. Feeds the SUAVE turbofan network and the Propulsion Analysis on-design cycle.",
        frozen_value: "Relaci\u{00f3}n entre el gasto m\u{00e1}sico derivado (conducto del fan) y el del n\u{00fa}cleo. Alimenta la red de turbof\u{00e1}n de SUAVE y el ciclo de dise\u{00f1}o del An\u{00e1}lisis de propulsi\u{00f3}n.",
    },
    CatalogCorrection {
        corrected_key: "Mission runner dir",
        corrected_value: "Directorio del ejecutor de Native mission model",
        frozen_key: "Suave runner dir",
        frozen_value: "Directorio del ejecutor de SUAVE",
    },
    CatalogCorrection {
        corrected_key: "Mission runtime dir",
        corrected_value: "Directorio del entorno virtual de Native mission model",
        frozen_key: "Suave venv dir",
        frozen_value: "Directorio del entorno virtual de SUAVE",
    },
    CatalogCorrection {
        corrected_key: "Total pressure ratio through the core compressors (LPC x HPC combined, NOT including the fan). Feeds Native mission model's compressor sizing (split into a fixed LPC ratio + a solved HPC ratio) and the Propulsion Analysis cycle's compressor_pressure_ratio.",
        corrected_value: "Relaci\u{00f3}n de presiones total a trav\u{00e9}s de los compresores del n\u{00fa}cleo (LPC x HPC combinados, SIN incluir el fan). Alimenta el dimensionado de compresores de Native mission model (dividido en una relaci\u{00f3}n fija de LPC m\u{00e1}s una de HPC resuelta) y la relaci\u{00f3}n de presiones del ciclo del An\u{00e1}lisis de propulsi\u{00f3}n.",
        frozen_key: "Total pressure ratio through the core compressors (LPC x HPC combined, NOT including the fan). Feeds SUAVE's compressor sizing (split into a fixed LPC ratio + a solved HPC ratio) and the Propulsion Analysis cycle's compressor_pressure_ratio.",
        frozen_value: "Relaci\u{00f3}n de presiones total a trav\u{00e9}s de los compresores del n\u{00fa}cleo (LPC x HPC combinados, SIN incluir el fan). Alimenta el dimensionado de compresores de SUAVE (dividido en una relaci\u{00f3}n fija de LPC m\u{00e1}s una de HPC resuelta) y la relaci\u{00f3}n de presiones del ciclo del An\u{00e1}lisis de propulsi\u{00f3}n.",
    },
    CatalogCorrection {
        corrected_key: "Total-pressure recovery through the inlet (ram + duct losses). Matches the mission inlet_nozzle.pressure_ratio convention.",
        corrected_value: "Recuperaci\u{00f3}n de presi\u{00f3}n total a trav\u{00e9}s de la toma (p\u{00e9}rdidas de impacto y de conducto). Coincide con la convenci\u{00f3}n inlet_nozzle.pressure_ratio de la misi\u{00f3}n.",
        frozen_key: "Total-pressure recovery through the inlet (ram + duct losses). Matches SUAVE's inlet_nozzle.pressure_ratio.",
        frozen_value: "Recuperaci\u{00f3}n de presi\u{00f3}n total a trav\u{00e9}s de la toma (p\u{00e9}rdidas de impacto y de conducto). Coincide con inlet_nozzle.pressure_ratio de SUAVE.",
    },
];

fn correction_for_frozen(key: &str) -> Option<&'static CatalogCorrection> {
    SOURCE_CORRECTED_CATALOG
        .iter()
        .find(|correction| correction.frozen_key == key)
}

fn correction_for_corrected(key: &str) -> Option<&'static CatalogCorrection> {
    SOURCE_CORRECTED_CATALOG
        .iter()
        .find(|correction| correction.corrected_key == key)
}

#[test]
fn the_shipped_catalog_matches_every_entry_in_the_fixture() {
    let fixture: Fixture = alas_testkit::load("i18n", "es_catalog");
    let shipped = alas_i18n::es::catalog();

    let mut comparison = Comparison::new("alas-i18n::es", Tier::Exact);
    for (key, expected) in &fixture.catalog {
        if let Some(correction) = correction_for_frozen(key) {
            comparison.exact(
                &format!("{key} (frozen reference value)"),
                expected,
                &correction.frozen_value.to_string(),
            );
            match shipped.get(correction.corrected_key) {
                Some(actual) => comparison.exact(
                    &format!("{} (source-corrected Rust value)", correction.corrected_key),
                    actual,
                    &correction.corrected_value.to_string(),
                ),
                None => comparison.exact(
                    &format!("{} (source-corrected Rust key)", correction.corrected_key),
                    &false,
                    &true,
                ),
            };
            continue;
        }
        match shipped.get(key) {
            Some(actual) => {
                comparison.exact(key, actual, expected);
            }
            None => {
                comparison.exact(
                    &format!("{key} (present in the shipped catalog)"),
                    &false,
                    &true,
                );
            }
        }
    }
    comparison.finish();
}

#[test]
fn the_shipped_catalog_has_nothing_the_fixture_does_not() {
    let fixture: Fixture = alas_testkit::load("i18n", "es_catalog");
    let shipped = alas_i18n::es::catalog();

    let mut comparison = Comparison::new("alas-i18n::es (extra entries)", Tier::Exact);
    for key in shipped.keys() {
        if correction_for_corrected(key).is_some() {
            comparison.exact(
                &format!("{key} (source-corrected key absent from fixture)"),
                &fixture.catalog.contains_key(key),
                &false,
            );
            continue;
        }
        comparison.exact(
            &format!("{key} (present in the fixture)"),
            &fixture.catalog.contains_key(key),
            &true,
        );
    }
    comparison.finish();
}

#[test]
fn the_shipped_catalog_and_the_fixture_have_the_same_entry_count() {
    let fixture: Fixture = alas_testkit::load("i18n", "es_catalog");
    let shipped = alas_i18n::es::catalog();

    // The two comparisons above walk each side against the other's keys, but
    // neither would notice a duplicate-under-some-normalization or a count
    // drift that happened to leave both `contains_key` checks satisfied --
    // this is the length check the parity rule for `slice` reasons about.
    assert_eq!(
        shipped.len(),
        fixture.catalog.len(),
        "shipped catalog has {} entries, fixture has {}",
        shipped.len(),
        fixture.catalog.len()
    );
}
