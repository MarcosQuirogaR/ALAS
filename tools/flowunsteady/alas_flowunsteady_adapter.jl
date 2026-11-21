# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez
#
# ALAS <-> FLOWUnsteady adapter.
#
#   julia alas_flowunsteady_adapter.jl --alas-request <request.txt> --alas-result <result.txt>
#
# Reads an ALAS_FLOWUNSTEADY_REQUEST_V2 file, solves every angle of its
# schedule with FLOWUnsteady's vortex lattice on the lifting surfaces, and
# writes ALAS_FLOWUNSTEADY_RESULT_V1 with the lift
# coefficient in wind axes and the pitching moment in body axes about the
# requested reference, both on the requested area and chord.
#
# Model, stated so the result can be read against the ALAS polar:
# * Lifting surfaces only. Surfaces that are not mirror-symmetric (the fin) and
#   surfaces marked "outside the lattice" carry no lift or pitching moment at
#   zero sideslip and are left out.
# * FLOWVLM is a single-chordwise-panel lifting-line lattice of flat sections.
#   Section camber enters as its thin-airfoil zero-lift angle, added to the
#   twist: alpha_L0 = -(1/pi) int_0^pi (dz/dx) (cos t - 1) dt (Anderson,
#   Fundamentals of Aerodynamics, 6th ed., 2017, sect. 4.8, eq. 4.61), the
#   same quadrature `external_preset_audit` uses.
# * Compressibility: the vortex-particle method is incompressible. The request
#   Mach is honoured with the Goethert / Prandtl-Glauert transform ALAS applies
#   to its own lattice (`alas_stab::neutral_point::goethert_stretch`, Katz and
#   Plotkin, Low-Speed Aerodynamics, 2nd ed., sect. 12.4): every x station,
#   chord and the moment reference x are stretched by 1/beta,
#   beta = sqrt(1 - M^2), the stretched lattice is solved at the true
#   geometric angle, CL = L'/(q S) and Cm = beta M'/(q S c) on the unstretched
#   S and c.
# * Wake (ALAS_FLOWUNSTEADY_MODE): "quasi-steady" (default) solves the steady
#   lattice with a rigid semi-infinite horseshoe wake and declares
#   model=quasi_steady_vortex_lattice, which ALAS labels reduced fidelity;
#   "vpm" time-marches the free vortex-particle wake
#   (model=vortex_particle_wake): every angle starts from rest and runs
#   settling + averaging reference chords, and CL and Cm are the means over
#   the averaging window.

import FLOWUnsteady as uns
import FLOWVLM as vlm
import FLOWVPM as vpm
using LinearAlgebra: dot, cross, norm
using Printf

const RESULT_HEADER = "ALAS_FLOWUNSTEADY_RESULT_V1"
const REQUEST_HEADER = "ALAS_FLOWUNSTEADY_REQUEST_V2"
const MAX_MACH = 0.99

struct Section
    le::Vector{Float64}
    chord::Float64
    twist_deg::Float64
    airfoil::Vector{Tuple{Float64,Float64}}
end

struct Surface
    name::String
    symmetric::Bool
    sections::Vector{Section}
end

fail(msg) = (println(stderr, "alas_flowunsteady_adapter: ", msg); exit(2))

function parse_args(args)
    request = result = ""
    i = 1
    while i <= length(args)
        if args[i] == "--alas-request" && i < length(args)
            request = args[i+1]; i += 2
        elseif args[i] == "--alas-result" && i < length(args)
            result = args[i+1]; i += 2
        else
            fail("unknown argument $(args[i])")
        end
    end
    (isempty(request) || isempty(result)) && fail("usage: --alas-request <file> --alas-result <file>")
    return request, result
end

numbers(s) = parse.(Float64, split(s, ','))

