import copy
import json
from pathlib import Path

import pytest

from cabin_renderer import SceneError, load_scene, render_svg
from cabin_renderer.model import recommended_scene_set, recommended_scenes
from cabin_renderer.render import render_recommended_svg
from cabin_renderer.validate import validate_scene

FIXTURE = Path(__file__).parent / "fixtures" / "a320_export_v2.json"


def raw():
    return json.loads(FIXTURE.read_text(encoding="utf-8"))


def test_valid_fixture_and_deterministic_svg():
    scene = load_scene(FIXTURE)
    assert not any(f.code == "OHCP_NOT_ATTACHED" for f in validate_scene(scene))
    assert any(d["field"] == "overhead.topology" for d in scene.metadata["diagnostics"])
    first, _ = render_svg(scene)
    second, _ = render_svg(scene)
    assert first == second
    assert "AKH · LD3-45W" in first
    assert 'data-schema="alas.cabin-scene/v2"' in first


def test_rejects_wrong_version_and_nonfinite():
    data = raw()
    data["schema_version"] = "alas.cabin-scene/v1"
    with pytest.raises(SceneError):
        load_scene(data)
    data = raw()
    data["stations"][0]["x_m"] = float("nan")
    with pytest.raises(SceneError, match="non-finite"):
        load_scene(data)


def test_detects_occupant_bin_collision_without_scaling():
    data = raw()
    data["seat_rows"][0]["envelope"]["height_m"] = 2.4
    findings = validate_scene(load_scene(data))
    assert any(f.code == "SEAT_OHCP_INTERSECTION" for f in findings)


def test_detects_uld_overlap_and_detached_bin():
    data = raw()
    item = copy.deepcopy(data["cargo"]["items"][0]); item["id"] = "cargo-2"; item["slot_id"] = "slot-2"
    slot = copy.deepcopy(data["cargo"]["slots"][0]); slot["id"] = "slot-2"; slot["occupied_by"] = "cargo-2"
    data["cargo"]["items"].append(item); data["cargo"]["slots"].append(slot)
    findings = validate_scene(load_scene(data))
    assert any(f.code == "ULD_OVERLAP" for f in findings)
    assert not any(f.code == "OHCP_NOT_ATTACHED" for f in findings)


def test_station_and_deck_selection_are_strict():
    with pytest.raises(SceneError, match="unavailable"):
        load_scene(FIXTURE, station_x_m=11)
    scene = load_scene(FIXTURE, station_x_m=10, deck_ids={"lower"})
    assert not scene.by_kind("seat") and len(scene.by_kind("uld")) == 1


def test_recommended_board_is_deterministic_and_uses_one_shell_station():
    passenger, hold = recommended_scenes(FIXTURE)
    assert hold is None and passenger.station_m == 10
    assert passenger.metadata["section_invariant"] == {"station_x_m": 10, "all_components_same_station": True}
    first, _ = render_recommended_svg(passenger, hold)
    second, _ = render_recommended_svg(passenger, hold)
    assert first == second
    assert "Hold-liner area used" in first
    assert "Unused usable hold cross-sectional area" in first
    assert 'data-fidelity="visualization_only"' in first


def test_real_a380_recommends_one_panel_for_every_deck_and_hold():
    source = Path(__file__).parents[3] / "outputs" / "cabin_scene_v2_fixtures" / "a380_800_cabin_scene_v2.json"
    passengers, hold = recommended_scene_set(source)
    assert [p.metadata["selected_decks"] for p in passengers] == [["lower", "main", "upper"]]
    assert hold is None
    svg, _ = render_recommended_svg(passengers, hold)
    assert "floor-main" in svg and "floor-upper" in svg and "floor-lower" in svg
    assert "visual-center-ohsc-main" in svg


def test_cross_sectional_hold_utilization_is_area_not_item_fill():
    scene, _ = recommended_scenes(FIXTURE)
    metrics = scene.metadata["cargo_metrics"]
    assert metrics["transverse_area_utilization"] == pytest.approx(1.48 / 2.025)
    assert metrics["longitudinal_fill_fraction"] == pytest.approx(.8)
    assert metrics["unused_hold_geometry"].area == pytest.approx(metrics["usable_hold_area_m2"] - 1.48)


def test_center_ohsc_requires_two_aisles_and_center_block():
    narrow, _ = recommended_scenes(FIXTURE)
    assert not any(c.id.startswith("visual-center-ohsc") for c in narrow.components)
    data = raw()
    data["seat_rows"][0]["blocks"] = [1, 2, 1]
    wide, _ = recommended_scenes(data)
    center = [c for c in wide.components if c.id.startswith("visual-center-ohsc")]
    assert len(center) == 1
    assert center[0].data["fidelity"] == "visualization_only_topology_fallback"


def test_unified_selection_fails_without_common_station():
    data = raw()
    data["stations"].append(copy.deepcopy(data["stations"][0]))
    data["stations"][1]["x_m"] = 20
    data["cargo"]["items"][0]["envelope"]["center_x_m"] = 20
    data["cargo"]["slots"][0]["envelope"]["center_x_m"] = 20
    with pytest.raises(SceneError, match="unified transverse section"):
        recommended_scene_set(data)
