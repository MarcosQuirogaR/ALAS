# SPDX-License-Identifier: AGPL-3.0-or-later
# Copyright (C) 2026 Marcos Quiroga Rodriguez

"""Normalize the supplied APC legacy tables into bounded Rust fixtures.

The legacy electrical project stores 436 propeller performance files in APC's
fixed-width ``.dat`` layout. The generator keeps the 12x6E parity fixture and
also emits an exact-decimal compact bundle plus normalized propeller records
for every non-duplicated table. Retaining dimensionless Ct/Cp samples, rather
than an intermediate spreadsheet, makes the source data auditable and lets the
Rust solver apply the requested air density without inventing a static-thrust
curve.

This generator performs no network access and never modifies the supplied
table.  Its expected cases use the legacy ideal battery/ESC assumptions:
fixed 3S nominal voltage, zero no-load current, and no controller loss.  The
Rust implementation keeps those assumptions explicit while using a bracketed
solve instead of the legacy open-loop update.
"""

from __future__ import annotations

import json
import math
import re
import struct
from decimal import Decimal, InvalidOperation
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
SOURCE = ROOT / "golden" / "prop_elec" / "Script Python" / "Data" / "PER3_12x6E.dat"
DATA_DIRECTORY = SOURCE.parent
OUTPUT = ROOT / "golden" / "prop_elec" / "apc_12x6e_fixture.json"
APC_TABLE_OUTPUT = ROOT / "golden" / "prop_elec" / "apc_performance_tables.bin"
APC_CATALOG_OUTPUT = ROOT / "crates" / "alas-uav" / "data" / "apc_performance_catalog.json"
MULTI_SOURCE_CATALOG = ROOT / "crates" / "alas-uav" / "data" / "multi_source_catalog.json"
RPM_RE = re.compile(r"PROP RPM\s*=\s*(\d+)")
MODEL_RE = re.compile(r"^\s*(?P<diameter>\d+(?:\.\d+)?)x(?P<pitch>\d+(?:\.\d+)?)")
MPH_TO_M_S = 0.44704
REFERENCE_DENSITY_KG_M3 = 1.225
DIAMETER_M = 0.3048
TABLE_MAGIC = b"ALASAPC1"
TABLE_SCHEMA_VERSION = 1
APC_PERFORMANCE_DATA_URL = "https://www.apcprop.com/technical-information/performance-data/"


def parse_curves() -> list[dict[str, object]]:
    """Read the Ct/Cp grid without relying on locale-dependent spreadsheets."""

    curves: list[dict[str, object]] = []
    rpm: int | None = None
    points: list[dict[str, float]] = []

    def finish_curve() -> None:
        nonlocal points
        if rpm is not None and points:
            curves.append({"rpm": rpm, "points": points})
        points = []

    for line in SOURCE.read_text(encoding="ascii").splitlines():
        match = RPM_RE.search(line)
        if match is not None:
            finish_curve()
            rpm = int(match.group(1))
            continue
        if rpm is None:
            continue
        fields = line.split()
        if len(fields) < 5:
            continue
        try:
            speed_m_s = float(fields[0]) * MPH_TO_M_S
            thrust_coefficient = float(fields[3])
            power_coefficient = float(fields[4])
        except ValueError:
            continue
        points.append(
            {
                "speed_m_s": speed_m_s,
                "thrust_coefficient": thrust_coefficient,
                "power_coefficient": power_coefficient,
            }
        )
    finish_curve()

    if len(curves) < 2:
        raise SystemExit(f"{SOURCE} did not contain enough RPM curves")
    for curve in curves:
        samples = curve["points"]
        assert isinstance(samples, list)
        if any(
            samples[index]["speed_m_s"] >= samples[index + 1]["speed_m_s"]
            for index in range(len(samples) - 1)
        ):
            raise SystemExit(f"RPM {curve['rpm']} has unordered velocity samples")
    return curves


def normalized_id(model: str) -> str:
    """Return the catalogue identifier shared by the table and propeller record."""

    slug = re.sub(r"[^a-z0-9]+", "-", model.lower()).strip("-")
    if not slug:
        raise SystemExit(f"could not make an APC identifier from {model!r}")
    return f"apc-{slug}"


def table_identifier(model: str, path: Path) -> str:
    """Preserve an F2B source-file variant when its printed model is shared."""

    identifier = normalized_id(model)
    suffix_match = re.search(r"\(([^)]+)\)", path.stem)
    if suffix_match is None:
        return identifier
    suffix = re.sub(r"[^a-z0-9]+", "-", suffix_match.group(1).lower()).strip("-")
    if not suffix:
        raise SystemExit(f"could not make an APC table suffix from {path.name}")
    return f"{identifier}-{suffix}"


