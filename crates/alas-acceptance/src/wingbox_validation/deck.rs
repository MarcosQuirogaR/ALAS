// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Straight transformed-section Euler-Bernoulli beam, SI units.

use super::model::Model;

fn real(value: f64) -> String {
    format!("{value:.9e}")
}

pub(super) fn beam(model: &Model) -> String {
    let mut text = format!("SOL 101\nCEND\nTITLE = {} EQUIVALENT WING BOX\nECHO = NONE\nSPC = 1\nLOAD = 1\nDISPLACEMENT(PRINT) = ALL\nSTRESS(PRINT) = ALL\nFORCE(PRINT) = ALL\nSPCFORCES(PRINT) = ALL\nBEGIN BULK\nPARAM,POST,-1\nMAT1,1,{},{},{}\nSPC1,1,123456,1\n", model.name, real(model.e), real(model.e / (2.0 * (1.0 + model.nu))), real(model.nu));
    for (index, &y) in model.case.y.iter().enumerate() {
        text.push_str(&format!("GRID,{},,0.,{},0.\n", index + 1, real(y)));
    }
    for (index, pair) in model.sections.windows(2).enumerate() {
        let eid = index + 1;
        // Element x follows global +y; element y is global +z. I1 therefore
        // supplies vertical bending. PBEAM K1=K2=0 removes transverse shear.
        text.push_str(&format!("CBEAM,{eid},{eid},{eid},{},0.,0.,1.\n", eid + 1));
        let a = &pair[0];
        let b = &pair[1];
        text.push_str(&format!(
            "PBEAM,{eid},1,{},{},{},0.,{},0.\n",
            real(a.area),
            real(a.i1),
            real(a.i2),
            real(a.j)
        ));
        recovery(&mut text, a.recovery);
        text.push_str(&format!(
            "+,YES,1.,{},{},{},0.,{},0.\n",
            real(b.area),
            real(b.i1),
            real(b.i2),
            real(b.j)
        ));
        recovery(&mut text, b.recovery);
        text.push_str("+,0.,0.\n");
        // A constant segment average reproduces the native double-trapezoid
        // station shear and moment exactly, without changing the load state.
        let q = 0.5 * (model.case.q_net[index] + model.case.q_net[index + 1]);
        text.push_str(&format!(
            "PLOAD1,1,{eid},FZ,FR,0.,{},1.,{}\n",
            real(q),
            real(q)
        ));
    }
    for &(y, force) in &model.point_forces {
        let i = model.case.y.windows(2).position(|p| y >= p[0] && y <= p[1]);
        if let Some(i) = i {
            let xi = (y - model.case.y[i]) / (model.case.y[i + 1] - model.case.y[i]);
            text.push_str(&format!(
                "PLOAD1,1,{},FZ,FR,{},{},{},{}\n",
                i + 1,
                real(xi),
                real(force),
                real(xi),
                real(force)
            ));
        }
    }
    text.push_str("ENDDATA\n");
    text
}

fn recovery(text: &mut String, c: f64) {
    text.push_str(&format!(
        "+,{c:.9e},0.,{:.9e},0.,{c:.9e},0.,{:.9e},0.\n",
        -c, -c
    ));
}
