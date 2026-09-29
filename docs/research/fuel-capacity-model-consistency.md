# Fuel capacity used by search, dispatch and balance

The shared `alas_mass::tanks::resolve_product_layout` is the product inventory
used by optimizer capacity, the optimizer balance ledger, product fuel
centroids, final dispatch capacity, final mass balance and operational-envelope
fuel vectors. Invalid inventories return missing evidence, not a synthetic
wing-only capacity.

For an unchanged registered tank arrangement, the existing `resolve_scaled`
method transfers each published cell from reference to candidate using the
ratio of its geometric candidate and reference usable volumes. This is a
conceptual calibration assumption, not a manufacturer validation of modified
wings. Declared-volume auxiliary cells keep their declared volume. The
geometric reference uses the registered design vector with the live geometry
scaffold, as the previous search-capacity path did. Fuel density is the live
configured density, consistently across these consumers.

If the user edits the tank arrangement, its cell declarations are resolved as
requested, without applying the registered total or restoring deleted tanks.
Published volumes explicitly left on a custom cell remain declared evidence;
users wanting a geometric custom cell must clear that published-volume field.
An exact reference design with the registered layout and its default density
retains the published-mass capacity/provenance in the final assessment, after
successful inventory resolution. This reference ceiling may differ slightly
from the sum of estimated per-cell volumes; it is not used as the capacity of
an optimized geometry. A changed density uses the resolved inventory.

## DC-10 failure evidence

A saved DC-10 optimisation result (measurement result and design database, not tracked)
showed a 6,451.47 kg reserve shortfall:
77,828.18 kg requested takeoff fuel versus 71,376.71 kg available after taxi.
The old final gate estimated only wing volume (71,728.79 kg before taxi),
whereas its mass ledger retained all published cells (111,542.94 kg including
45,317.46 kg auxiliary). These were different inventories for one candidate.
The optimizer capacity already scaled reference cells, while its balance
ledger did not. This disagreement invalidates interpreting that rejection as
proof of real DC-10 tank or route infeasibility.

The dedicated integration regression reconstructs the saved candidate's main
wing dimensions, sweep and thickness factor (other parameters remain nominal),
and obtains 97,084.48 kg from the shared inventory. The assessment and mass
ledger agree exactly within floating-point summation tolerance. This is a
numerical consistency check, not a replay or acceptance of the complete saved
mission. Route, payload, engine and reserve assumptions still require their own
validation; no reserve threshold or physical limit was changed.

Tests cover all eight preset inventories under reduced span/chord, positive
capacity, fuel-item mass closure, overfill rejection, unchanged auxiliary
volume, and custom auxiliary removal. The final-report regression also checks
dispatch/ledger capacity equality and custom-layout provenance.