function read_request(path)
    lines = readlines(path)
    (isempty(lines) || lines[1] != REQUEST_HEADER) && fail("$path is not $REQUEST_HEADER")
    keys_ = Dict{String,String}()
    surfaces = Dict{Int,Tuple{String,Bool,Int}}()
    sections = Dict{Tuple{Int,Int},Vector{Float64}}()
    points = Dict{Tuple{Int,Int},Vector{Tuple{Float64,Float64}}}()
    controls = String[]
    for line in lines[2:end]
        k, v = split(line, '='; limit=2)
        if k == "surface"
            f = split(v, ',')
            surfaces[parse(Int, f[1])] = (String(f[2]), f[3] == "true", parse(Int, f[4]))
        elseif k == "section"
            f = split(v, ',')
            sections[(parse(Int, f[1]), parse(Int, f[2]))] = parse.(Float64, f[3:7])
        elseif k == "airfoil_point"
            f = split(v, ',')
            key = (parse(Int, f[1]), parse(Int, f[2]))
            push!(get!(points, key, Tuple{Float64,Float64}[]), (parse(Float64, f[3]), parse(Float64, f[4])))
        elseif k == "control"
            push!(controls, v)
        else
            keys_[k] = v
        end
    end
    get(keys_, "units", "") == "SI" || fail("units must be SI")
    get(keys_, "geometry_axes", "") == "x_aft_y_right_z_up" || fail("unsupported geometry axes")
    surfs = Surface[]
    for idx in sort(collect(keys(surfaces)))
        name, sym, n = surfaces[idx]
        secs = [Section(sections[(idx, s)][1:3], sections[(idx, s)][4], sections[(idx, s)][5],
                        points[(idx, s)]) for s in 0:n-1]
        push!(surfs, Surface(name, sym, secs))
    end
    for c in controls
        f = split(c, ',')
        deflection = parse(Float64, f[7])
        applied = f[8] == "true"
        if !applied && abs(deflection) > 0
            fail("control $(f[1]) on $(f[2]) is deflected $(deflection) deg but not applied to the geometry; this adapter models clean surfaces only")
        end
    end
    return keys_, surfs
end

# Thin-airfoil zero-lift angle (rad) of a closed Selig-ordered section
# (trailing edge, upper surface, leading edge, lower surface, trailing edge).
function zero_lift_angle(points)
    xs = first.(points); zs = last.(points)
    ile = argmin(xs)
    up = sort(collect(zip(xs[1:ile], zs[1:ile])))
    lo = sort(collect(zip(xs[ile:end], zs[ile:end])))
    interp(tab, x) = begin
        xv = first.(tab); zv = last.(tab)
        x <= xv[1] && return zv[1]
        x >= xv[end] && return zv[end]
        j = searchsortedlast(xv, x)
        xv[j+1] == xv[j] && return zv[j]
        zv[j] + (zv[j+1] - zv[j]) * (x - xv[j]) / (xv[j+1] - xv[j])
    end
    x0, x1 = xs[ile], maximum(xs)
    camber(xc) = 0.5 * (interp(up, x0 + xc * (x1 - x0)) + interp(lo, x0 + xc * (x1 - x0)))
    n = 400
    acc = 0.0
    for k in 1:n
        t = (k - 0.5) * pi / n
        xc = 0.5 * (1 - cos(t))
        h = 1e-4
        dzdx = (camber(min(xc + h, 1.0)) - camber(max(xc - h, 0.0))) / (min(xc + h, 1.0) - max(xc - h, 0.0))
        acc += dzdx * (cos(t) - 1) * (pi / n)
    end
    return -acc / pi
end

# FLOWVLM wing from a polyline of (le, chord, twist_deg) stations ordered in
# increasing y, with `nspan` elements distributed by segment length.
function build_wing(stations, nspan)
    lens = [norm(stations[k+1][1][2:3] .- stations[k][1][2:3]) for k in 1:length(stations)-1]
    total = sum(lens)
    s1 = stations[1]
    wing = vlm.Wing(s1[1][1], s1[1][2], s1[1][3], s1[2], s1[3])
    for k in 2:length(stations)
        lens[k-1] <= 1e-9 * max(total, 1.0) && continue
        n = max(1, round(Int, nspan * lens[k-1] / total))
        s = stations[k]
        vlm.addchord(wing, s[1][1], s[1][2], s[1][3], s[2], s[3], n)
    end
    return wing