def scaled_decimal(value: str, scale: int, context: str) -> int:
    """Convert a fixed-decimal source token without a binary rounding change."""

    scaled = Decimal(value) * scale
    integral = int(scaled)
    if scaled != integral:
        raise SystemExit(f"{context} is not representable at scale {scale}: {value}")
    return integral


def bounded_unsigned(value: int, bits: int, context: str) -> int:
    maximum = (1 << bits) - 1
    if value < 0 or value > maximum:
        raise SystemExit(f"{context} does not fit in u{bits}: {value}")
    return value


def bounded_signed(value: int, bits: int, context: str) -> int:
    minimum = -(1 << (bits - 1))
    maximum = (1 << (bits - 1)) - 1
    if value < minimum or value > maximum:
        raise SystemExit(f"{context} does not fit in i{bits}: {value}")
    return value


def parse_compact_table(path: Path) -> dict[str, object]:
    """Parse one APC table in the compact exact-decimal representation."""

    lines = path.read_text(encoding="ascii").splitlines()
    if not lines:
        raise SystemExit(f"{path} is empty")
    heading = lines[0].split("(", maxsplit=1)[0].strip()
    dimensions = MODEL_RE.match(heading)
    if dimensions is None:
        raise SystemExit(f"{path} has no parseable diameter and pitch in {heading!r}")
    diameter_10k_in = scaled_decimal(dimensions.group("diameter"), 10_000, str(path))
    pitch_10k_in = scaled_decimal(dimensions.group("pitch"), 10_000, str(path))
    curves: list[tuple[int, list[tuple[int, int, int]]]] = []
    rpm: int | None = None
    points: list[tuple[int, int, int]] = []

    def finish_curve() -> None:
        nonlocal points
        if rpm is not None and points:
            curves.append((rpm, points))
        points = []

    for line in lines:
        match = RPM_RE.search(line)
        if match is not None:
            finish_curve()
            rpm = int(match.group(1))
            continue
        if rpm is None:
            continue
        fields = line.split()
        if len(fields) < 5:
            continue
        try:
            speed_centimph = scaled_decimal(fields[0], 100, str(path))
            thrust_1e4 = scaled_decimal(fields[3], 10_000, str(path))
            power_1e4 = scaled_decimal(fields[4], 10_000, str(path))
        except (ArithmeticError, InvalidOperation, ValueError):
            continue
        points.append((speed_centimph, thrust_1e4, power_1e4))
    finish_curve()
    if len(curves) < 2:
        raise SystemExit(f"{path} did not contain enough RPM curves")
    return {
        "id": table_identifier(heading, path),
        "model": heading,
        "source_file": path.name,
        "diameter_10k_in": diameter_10k_in,
        "pitch_10k_in": pitch_10k_in,
        "curves": curves,
    }


def write_u8(buffer: bytearray, value: int, context: str) -> None:
    buffer.extend(struct.pack("<B", bounded_unsigned(value, 8, context)))


def write_u16(buffer: bytearray, value: int, context: str) -> None:
    buffer.extend(struct.pack("<H", bounded_unsigned(value, 16, context)))


def write_u32(buffer: bytearray, value: int, context: str) -> None:
    buffer.extend(struct.pack("<I", bounded_unsigned(value, 32, context)))


def write_i16(buffer: bytearray, value: int, context: str) -> None:
    buffer.extend(struct.pack("<h", bounded_signed(value, 16, context)))


def write_text(buffer: bytearray, value: str, context: str) -> None:
    encoded = value.encode("ascii")
    write_u8(buffer, len(encoded), f"{context} length")
    buffer.extend(encoded)


