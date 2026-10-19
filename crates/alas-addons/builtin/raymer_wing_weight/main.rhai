// Raymer, Aircraft Design: A Conceptual Approach, 6th ed., Eq. 15.25 (p. 574).
// British units inside the equation: pounds and square feet.
let wdg = kg_to_lb(inputs.mtow_kg);
let nz = 1.5 * inputs.limit_load_factor;
let sw = m2_to_ft2(inputs.wing_area_m2);
let scsw = m2_to_ft2(inputs.control_surface_area_m2);
let sweep = deg_to_rad(inputs.sweep_quarter_chord_deg);

let lb = 0.0051
    * ((wdg * nz) ** 0.557)
    * (sw ** 0.649)
    * (inputs.aspect_ratio ** 0.5)
    * (inputs.thickness_ratio_root ** (-0.4))
    * ((1.0 + inputs.taper_ratio) ** 0.1)
    / cos(sweep)
    * (scsw ** 0.1);

#{ wing_mass_kg: lb_to_kg(lb), wing_mass_lb: lb }
