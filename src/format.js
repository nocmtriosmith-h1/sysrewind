import { getLang, t } from "./i18n.js";

export const now = () => Math.floor(Date.now() / 1000);

export function pct(v) {
  if (v == null || Number.isNaN(v)) return "–";
  return `${v.toFixed(v < 10 ? 1 : 0)}%`;
}

export function bytes(b) {
  if (b == null || Number.isNaN(b)) return "–";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  while (b >= 1024 && i < units.length - 1) {
    b /= 1024;
    i++;
  }
  return `${b.toFixed(b < 10 && i > 0 ? 1 : 0)} ${units[i]}`;
}

export const rate = (b) => (b == null ? "–" : `${bytes(b)}/s`);

export const celsius = (v) => (v == null ? "–" : `${Math.round(v)}°C`);

function locale() {
  return getLang() === "vi" ? "vi-VN" : "en-GB";
}

export function time(ts, withDate = false) {
  const d = new Date(ts * 1000);
  const hm = d.toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });
  return withDate ? `${d.toLocaleDateString(locale(), { day: "2-digit", month: "2-digit" })} ${hm}` : hm;
}

export function longDate(ts) {
  return new Date(ts * 1000).toLocaleDateString(locale(), { weekday: "long", day: "numeric", month: "long" });
}

export function duration(secs) {
  const d = t("dur");
  if (secs < 60) return d.s(Math.round(secs));
  const mins = Math.round(secs / 60);
  if (mins < 60) return d.m(mins);
  return d.h(Math.floor(mins / 60), mins % 60);
}

/** Stable color per app name so the same app always gets the same tile. */
export function appColor(name) {
  const palette = ["#1D5FD6", "#2E7D32", "#B85C38", "#7B4FD6", "#0E7C86", "#C2410C", "#A23B72", "#3F6212"];
  let h = 0;
  for (const ch of name) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
  return palette[h % palette.length];
}

/** `<input type="datetime-local">` value <-> unix seconds, in local time. */
export function toLocalInput(ts) {
  const d = new Date(ts * 1000);
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}
export const fromLocalInput = (v) => Math.floor(new Date(v).getTime() / 1000);