def write_apc_table_bundle(tables: list[dict[str, object]]) -> None:
    """Store every supplied table without the source text's repeated columns."""

    output = bytearray(TABLE_MAGIC)
    write_u16(output, TABLE_SCHEMA_VERSION, "table schema")
    write_u16(output, len(tables), "table count")
    for table in tables:
        identifier = table["id"]
        model = table["model"]
        source_file = table["source_file"]
        curves = table["curves"]
        assert isinstance(identifier, str)
        assert isinstance(model, str)
        assert isinstance(source_file, str)
        assert isinstance(curves, list)
        write_text(output, identifier, "identifier")
        write_text(output, model, "model")
        write_text(output, source_file, "source file")
        write_u32(output, int(table["diameter_10k_in"]), "diameter")
        write_u32(output, int(table["pitch_10k_in"]), "pitch")
        write_u16(output, len(curves), "curve count")
        for rpm, points in curves:
            write_u32(output, rpm, "RPM")
            write_u16(output, len(points), "sample count")
            previous_speed = -1
            for speed_centimph, thrust_1e4, power_1e4 in points:
                if speed_centimph <= previous_speed:
                    raise SystemExit(f"{identifier} has unordered speed data")
                previous_speed = speed_centimph
                write_u16(output, speed_centimph, "speed")
                write_i16(output, thrust_1e4, "thrust coefficient")
                write_i16(output, power_1e4, "power coefficient")
    APC_TABLE_OUTPUT.write_bytes(output)


def electric_designation(model: str) -> bool | None:
    """Retain only the explicit APC E-series compatibility designation."""

    return True if "E" in model.upper() else None


def write_apc_catalog(tables: list[dict[str, object]]) -> None:
    """Create source-backed catalogue records for tables absent from the review."""

    existing_catalog = json.loads(MULTI_SOURCE_CATALOG.read_text(encoding="utf-8"))
    existing_ids = {record["id"] for record in existing_catalog["records"]}
    records = []
    for table in tables:
        identifier = table["id"]
        model = table["model"]
        source_file = table["source_file"]
        assert isinstance(identifier, str)
        assert isinstance(model, str)
        assert isinstance(source_file, str)
        if identifier in existing_ids:
            continue
        records.append(
            {
                "id": identifier,
                "manufacturer": "APC Propellers",
                "model": model,
                "kind": "propeller",
                "spec": {
                    "diameter_m": int(table["diameter_10k_in"]) / 10_000 * 0.0254,
                    "pitch_m": int(table["pitch_10k_in"]) / 10_000 * 0.0254,
                    "blade_count": None,
                    "mass_kg": None,
                    "bore_diameter_m": None,
                    "electric_compatible": electric_designation(model),
                },
                "provenance": {
                    "publisher": "APC Propellers",
                    "source_url": APC_PERFORMANCE_DATA_URL,
                    "source_title": f"APC performance data: {source_file}",
                    "transformations": [
                        "diameter and pitch multiplied by 0.0254 from the official performance-file title in inches to m",
                        "Ct/Cp performance data are stored separately in golden/prop_elec/apc_performance_tables.bin without extrapolation",
                        "E-series electric compatibility is retained only when the APC model designation contains E; other compatibility fields remain absent",
                    ],
                },
            }
        )
    catalog = {"schema_version": 1, "records": records}
    APC_CATALOG_OUTPUT.write_text(json.dumps(catalog, indent=2) + "\n", encoding="utf-8")


def interpolate(lower: float, upper: float, fraction: float) -> float:
    return lower + fraction * (upper - lower)


def curve_at_speed(curve: dict[str, object], speed_m_s: float) -> tuple[float, float] | None:
    points = curve["points"]
    assert isinstance(points, list)
    if speed_m_s < points[0]["speed_m_s"] or speed_m_s > points[-1]["speed_m_s"]:
        return None
    for lower, upper in zip(points, points[1:]):
        if lower["speed_m_s"] <= speed_m_s <= upper["speed_m_s"]:
            span = upper["speed_m_s"] - lower["speed_m_s"]
            fraction = 0.0 if span == 0.0 else (speed_m_s - lower["speed_m_s"]) / span
            return (
                interpolate(lower["thrust_coefficient"], upper["thrust_coefficient"], fraction),
                interpolate(lower["power_coefficient"], upper["power_coefficient"], fraction),
            )
    return None


def evaluate(
    curves: list[dict[str, object]], rpm: float, speed_m_s: float
) -> tuple[float, float] | None:
    available: list[tuple[float, float, float]] = []
    for curve in curves:
        value = curve_at_speed(curve, speed_m_s)
        if value is not None:
            available.append((float(curve["rpm"]), *value))
    if len(available) < 2 or rpm < available[0][0] or rpm > available[-1][0]:
        return None
    for lower, upper in zip(available, available[1:]):
        if lower[0] <= rpm <= upper[0]:
            span = upper[0] - lower[0]
            fraction = 0.0 if span == 0.0 else (rpm - lower[0]) / span
            return (
                interpolate(lower[1], upper[1], fraction),
                interpolate(lower[2], upper[2], fraction),
            )
    return None


