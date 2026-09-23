// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares the embedded Spanish catalog against `alas.translations.es.CATALOG`,
//! dumped verbatim by `golden/generators/gen_i18n.py`.
//!
//! Keys are matched by exact English text, so this is `exact` tier rather
//! than a tolerance comparison: a translation one character off from the
//! Python source is exactly as wrong as one that is unrelated. Checked in
//! both directions, every fixture entry present with the right value in the
//! shipped catalog, and nothing extra in the shipped catalog the fixture
//! doesn't know about, because zipping the two together would let a length
//! mismatch (a key dropped from one side, or a stray extra key) hide behind
//! however many pairs happened to still agree.
//!
//! Sixteen source strings are intentionally changed and pinned in
//! [`SOURCE_CORRECTED_CATALOG`]: ten when the native mission replaced the
//! retired SUAVE runtime boundary, and five more (plus one further rewrite of
//! an original SUAVE-era entry) when the vortex-lattice mesh-resolution
//! fields were given absolute-count, evidence-backed help text, and one when
//! the wave-drag help moved to the Lock/Korn critical-Mach law. That second
//! group mirrors `alas_config::tests::parity_config`'s
//! `solver_agnostic_help_correction`, which documents the same
//! 2026-09-11 VLM resolution-sensitivity study; see that test and
//! `alas_config::analysis` for the underlying measurements. Their corrected
//! Rust keys and values, together with the frozen reference keys and values,
//! are kept in the named allowlist below. This keeps the exact tier while
//! making the textual deviation visible in both directions.
//!
//! Two further shipped entries have no Python counterpart at all: the label
//! and help text of the native `min_passenger_capacity` field
//! (`alas_config::DesignRequirements::min_passenger_capacity`), a hard floor
//! added after a physical audit found the translated optimizer replacing
//! real-preset passenger targets with no minimum. [`NATIVE_CATALOG_ENTRIES`]
//! pins both the frozen fixture's absence and the shipped catalog's value for
//! each, so the divergence cannot drift silently in either direction, and the
//! entry-count check derives its expected count from that registry instead of
//! a bare number.

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
    // `fine_chordwise_resolution`'s help was rewritten a second time, after
    // the SUAVE-mention removal below was first pinned: the current shipped
    // text replaces the old "~8 chordwise panels ... (~1-4 deg)" prose with
    // the measured convergence behaviour (8 panels ~1 deg high, 16 ~0.5 deg,
    // first-order in panel count) from the same VLM resolution-sensitivity
    // study `alas_config::tests::parity_config` pins for
    // `AnalysisConfig.fine_chordwise_resolution`. The frozen reference value
    // is unchanged from the original SUAVE-mention row.
    CatalogCorrection {
        corrected_key: "Chordwise panel resolution for the once-per-run final/reported analysis. A supercritical section needs roughly 8 chordwise panels before the VLM resolves its camber line at all, and the convergence is first-order in panel count: 8 still leaves the reported cruise attitude about 1 deg high on a supercritical wing, 16 about 0.5 deg. Kept above the in-loop value so the REPORTED cruise alpha and L/D are the more trustworthy of the two, at a cost paid once per run.",
        corrected_value: "Resoluci\u{00f3}n de paneles en cuerda para el an\u{00e1}lisis final informado de una vez por ejecuci\u{00f3}n. Un perfil supercr\u{00ed}tico necesita unos 8 paneles en cuerda antes de que el m\u{00e9}todo resuelva siquiera su l\u{00ed}nea de curvatura, y la convergencia es de primer orden en el n\u{00fa}mero de paneles: con 8 la actitud de crucero informada sigue siendo aproximadamente 1 grado alta en un ala supercr\u{00ed}tica, y con 16 unos 0,5 grados. Se mantiene por encima del valor del bucle para que el \u{00e1}ngulo de ataque de crucero y la eficiencia aerodin\u{00e1}mica INFORMADOS sean los m\u{00e1}s fiables de los dos, a un coste que se paga una vez por ejecuci\u{00f3}n.",
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
        corrected_key: "Model Comparison: shared variables across Native analysis / Native mission model / MSES",
        corrected_value: "Comparaci\u{00f3}n de modelos: variables comunes entre Native analysis, Native mission model y MSES",
        frozen_key: "Model Comparison: shared variables across AeroSandbox / SUAVE / MSES",
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
    // The five rows below pin the vortex-lattice mesh-resolution help text
    // rewritten alongside `alas_config::tests::parity_config`'s
    // `solver_agnostic_help_correction`. The frozen Python/SUAVE-era prose
    // described a generic multiplier/refinement ("Higher = more accurate,
    // slower") with no stated units; the corrected prose states the actual
    // absolute-panel-count semantics and the physical cost/benefit measured
    // in the 2026-09-11 VLM resolution-sensitivity study (see
    // `alas_config::analysis`'s module doc), so a user reading the help text
    // knows what the number means and why the default is what it is.
    CatalogCorrection {
        corrected_key: "Spanwise panels across the whole wing semispan for the vortex-lattice solver. This is an absolute count, not a count per section: a planform with a side-of-body station and a kink gets the same mesh density as one without, and adding a station no longer changes the panel count underneath a search. Every planform station (root, side-of-body, kink, tip) is always kept as a panel edge whatever the count, so refining the mesh never averages a kink away. The default of 24 is converged: a twelve-fold refinement moves the trimmed cruise attitude by 0.01 deg.",
        corrected_value: "Paneles en envergadura a lo largo de toda la semienvergadura del ala para el solver de malla de torbellinos. Es un recuento absoluto, no un recuento por tramo: una planta alar con estaci\u{00f3}n de costado de fuselaje y quiebro recibe la misma densidad de malla que una sin ellas, y a\u{00f1}adir una estaci\u{00f3}n ya no cambia el n\u{00fa}mero de paneles bajo los pies de una b\u{00fa}squeda. Toda estaci\u{00f3}n de la planta alar (ra\u{00ed}z, costado de fuselaje, quiebro y punta) se mantiene siempre como borde de panel sea cual sea el recuento, as\u{00ed} que refinar la malla nunca promedia un quiebro hasta borrarlo. El valor por defecto de 24 est\u{00e1} convergido: un refinamiento de doce veces mueve la actitud de crucero equilibrada 0,01 grados.",
        frozen_key: "Spanwise panel refinement per wing section for the vortex-lattice solver. Higher = more accurate, slower.",
        frozen_value: "Refinamiento de paneles en envergadura por secci\u{00f3}n del ala para el m\u{00e9}todo de red de torbellinos. M\u{00e1}s alto = m\u{00e1}s preciso pero m\u{00e1}s lento.",
    },
    CatalogCorrection {
        corrected_key: "Spanwise panels across each tail surface for the vortex-lattice solver, as an absolute count rather than a count per section. Both stabilizers are single-section surfaces, so this is the panel count they already had; it is stated absolutely so a cranked fin later gets the same density rather than twice it.",
        corrected_value: "Paneles en envergadura a lo largo de cada superficie de cola para el solver de malla de torbellinos, como recuento absoluto en lugar de por tramo. Ambos estabilizadores son superficies de un solo tramo, as\u{00ed} que este es el recuento de paneles que ya ten\u{00ed}an; se expresa de forma absoluta para que una deriva quebrada en el futuro reciba la misma densidad y no el doble.",
        frozen_key: "Spanwise panel refinement per tail surface for the vortex-lattice solver.",
        frozen_value: "Refinamiento de paneles en envergadura por superficie de cola para el m\u{00e9}todo de red de torbellinos.",
    },
    CatalogCorrection {
        corrected_key: "Multiplier on each surface's built-in spanwise panel subdivision for the vortex-lattice solver. Leave at 1: the geometry builder has already subdivided every surface (24 strips per semispan on the main wing), and that is converged, refining it further moves the trimmed cruise attitude by 0.01 deg. Values above 2 are rejected, because this multiplier re-applies a cosine spacing inside each existing strip and the induced drag then stops converging. Part of the Fidelity preset.",
        corrected_value: "Multiplicador de la subdivisi\u{00f3}n de paneles en envergadura propia de cada superficie para el solver de malla de torbellinos. D\u{00e9}jalo en 1: el constructor de geometr\u{00ed}a ya ha subdividido cada superficie (24 franjas por semienvergadura en el ala principal) y ese valor est\u{00e1} convergido; refinarlo m\u{00e1}s mueve la actitud de crucero equilibrada 0,01 grados. Se rechazan valores superiores a 2, porque este multiplicador vuelve a aplicar un espaciado coseno dentro de cada franja existente y la resistencia inducida deja de converger. Forma parte del ajuste de Fidelidad.",
        frozen_key: "Multiplier on each surface's built-in spanwise panel subdivision for the vortex-lattice solver. Higher = finer mesh, slower. Part of the Fidelity preset.",
        frozen_value: "Multiplicador de la subdivisi\u{00f3}n de paneles en envergadura de cada superficie para el m\u{00e9}todo de red de torbellinos. M\u{00e1}s alto = malla m\u{00e1}s fina y m\u{00e1}s lenta. Forma parte del preajuste de fidelidad.",
    },
    CatalogCorrection {
        corrected_key: "Number of chordwise panels per strip for the vortex-lattice solver, used by the fast in-loop estimate the optimizer ranks candidates with; the final reported analysis uses fine_chordwise_resolution instead. This is a literal panel count, not a multiplier, and nothing else in the pipeline sets one. At 1 the mesh samples the camber line only at the leading and trailing edges, where it is zero, so the section becomes a flat plate: cruise attitude comes out 1-4 deg high, L/D wrong by -14 to +3 percent, and the four airfoil bump design variables have no effect at all. 8 ranks candidates identically to a converged mesh. Higher = finer mesh, slower. Part of the Fidelity preset.",
        corrected_value: "N\u{00fa}mero de paneles en cuerda por franja para el solver de malla de torbellinos, usado por la estimaci\u{00f3}n r\u{00e1}pida del bucle con la que el optimizador ordena los candidatos; el an\u{00e1}lisis final informado usa fine_chordwise_resolution. Es un recuento literal de paneles, no un multiplicador, y ning\u{00fa}n otro punto del flujo fija uno. Con valor 1 la malla muestrea la l\u{00ed}nea de curvatura solo en los bordes de ataque y salida, donde vale cero, as\u{00ed} que el perfil se convierte en una placa plana: la actitud de crucero sale 1-4 grados alta, la eficiencia aerodin\u{00e1}mica se desv\u{00ed}a entre -14 y +3 por ciento y las cuatro variables de dise\u{00f1}o de protuberancia del perfil no tienen ning\u{00fa}n efecto. Con 8 el orden de los candidatos coincide con el de una malla convergida. M\u{00e1}s alto = malla m\u{00e1}s fina y m\u{00e1}s lenta. Forma parte del ajuste de Fidelidad.",
        frozen_key: "Multiplier on each surface's built-in chordwise panel subdivision for the vortex-lattice solver. Higher = finer mesh, slower. Part of the Fidelity preset. Used by the fast in-loop estimate; the final reported analysis uses fine_chordwise_resolution instead.",
        frozen_value: "Multiplicador de la subdivisi\u{00f3}n de paneles en cuerda de cada superficie para el m\u{00e9}todo de red de torbellinos. M\u{00e1}s alto = malla m\u{00e1}s fina y m\u{00e1}s lenta. Forma parte del preajuste de fidelidad. Lo usa la estimaci\u{00f3}n r\u{00e1}pida dentro del bucle; el an\u{00e1}lisis final utiliza fine_chordwise_resolution.",
    },
    CatalogCorrection {
        corrected_key: "Spanwise panel resolution used ONLY for the once-per-run final/reported analysis (drag polar, trimmed cruise point, neutral point), not the optimizer loop. Leave at 1 for the same reason as the in-loop field: the span is already converged, so raising this doubles the panel count to change the answer by about 1 percent. Spend the panels on fine_chordwise_resolution instead.",
        corrected_value: "Resoluci\u{00f3}n de paneles en envergadura usada SOLO para el an\u{00e1}lisis final informado de una vez por ejecuci\u{00f3}n (polar de resistencia, punto de crucero equilibrado, punto neutro), no para el bucle del optimizador. D\u{00e9}jala en 1 por la misma raz\u{00f3}n que el campo del bucle: la envergadura ya est\u{00e1} convergida, as\u{00ed} que subirla duplica el n\u{00fa}mero de paneles para cambiar el resultado en torno a un 1 por ciento. Invierte esos paneles en fine_chordwise_resolution.",
        frozen_key: "Spanwise panel resolution used ONLY for the once-per-run final/reported analysis (drag polar, trimmed cruise point, neutral point), not the optimizer loop. Higher fidelity where speed doesn't matter.",
        frozen_value: "Resoluci\u{00f3}n de paneles en envergadura empleada SOLO en el an\u{00e1}lisis final que se ejecuta una vez por ejecuci\u{00f3}n (polar de resistencia, punto de crucero equilibrado, punto neutro), no en el bucle del optimizador. Mayor fidelidad donde la velocidad no importa.",
    },
    // The frozen help states the Korn rise from M_drag_divergence; the
    // corrected law is the Lock/Korn form from the critical Mach, with a
    // positive coefficient enforced by `alas_config`'s wave-drag validation
    // (see `alas_config::tests::parity_config`'s wave_drag_coefficient pin).
    CatalogCorrection {
        corrected_key: "Leading constant in the Lock/Korn wave-drag rise: CD_wave = coefficient * max(M - M_critical, 0)^4. The critical Mach is M_drag_divergence - (0.1 / (4 * coefficient))^(1/3); coefficient must be positive.",
        corrected_value: "Constante principal del aumento de resistencia de onda de Lock/Korn: CD_wave = coefficient * max(M - M_critical, 0)^4. El Mach cr\u{00ed}tico es M_drag_divergence - (0.1 / (4 * coefficient))^(1/3); el coeficiente debe ser positivo.",
        frozen_key: "Leading constant in the Korn wave-drag rise: CD_wave = coefficient * (M - M_drag_divergence)^4.",
        frozen_value: "Constante principal del aumento de resistencia de onda de Korn: CD_onda = coeficiente * (M - M_divergencia)^4.",
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

/// A shipped catalog entry with no Python counterpart at all: a native
/// product string the frozen fixture predates by construction, rather than a
/// reworded translation of something upstream once had. Mirrors
/// `alas_config::tests::parity_config`'s `is_native_config_field`, but pins a
/// value rather than merely excusing a key's absence, since a catalog entry
/// (unlike a config field) *is* its value.
#[derive(Clone, Copy)]
struct NativeCatalogEntry {
    key: &'static str,
    value: &'static str,
}

/// The label and help text of `alas_config::DesignRequirements`'s
/// `min_passenger_capacity`: a native advanced-only load-case floor added
/// after a physical audit found the translated optimizer silently replacing
/// real-preset passenger targets with no minimum. There is no Python field to
/// have translated, so both rows here are pinned two-sidedly: the fixture
/// must not have the key, and the shipped catalog must hold exactly this
/// value.
const NATIVE_CATALOG_ENTRIES: &[NativeCatalogEntry] = &[
    NativeCatalogEntry {
        key: "Minimum passenger capacity",
        value: "Capacidad m\u{00ed}nima de pasajeros",
    },
    NativeCatalogEntry {
        key: "Hard floor on the geometry-resolved passenger capacity: a candidate whose class-mix and geometry produce fewer than this many seats is scored infeasible under the configured geometry constraint policy. 0 = disabled (the default): capacity is otherwise always dynamic, whatever the configured cabin class-mix percentages and the candidate's actual fuselage/cabin geometry produce, with no minimum.",
        value: "L\u{00ed}mite m\u{00ed}nimo obligatorio sobre la capacidad de pasajeros resuelta por geometr\u{00ed}a: un candidato cuya mezcla de clases y geometr\u{00ed}a produzcan menos asientos que este n\u{00fa}mero se punt\u{00fa}a como no factible seg\u{00fa}n la pol\u{00ed}tica de restricciones de geometr\u{00ed}a configurada. 0 = desactivado (el valor por defecto): la capacidad es, por lo dem\u{00e1}s, siempre din\u{00e1}mica, sea cual sea el resultado de los porcentajes de mezcla de clases configurados y de la geometr\u{00ed}a real de fuselaje/cabina del candidato, sin m\u{00ed}nimo alguno.",
    },
];

fn native_catalog_entry(key: &str) -> Option<&'static NativeCatalogEntry> {
    NATIVE_CATALOG_ENTRIES.iter().find(|entry| entry.key == key)
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
    for (key, actual_value) in shipped.iter() {
        if correction_for_corrected(key).is_some() {
            comparison.exact(
                &format!("{key} (source-corrected key absent from fixture)"),
                &fixture.catalog.contains_key(key),
                &false,
            );
            continue;
        }
        if let Some(native) = native_catalog_entry(key) {
            // Pinned two-sidedly: the frozen fixture must not have picked
            // this key up from somewhere (it has no Python source to have
            // come from), and the shipped catalog must hold exactly the
            // recorded native value, so this arm cannot silently swallow a
            // future unrelated drift in either the key or its translation.
            comparison.exact(
                &format!("{key} (native key absent from fixture)"),
                &fixture.catalog.contains_key(key),
                &false,
            );
            comparison.exact(
                &format!("{key} (native shipped value)"),
                actual_value,
                &native.value.to_string(),
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
    // drift that happened to leave both `contains_key` checks satisfied;
    // this is the length check the parity rule for `slice` reasons about.
    //
    // The shipped catalog carries `NATIVE_CATALOG_ENTRIES.len()` more entries
    // than the fixture: exactly the native strings with no Python
    // counterpart. Expressing the expected count through the registry, not a
    // bare literal, means adding or removing a native entry there is the only
    // way to move this number, so it cannot rot out of sync with the rows
    // that justify it.
    let expected_shipped_len = fixture.catalog.len() + NATIVE_CATALOG_ENTRIES.len();
    assert_eq!(
        shipped.len(),
        expected_shipped_len,
        "shipped catalog has {} entries, fixture has {} plus {} native entries",
        shipped.len(),
        fixture.catalog.len(),
        NATIVE_CATALOG_ENTRIES.len()
    );
}
