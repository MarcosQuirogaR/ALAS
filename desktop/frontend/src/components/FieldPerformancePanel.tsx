// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodríguez

import { useEffect, useState } from "react";
import { sidecarGet } from "../lib/sidecarClient";
import { StatTile } from "./StatTile";

// The numeric field-performance panel (V1/VR/V2 + distances) the Qt LTO widget
// shows as text. Fetched from GET /pipeline/{id}/field-performance and
// rendered as StatTile grids for the departure and arrival airports, above
// the LTO figures on the Field Performance results tab -- the same "labeled
// headline number" look as the Summary tab's metrics, instead of a plain
// table that read as a visually unrelated, less prominent presentation of
// the same kind of number.

type AirportData = {
  airport: string;
  distances_m: Record<string, number>;
  v_speeds_ms: Record<string, number>;
  v_speeds_kt: Record<string, number>;
};
type FieldPerf = { tw_sl: number; airports: Record<string, AirportData> };

const V_ORDER = ["Vstall_TO", "Vstall_land", "Vmc", "V1", "VR", "V2", "VAPP", "VTD"];
const D_ORDER = ["TODR", "BFL", "ASD", "LDR", "TODA", "LDA"];

function AirportCard({ role, d }: { role: string; d: AirportData }) {
  return (
    <div className="af-card" style={{ flex: "1 1 320px", minWidth: 300 }}>
      <div className="af-card-title">{role} — {d.airport}</div>
      <div className="af-stack" style={{ gap: 10 }}>
        <div>
          <div className="af-help" style={{ marginBottom: 4 }}>V-speeds</div>
          <div className="af-row" style={{ gap: 8, flexWrap: "wrap" }}>
            {V_ORDER.filter((k) => k in d.v_speeds_ms).map((k) => (
              <StatTile
                key={k}
                label={k}
                value={`${d.v_speeds_kt[k].toFixed(0)} kt`}
                sub={`${d.v_speeds_ms[k].toFixed(1)} m/s`}
                minWidth={90}
              />
            ))}
          </div>
        </div>
        <div>
          <div className="af-help" style={{ marginBottom: 4 }}>Distances</div>
          <div className="af-row" style={{ gap: 8, flexWrap: "wrap" }}>
            {D_ORDER.filter((k) => k in d.distances_m).map((k) => (
              <StatTile key={k} label={k} value={`${d.distances_m[k].toFixed(0)} m`} minWidth={90} />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

export function FieldPerformancePanel({ runId }: { runId: string }) {
  const [data, setData] = useState<FieldPerf | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setData(null);
    setError(null);
    sidecarGet<FieldPerf>(`/pipeline/${runId}/field-performance`)
      .then((d) => !cancelled && setData(d))
      .catch((e) => !cancelled && setError(String(e?.message ?? e)));
    return () => {
      cancelled = true;
    };
  }, [runId]);

  if (error) return null; // baseline-only / no report: silently omit
  if (!data) return <div className="af-help">Loading field performance…</div>;

  return (
    <div className="af-stack">
      <div className="af-help">Static thrust-to-weight T0/W0 = {data.tw_sl.toFixed(3)}</div>
      <div className="af-row" style={{ alignItems: "flex-start", gap: 16, flexWrap: "wrap" }}>
        {data.airports.departure && <AirportCard role="Departure" d={data.airports.departure} />}
        {data.airports.arrival && <AirportCard role="Arrival" d={data.airports.arrival} />}
      </div>
    </div>
  );
}