def solve_legacy_case(curves: list[dict[str, object]], speed_m_s: float) -> dict[str, float]:
    """Solve the equations implemented by the legacy ideal open-loop model."""

    voltage_v = 11.4
    kv_rpm_per_v = 900.0
    motor_resistance_ohm = 0.082
    torque_constant_nm_per_a = 60.0 / (2.0 * math.pi * kv_rpm_per_v)
    available = [
        float(curve["rpm"])
        for curve in curves
        if curve_at_speed(curve, speed_m_s) is not None
    ]
    if len(available) < 2:
        raise SystemExit(f"legacy case at {speed_m_s} m/s has no interpolable RPM range")

    def values(rpm: float) -> tuple[float, float, float, float]:
        coefficients = evaluate(curves, rpm, speed_m_s)
        if coefficients is None:
            raise SystemExit("solver queried outside the table")
        thrust_coefficient, power_coefficient = coefficients
        revolutions_per_s = rpm / 60.0
        thrust_n = (
            REFERENCE_DENSITY_KG_M3
            * thrust_coefficient
            * revolutions_per_s**2
            * DIAMETER_M**4
        )
        torque_nm = (
            REFERENCE_DENSITY_KG_M3
            * power_coefficient
            * revolutions_per_s**2
            * DIAMETER_M**5
            / (2.0 * math.pi)
        )
        motor_current_a = torque_nm / torque_constant_nm_per_a
        residual_v = voltage_v - rpm / kv_rpm_per_v - motor_current_a * motor_resistance_ohm
        return residual_v, thrust_n, torque_nm, motor_current_a

    lower = available[0]
    upper = available[-1]
    lower_residual = values(lower)[0]
    upper_residual = values(upper)[0]
    if lower_residual < 0.0 or upper_residual > 0.0:
        raise SystemExit(f"legacy case at {speed_m_s} m/s is outside the source table")
    for _ in range(96):
        midpoint = 0.5 * (lower + upper)
        residual = values(midpoint)[0]
        if residual > 0.0:
            lower = midpoint
        else:
            upper = midpoint
    rpm = 0.5 * (lower + upper)
    _, thrust_n, torque_nm, motor_current_a = values(rpm)
    shaft_power_w = torque_nm * rpm * 2.0 * math.pi / 60.0
    return {
        "rpm": rpm,
        "motor_current_a": motor_current_a,
        "thrust_n": thrust_n,
        "shaft_power_w": shaft_power_w,
        "battery_power_w": voltage_v * motor_current_a,
    }


def main() -> None:
    curves = parse_curves()
    cases = []
    for name, speed_m_s in [
        ("legacy static full throttle", 0.0),
        ("legacy climb full throttle", 10.0),
        ("legacy cruise full throttle", 20.0),
    ]:
        cases.append(
            {
                "name": name,
                "speed_m_s": speed_m_s,
                "battery_voltage_v": 11.4,
                "throttle": 1.0,
                "motor_kv_rpm_per_v": 900.0,
                "motor_resistance_ohm": 0.082,
                "motor_no_load_current_a": 0.0,
                "expected": solve_legacy_case(curves, speed_m_s),
            }
        )
    fixture = {
        "schema_version": 1,
        "source": {
            "legacy_file": "golden/prop_elec/Script Python/Data/PER3_12x6E.dat",
            "propeller_id": "apc-12x6e",
            "diameter_m": DIAMETER_M,
            "reference_density_kg_m3": REFERENCE_DENSITY_KG_M3,
            "normalization": [
                "speed multiplied by 0.44704 from mph to m/s",
                "Ct and Cp copied from columns 4 and 5 of the supplied table",
                "the fixture keeps the table's dimensionless coefficients so the native solver can scale thrust and torque by density",
            ],
        },
        "curves": curves,
        "legacy_ideal_cases": cases,
    }
    OUTPUT.write_text(json.dumps(fixture, indent=2) + "\n", encoding="utf-8")
    tables = [parse_compact_table(path) for path in sorted(DATA_DIRECTORY.glob("PER3_*.dat"))]
    if len(tables) != 436:
        raise SystemExit(f"expected 436 supplied APC tables, found {len(tables)}")
    if len({table["id"] for table in tables}) != len(tables):
        raise SystemExit("APC table identifiers are not unique")
    write_apc_table_bundle(tables)
    write_apc_catalog(tables)


if __name__ == "__main__":
    main()