end

function main()
    request_path, result_path = parse_args(ARGS)
    t0 = time()
    println("FLOWUnsteady ", pkgversion(uns), ", ", Threads.nthreads(), " threads, wake model ",
            get(ENV, "ALAS_FLOWUNSTEADY_MODE", "quasi-steady")); flush(stdout)
    req, surfaces = read_request(request_path)
    num(k) = parse(Float64, req[k])
    S = num("area_m2"); c = num("chord_m"); b = num("span_m")
    xref = numbers(req["moment_reference_m"])
    rho = num("density_kg_m3"); V = num("true_airspeed_m_s"); mach = num("mach")
    beta_deg = num("beta_deg")
    abs(beta_deg) > 1e-12 && fail("sideslip $(beta_deg) deg: this adapter solves symmetric flight only")
    any(abs.(numbers(req["angular_rates_rad_s"])) .> 1e-12) && fail("non-zero angular rates are not modelled")
    steps_per_chord = parse(Int, req["steps_per_reference_chord"])
    settle = num("settling_reference_chords"); average = num("averaging_reference_chords")
    alphas = numbers(req["alpha_deg"])
    nspan_main = parse(Int, get(ENV, "ALAS_FLOWUNSTEADY_SPAN_ELEMENTS", "24"))
    # Wake model. "quasi-steady" (default): the rigid semi-infinite
    # horseshoe wake of FLOWUnsteady's QVLMVehicle, solved directly with its
    # FLOWVLM lattice solver (`vlm.solve`) and Kutta-Joukowski forces on every
    # bound segment, as FLOWVLM's `_calculate_forces` sums them (its
    # `calculate_field("Ftot")` then fails decomposing a WingSystem in
    # `check_coord_sys`, so the sum is done here). This is the steady
    # limit of the requested unsteady lattice. FLOWUnsteady 3.4.0's
    # `run_simulation` cannot drive a QVLMVehicle: it passes
    # `apply_correction3D`/`apply_extrapolate` to a quasi-steady `solve` that
    # does not accept them (MethodError), hence the direct call.
    # "vpm": the free vortex-particle wake (VLMVehicle), time-marched over the
    # requested settling and averaging chords; it costs about an hour per
    # angle on three threads, so it is for offline runs with a long timeout.
    mode = get(ENV, "ALAS_FLOWUNSTEADY_MODE", "quasi-steady")
    mode in ("quasi-steady", "vpm") || fail("ALAS_FLOWUNSTEADY_MODE must be quasi-steady or vpm, not '$(mode)'")
    quasi = mode == "quasi-steady"

    beta = sqrt(1 - min(abs(mach), MAX_MACH)^2)
    stretch(x) = [x[1] / beta, x[2], x[3]]
    xref_s = stretch(xref)
    q = 0.5 * rho * V^2

    # Geometry: one FLOWVLM wing per mirrored surface (left tip to right tip
    # when the surface starts on the plane of symmetry, else one per side).
    system = vlm.WingSystem()
    kept = String[]
    for (i, surf) in enumerate(surfaces)
        if !surf.symmetric || occursin("outside the lattice", surf.name)
            println("skipping surface '$(surf.name)' (not a symmetric lifting surface)")
            continue
        end
        right = [(stretch(s.le), s.chord / beta, s.twist_deg - rad2deg(zero_lift_angle(s.airfoil))) for s in surf.sections]
        semispan = sum(norm(right[k+1][1][2:3] .- right[k][1][2:3]) for k in 1:length(right)-1)
        nspan = max(4, round(Int, nspan_main * semispan / (b / 2)))
        mirror(st) = ([st[1][1], -st[1][2], st[1][3]], st[2], st[3])
        if abs(right[1][1][2]) < 1e-9
            stations = vcat(reverse(mirror.(right[2:end])), right)
            vlm.addwing(system, "s$(i)", build_wing(stations, 2 * nspan))
        else
            vlm.addwing(system, "s$(i)L", build_wing(reverse(mirror.(right)), nspan))
            vlm.addwing(system, "s$(i)R", build_wing(right, nspan))
        end
        push!(kept, surf.name)
    end
    isempty(kept) && fail("no symmetric lifting surface in the request")
    m = vlm.get_m(system)
    @printf("surfaces: %s; %d horseshoes; Mach %.3f, beta %.4f\n", join(kept, ", "), m, mach, beta)
    flush(stdout)

    c_s = c / beta                       # stretched reference chord
    dt = c_s / (V * steps_per_chord)
    nsteps = round(Int, steps_per_chord * (settle + average))
    navg = max(1, round(Int, steps_per_chord * average))
    ttot = nsteps * dt
    p_per_step = 1
    sigma_vpm = 2.0 * V * dt / p_per_step
    sigma_surf = 0.05 * b
    # Wind-axis CL and body-axis Cm about the stretched reference from the
    # per-horseshoe forces, each applied at its bound-vortex midpoint; the
    # stretch scales moment arms in x by 1/beta, undone by the factor beta.
    function coefficients(wing, F, Lhat)
        My = 0.0
        for k in 1:m
            hs = vlm.getHorseshoe(wing, k)
            r = 0.5 .* (hs[2] .+ hs[3]) .- xref_s
            My += cross(r, F[k])[2]
        end
        return dot(sum(F), Lhat) / (q * S), beta * My / (q * S * c)
    end
    # Kutta-Joukowski force rho Gamma V x dl on the bound segment and the two
    # chordwise legs of each horseshoe, V the freestream plus the velocity
    # induced by the whole lattice (FLOWVLM `_calculate_forces`).
    function steady_forces(wing, Vinf)
        F = Vector{Vector{Float64}}()
        for k in 1:m
            Ap, A, B, Bp, _, _, _, Gamma = vlm.getHorseshoe(wing, k)
            f = zeros(3)
            for (P, Q) in ((A, B), (Ap, A), (B, Bp))
                X = 0.5 .* (P .+ Q)
                Vx = Vinf(X, 0.0) .+ vlm.Vind(wing, X; t=0.0, ign_col=true)
                f .+= rho * Gamma .* cross(Vx, Q .- P)
            end
            push!(F, f)
        end
        return F
    end
    calc_force = uns.generate_calc_aerodynamicforce(; add_parasiticdrag=false, add_skinfriction=false)

    lines = String[]
    for alpha in alphas
        ta = time()
        Vinf(X, t) = V * [cosd(alpha), 0.0, sind(alpha)]
        Dhat = [cosd(alpha), 0.0, sind(alpha)]
        Lhat = cross(Dhat, [0.0, 1.0, 0.0])
        wing = deepcopy(system)
        if quasi
            # The wing's freestream-aligned trailing legs cross the tailplane
            # lattice at some angles; an unregularised filament then induces
            # a near-singular velocity at a tail control point (CL and Cm
            # jumped between 5.1 and 5.8 deg on the A320). FLOWVLM's
            # Winckelmans regularisation (G. S. Winckelmans and A. Leonard,
            # J. Comput. Phys. 109(2), 1993, pp. 247-273), the one FLOWUnsteady
            # applies through `sigma_vlm_solver`, is enabled with a core equal
            # to the shortest bound vortex. A quarter of it left the jump
            # (filaments still pass within a core of a control point); one
            # length removes it and moves CL by 0.13 % away from the crossing.
            vlm.setVinf(wing, Vinf)
            shortest = minimum(k -> norm(vlm.getHorseshoe(wing, k)[3] .- vlm.getHorseshoe(wing, k)[2]), 1:m)
            vlm.VLMSolver._blobify(true)
            vlm.VLMSolver._smoothing_rad(shortest)
            vlm.solve(wing, Vinf)
            CL, Cm = coefficients(wing, steady_forces(wing, Vinf), Lhat)
            (isfinite(CL) && isfinite(Cm)) || fail("non-finite coefficients at alpha $(alpha)")
            @printf("alpha %7.3f deg  CL %.5f  Cm %.5f  (steady lattice, %.1f s)\n", alpha, CL, Cm, time() - ta)
            flush(stdout)
            push!(lines, @sprintf("point=%.12f,%.12f,%.12f,%.12f,%.12f", alpha, mach, beta_deg, CL, Cm))
            continue
        end
        vehicle = uns.VLMVehicle(wing; vlm_system=wing, wake_system=wing)
        maneuver = uns.KinematicManeuver((), (), t -> zeros(3), t -> zeros(3))
        sim = uns.Simulation(vehicle, maneuver, 0.0, 0.0, ttot; Vinit=zeros(3), Winit=zeros(3))
        max_particles = (nsteps + 1) * (m * (p_per_step + 1) + p_per_step)
        cls = Float64[]; cms = Float64[]
        prev = Ref{Any}(nothing)
        function monitor(sim, PFIELD, T, DT; optargs...)
            if PFIELD.nt > nsteps - navg
                F = calc_force(wing, prev[], PFIELD, Vinf, DT, rho; t=PFIELD.t, spandir=[0.0, 1.0, 0.0])
                CL, Cm = coefficients(wing, F, Lhat)
                push!(cls, CL)
                push!(cms, Cm)
            end
            prev[] = deepcopy(wing)
            if PFIELD.nt % 50 == 0
                @printf("  alpha %.3f: step %d/%d, %d particles, %.1f s\n", alpha, PFIELD.nt, nsteps, vpm.get_np(PFIELD), time() - ta)
                flush(stdout)
            end
            return false
        end
        uns.run_simulation(sim, nsteps;
            Vinf=Vinf, rho=rho, sound_spd=nothing,
            p_per_step=p_per_step, max_particles=max_particles,
            vpm_integration=vpm.euler,
            sigma_vlm_solver=-1, sigma_vlm_surf=sigma_surf, sigma_rotor_surf=sigma_surf,
            sigma_vpm_overwrite=sigma_vpm, shed_starting=true, vlm_rlx=0.7,
            extra_runtime_function=monitor,
            save_path=nothing, prompt=false, verbose=false, raisewarnings=false)
        isempty(cls) && fail("no averaged samples at alpha $(alpha)")
        CL = sum(cls) / length(cls); Cm = sum(cms) / length(cms)
        (isfinite(CL) && isfinite(Cm)) || fail("non-finite coefficients at alpha $(alpha)")
        @printf("alpha %7.3f deg  CL %.5f  Cm %.5f  (%d samples, %.1f s)\n", alpha, CL, Cm, length(cls), time() - ta)
        flush(stdout)
        push!(lines, @sprintf("point=%.12f,%.12f,%.12f,%.12f,%.12f", alpha, mach, beta_deg, CL, Cm))
    end

    open(result_path * ".partial", "w") do io
        println(io, RESULT_HEADER)
        println(io, "area_m2=", req["area_m2"])
        println(io, "chord_m=", req["chord_m"])
        println(io, "span_m=", req["span_m"])
        println(io, "moment_reference_m=", req["moment_reference_m"])
        println(io, "lifting_surfaces_only=true")
        println(io, "lift_is_wind_axis=true")
        println(io, "pitch_moment_is_body_axis=true")
        println(io, "model=", quasi ? "quasi_steady_vortex_lattice" : "vortex_particle_wake")
        foreach(l -> println(io, l), lines)
    end
    mv(result_path * ".partial", result_path; force=true)
    @printf("done: %d points in %.1f s\n", length(lines), time() - t0)
end

main()
