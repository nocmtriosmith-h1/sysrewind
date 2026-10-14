import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import { time } from "./format.js";

export function cssVar(name) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

const tooltip = () => document.getElementById("tooltip");

/**
 * A themed time-series chart.
 * opts: { t, series: [{ label, values, color, fill }], fmt, yMax, height, bands, onSelect, extra(idx) }
 *  - yMax: fixed top of the y axis (e.g. 100 for %), otherwise auto from data
 *  - bands: [{ from, to }] shaded behind the lines (insight episodes)
 *  - extra(idx): optional HTML-free string shown at the bottom of the tooltip
 */
export function makeChart(el, opts) {
  const { t, series, fmt, yMax, height = 280, bands = [], onSelect, extra } = opts;
  const longRange = t.length > 1 && t[t.length - 1] - t[0] >= 86400;
  const axis = {
    stroke: cssVar("--muted"),
    grid: { stroke: cssVar("--grid"), width: 1 },
    ticks: { show: false },
    font: '11px "Geist Mono Variable", Consolas, monospace',
  };

  const hideTip = () => (tooltip().hidden = true);

  const u = new uPlot(
    {
      width: el.clientWidth,
      height,
      padding: [8, 8, 0, 0],
      legend: { show: false },
      cursor: {
        drag: { x: !!onSelect, y: false, setScale: false },
        points: { size: 7, fill: cssVar("--surface") },
      },
      scales: {
        x: { time: true },
        y: yMax != null ? { range: [0, yMax] } : { range: (u, min, max) => [0, Math.max(max * 1.15, 1)] },
      },
      axes: [
        { ...axis, values: (u, vals) => vals.map((v) => time(v, longRange)) },
        { ...axis, size: 64, values: (u, vals) => vals.map(fmt) },
      ],
      series: [
        {},
        ...series.map((s) => ({
          label: s.label,
          stroke: s.color,
          width: 1.6,
          fill: s.fill,
          points: { show: false },
        })),
      ],
      hooks: {
        drawAxes: [
          (u) => {
            if (!bands.length) return;
            const { ctx, bbox } = u;
            ctx.save();
            ctx.fillStyle = cssVar("--band");
            for (const b of bands) {
              const x0 = Math.max(u.valToPos(b.from, "x", true), bbox.left);
              const x1 = Math.min(u.valToPos(b.to, "x", true), bbox.left + bbox.width);
              if (x1 > x0) ctx.fillRect(x0, bbox.top, Math.max(x1 - x0, 2), bbox.height);
            }
            ctx.restore();
          },
        ],
        setCursor: [
          (u) => {
            const idx = u.cursor.idx;
            const tip = tooltip();
            if (idx == null || u.cursor.left < 0) return hideTip();
            const rows = series
              .map((s, i) => {
                const v = u.data[i + 1][idx];
                return `<div class="row"><span><i style="background:${s.color}"></i>${s.label}</span><b>${v == null ? "–" : fmt(v)}</b></div>`;
              })
              .join("");
            const foot = extra ? extra(idx) : "";
            tip.innerHTML = `<div class="mono muted">${time(u.data[0][idx], true)}</div>${rows}${
              foot ? `<div class="foot"></div>` : ""
            }`;
            if (foot) tip.querySelector(".foot").textContent = foot;
            tip.hidden = false;
            const r = u.over.getBoundingClientRect();
            let left = r.left + u.cursor.left + 16;
            if (left + tip.offsetWidth > window.innerWidth - 8) left = r.left + u.cursor.left - tip.offsetWidth - 16;
            tip.style.left = `${left}px`;
            tip.style.top = `${r.top + Math.max(u.cursor.top - 20, 0)}px`;
          },
        ],
        setSelect: [
          (u) => {
            if (!onSelect || u.select.width < 4) return;
            const from = Math.floor(u.posToVal(u.select.left, "x"));
            const to = Math.ceil(u.posToVal(u.select.left + u.select.width, "x"));
            onSelect({ from, to });
          },
        ],
      },
    },
    [t, ...series.map((s) => s.values)],
    el,
  );
  u.over.addEventListener("mouseleave", hideTip);
  return u;
}

/** Inline SVG sparkline for a KPI card. */
export function sparkline(values, color, max) {
  const pts = values.filter((v) => v != null);
  if (pts.length < 2) return "";
  const top = max ?? Math.max(...pts, 1e-9);
  // Downsample to ~60 points; a sparkline only shows the shape.
  const step = Math.max(1, Math.floor(pts.length / 60));
  const sampled = pts.filter((_, i) => i % step === 0);
  const n = sampled.length - 1;
  const coords = sampled.map((v, i) => `${((i / n) * 120).toFixed(1)},${(34 - (Math.min(v, top) / top) * 30).toFixed(1)}`);
  return `<svg class="spark" viewBox="0 0 120 36" preserveAspectRatio="none" aria-hidden="true"><polyline points="${coords.join(" ")}" stroke="${color}"/></svg>`;
}
