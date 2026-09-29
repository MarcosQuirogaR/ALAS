# ATR 72 preset: approximate T-tail reconstruction

The preset tail is a conceptual reconstruction, not certified geometry or an
airworthiness validation. The previous horizontal tail at z=1 m was a low tail;
its 2.8 m leading-edge sweep also disagreed with the nearly straight tail shown
in the manufacturer views. Corrections were selected from the drawing before
running acceptance checks. No CG limits, gear stations, masses or rejection
thresholds were changed as part of this correction.

## Source and reproducible reading

ATR, *ATR 72-600 factsheet*, manufacturer-hosted 2020 PDF, printed page 22:
[manufacturer PDF](https://www.atr-aircraft.com/wp-content/uploads/2020/07/Factsheets_-_ATR_72-600.pdf).
The dimensioned side/front views and rotated top view are rendered illustrations,
not dimensioned tail engineering drawings. The reference renders (`atr72-600-manufacturer.pdf`,
`three-view.png`, `side-detail.png`, `top-tail.png`) are not tracked in this repository;
regenerate them from the PDF as described below.

Reproduce the reference image with Poppler:

```text
pdftoppm -f 1 -singlefile -scale-to 2000 -png atr72-600-manufacturer.pdf three-view
```

The resulting image is 1051 by 2000 pixels. Pixel coordinates below use its
upper-left origin. Side-view nose/tail endpoints are approximately x=61/560;
the printed length 27.166 m gives 0.05444 m/pixel. The straight barrel centre
is y=421 +/-3 pixels and horizontal-tail chord plane y=335 +/-3 pixels:
z=(421-335)*0.05444=4.68 m. Ground y=464 and maximum fin cap y=323 give 7.68 m,
consistent with the printed 7.65 m overall height. Ground is not the geometry
z datum. The small cap/fairing above the tail is not represented by a separate
aerodynamic section in this two-section fin model.

The rotated top-view axis runs approximately from nose (638,449) to aft
fuselage (986,802). Project along that axis and scale its length to 27.166 m.
Approximate horizontal-tail root LE/TE are (957,771)/(979,794), and right tip
LE/TE (1010,731)/(1026,746). These imply root x about 24.7 m, root chord
1.8 m, tip chord 1.2 m, leading-edge setback 0.5 m, and semispan 3.6 m.
The side view independently puts the upper fin leading edge near x=25 m.
Root fillets, line thickness, perspective and axis identification dominate
uncertainty: use **+/-0.3 m** for reconstructed positions/chords/setback, not
the pixel scale as a precision claim. This is an engineering interpretation
bound, not a statistical confidence interval.

## Coordinates used

ALAS uses x aft from its nose, y right and z up from the barrel centre
(`nose_z_m=cabin_z_m=0`). Tail offsets mean fuselage length minus root x;
tip LE coordinates are relative to each root. At nominal tail scale 1 and
tail x shift 0:

| Surface | Root LE x,z (m) | Tip LE x,y,z (m) | Root/tip chord (m) |
| --- | --- | --- | --- |
| Horizontal, mirrored | 24.8,4.7 | 25.4,3.6,4.7 | 1.8/1.2 |
| Vertical | 22.3,1.0 | 24.8,0,4.7 | 3.8/1.8 |

The fin root leading/trailing edges approximate the side-view primary fin
at x~22.3/26.1 m near z=1 m; its long dorsal fuselage fillet is omitted.
The two reference chord lines coincide at the attachment; the horizontal
surface's existing -1 degree incidence remains an independent modelling
assumption, so solid surface fillets/structural junctions are not reproduced.
Tip setback is rounded to 0.6 m within the image uncertainty. The existing
3.6 m semispan is consistent with the top view and is retained. The front
view supports a level horizontal tail. Every trailing-edge reference station
remains ahead of the 27.166 m aft fuselage endpoint.

`alas-geom/src/builder.rs` translates these root/tip sections
without an additional datum conversion. OpenVSP exports those generated
sections with symmetry for the horizontal tail and vertical rotation for the
fin. The regression checks nominal attachment, finite trailing-edge positions,
the overall longitudinal envelope and the level tail reference plane. It does
not establish mass, trim, stability, aerodynamic convergence or safety.

The product builder recognizes an explicit nominal horizontal-root/fin-tip LE
coincidence (x,z within 1e-8 m and fin-tip y=0). When this geometric relationship
exists, the horizontal root follows the fin tip's x/z displacement under tail
scaling. Identification does not depend on an aircraft name. Independently
positioned tails keep their existing translation, and the frozen reference
compatibility builder retains its historical behavior. Integration regressions
build the ATR at scales 0.7, 1.0 and 1.3 with simultaneous length/tail-shift
changes, checking LE/chord continuity and finite coordinates; another regression
checks all other presets retain their root translation. This keeps the
reference junction coherent, without modelling structural fillets or certifying
an optimized aircraft.
