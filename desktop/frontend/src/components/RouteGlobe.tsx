// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useEffect, useRef, useState } from "react";
import * as THREE from "three";
import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
import ThreeGlobe from "three-globe";
import earthTexture from "../assets/images/earth_blue_marble.jpg";
import { sidecarGet } from "../lib/sidecarClient";

// Interactive 3D route globe (Mission & Route tab): replaces the old
// server-rendered PyVista-screenshot-as-a-static-image (figure_route_globe /
// route_globe.py) with a real client-side WebGL globe -- genuinely
// rotatable/zoomable via mouse, and fast, since nothing round-trips to the
// backend per frame (route_geo_data() is fetched once, plain JSON, no
// matplotlib/PyVista rendering involved at all).

type Waypoint = { lat: number; lon: number; alt_m: number; ident: string };
type RouteGeo = {
  source: string;
  origin_ident: string;
  dest_ident: string;
  waypoints: Waypoint[];
  mass_kg: number[] | null;
  total_distance_m: number;
};

export function RouteGlobe({ runId }: { runId: string }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [route, setRoute] = useState<RouteGeo | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setRoute(null);
    setError(null);
    sidecarGet<RouteGeo>(`/pipeline/${runId}/route-geo`)
      .then((d) => !cancelled && setRoute(d))
      .catch((e) => !cancelled && setError(String(e?.message ?? e)));
    return () => {
      cancelled = true;
    };
  }, [runId]);

  useEffect(() => {
    if (!route || route.waypoints.length === 0 || !containerRef.current) return;
    const container = containerRef.current;

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(50, 1, 0.1, 10000);
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(window.devicePixelRatio || 1);
    container.appendChild(renderer.domElement);

    scene.add(new THREE.AmbientLight(0xffffff, 1.4));
    const sun = new THREE.DirectionalLight(0xffffff, 0.6);
    sun.position.set(1, 1, 1);
    scene.add(sun);

    const globe = new ThreeGlobe()
      .globeImageUrl(earthTexture)
      .showAtmosphere(true)
      .atmosphereColor("#4f8cff")
      .atmosphereAltitude(0.18);
    scene.add(globe);

    // Route/marker altitude above the surface, in globe-radius units. Scaled
    // from the real per-waypoint climb/cruise/descent altitude (route-geo's
    // `alt_m`, already projected from the SUAVE mission when one ran -- see
    // figures_extra.route_geo_data()) so the path visibly rises over cruise
    // and descends into the arrival airport, instead of sitting at a flat
    // height. maxAlt guards the still-air (no-mission) fallback where every
    // waypoint reports the same cruise altitude.
    const altScaleBase = 0.02;
    const altScaleCruise = 0.09;
    const maxAlt = Math.max(1, ...route.waypoints.map((wp) => wp.alt_m));
    const altAt = (wp: Waypoint) => altScaleBase + (wp.alt_m / maxAlt) * (altScaleCruise - altScaleBase);
    // Low altitude (departure/arrival, ground-hugging) -> warm amber; cruise
    // altitude -> cool blue -- a simple two-stop ramp readable at a glance.
    const colorAt = (wp: Waypoint) => {
      const t = Math.max(0, Math.min(1, wp.alt_m / maxAlt));
      const r = Math.round(255 + t * (79 - 255));
      const g = Math.round(153 + t * (140 - 153));
      const b = Math.round(0 + t * (255 - 0));
      return `rgb(${r},${g},${b})`;
    };

    globe
      .pathsData([route])
      .pathPoints((d: any) => (d as RouteGeo).waypoints)
      .pathPointLat((wp: any) => (wp as Waypoint).lat)
      .pathPointLng((wp: any) => (wp as Waypoint).lon)
      .pathPointAlt((wp: any) => altAt(wp as Waypoint))
      .pathColor(() => route.waypoints.map(colorAt))
      .pathStroke(1.6)
      .pathResolution(2);

    const labels = [
      { lat: route.waypoints[0].lat, lng: route.waypoints[0].lon, text: route.origin_ident, color: "#2ecc71" },
      {
        lat: route.waypoints[route.waypoints.length - 1].lat,
        lng: route.waypoints[route.waypoints.length - 1].lon,
        text: route.dest_ident,
        color: "#e74c3c",
      },
    ];
    globe
      .labelsData(labels)
      .labelLat((d: any) => d.lat)
      .labelLng((d: any) => d.lng)
      .labelText((d: any) => d.text)
      .labelColor((d: any) => d.color)
      .labelSize(1.4)
      .labelAltitude(altScaleBase + 0.01)
      .labelDotRadius(0.35)
      .labelResolution(2);

    const radius = globe.getGlobeRadius();
    const mid = route.waypoints[Math.floor(route.waypoints.length / 2)];
    const camStart = globe.getCoords(mid.lat, mid.lon, 1.9);
    camera.position.set(camStart.x, camStart.y, camStart.z);
    camera.lookAt(0, 0, 0);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.1;
    controls.minDistance = radius * 1.15;
    controls.maxDistance = radius * 6;
    controls.rotateSpeed = 0.6;

    let raf = 0;
    function animate() {
      controls.update();
      renderer.render(scene, camera);
      raf = requestAnimationFrame(animate);
    }
    animate();

    function resize() {
      const w = container.clientWidth || 1;
      const h = container.clientHeight || 1;
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
      renderer.setSize(w, h);
    }
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(container);

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      controls.dispose();
      // scene.clear() only detaches children -- geometries, materials and
      // the globe's Earth texture live in GPU memory until explicitly
      // disposed, so each run change (this effect re-runs per route) would
      // otherwise leak a full globe's worth of WebGL resources.
      scene.traverse((obj: any) => {
        obj.geometry?.dispose?.();
        const mats = Array.isArray(obj.material) ? obj.material : obj.material ? [obj.material] : [];
        for (const m of mats) {
          m.map?.dispose?.();
          m.dispose?.();
        }
      });
      scene.clear();
      renderer.dispose();
      if (renderer.domElement.parentNode === container) container.removeChild(renderer.domElement);
    };
  }, [route]);

  return (
    <div className="af-chart fill" style={{ minHeight: 420 }}>
      <div className="af-chart-head">
        <span>
          3D Route Globe{route ? ` — ${route.origin_ident} → ${route.dest_ident}` : ""}
        </span>
        {route && (
          <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span style={{ opacity: 0.8 }}>Altitude:</span>
            <span style={{ width: 46, height: 8, borderRadius: 4, background: "linear-gradient(90deg,#ff9900,#4f8cff)" }} />
            <span style={{ opacity: 0.8 }}>{Math.round(Math.max(...route.waypoints.map((wp) => wp.alt_m)) * 3.28084).toLocaleString()} ft</span>
          </span>
        )}
      </div>
      {error ? (
        <div className="af-placeholder">{error.includes("404") ? "Not available for this run." : error}</div>
      ) : !route ? (
        <div className="af-placeholder">loading…</div>
      ) : (
        <div ref={containerRef} style={{ flex: 1, minHeight: 320, cursor: "grab" }} />
      )}
      <div className="af-help" style={{ textAlign: "center" }}>Drag to rotate - scroll to zoom</div>
    </div>
  );
}
