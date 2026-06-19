/* DAEDALUS — icons. Minimal geometric line/solid glyphs (SF-symbol-like). */
const S = { fill: "none", stroke: "currentColor", strokeWidth: 1.6, strokeLinecap: "round", strokeLinejoin: "round" };

// status glyphs (icon-led / glyph) — distinct shape per status (color-blind safe)
const STATUS_GLYPH = {
  starting: <g {...S}><circle cx="12" cy="12" r="7.5" strokeDasharray="6 5" /></g>,
  running:  <g><circle cx="12" cy="12" r="6.5" fill="currentColor" /></g>,
  stalled:  <g {...S}><path d="M12 4.5 21 19H3z" /><path d="M12 10.5v3.5" /><circle cx="12" cy="16.6" r=".3" fill="currentColor" stroke="currentColor" /></g>,
  failed:   <g {...S}><path d="M8 5h8l3.5 3.5v8L16 20H8l-3.5-3.5v-8z" /><path d="M9.5 9.5l5 5M14.5 9.5l-5 5" /></g>,
  completed:<g {...S}><circle cx="12" cy="12" r="8" /><path d="M8.3 12.2l2.6 2.6 4.8-5.2" /></g>,
  stopped:  <g {...S}><rect x="6" y="6" width="12" height="12" rx="2.5" /></g>,
  unknown:  <g {...S}><circle cx="12" cy="12" r="8" strokeDasharray="3 3" /><path d="M9.6 9.6a2.4 2.4 0 1 1 3.2 2.3c-.8.3-1.3 .9-1.3 1.8" /><circle cx="11.5" cy="16.4" r=".2" /></g>,
  unreachable:<g {...S}><circle cx="12" cy="12" r="8" /><path d="M7 7l10 10" /></g>,
};

const PATHS = {
  // nav
  fleet:    <g {...S}><rect x="3.5" y="4" width="7" height="7" rx="1.6" /><rect x="13.5" y="4" width="7" height="7" rx="1.6" /><rect x="3.5" y="14" width="7" height="6" rx="1.6" /><rect x="13.5" y="14" width="7" height="6" rx="1.6" /></g>,
  board:    <g {...S}><rect x="3.5" y="4" width="4.6" height="16" rx="1.4" /><rect x="9.7" y="4" width="4.6" height="11" rx="1.4" /><rect x="15.9" y="4" width="4.6" height="14" rx="1.4" /></g>,
  maze:     <g {...S}><path d="M9.5 20 L4 20 L4 4 L20 4 L20 20 L14.5 20" /><path d="M10 8.5 L8 8.5 L8 15.5 L16 15.5 L16 8.5 L14 8.5" /><circle cx="12" cy="12" r="1.4" fill="currentColor" stroke="none" /></g>,
  gpu:      <g {...S}><rect x="3" y="6" width="18" height="12" rx="2" /><rect x="6.5" y="9.5" width="5.5" height="5" rx="1" /><circle cx="16" cy="12" r="2" /><path d="M6 18v2M12 18v2M18 18v2" /></g>,
  tag:      <g {...S}><path d="M4 12.5V5.4C4 4.6 4.6 4 5.4 4H12l8 8-7.2 7.2a1.2 1.2 0 0 1-1.7 0L4 12.5z" /><circle cx="8" cy="8" r="1.3" fill="currentColor" stroke="none" /></g>,
  discover: <g {...S}><circle cx="11" cy="11" r="6.5" /><path d="M16 16l4 4" /></g>,
  tools:    <g {...S}><path d="M14.5 6.5a3.5 3.5 0 0 0-4.8 4.5L4 16.8 7.2 20l5.8-5.7a3.5 3.5 0 0 0 4.5-4.8l-2.3 2.3-2.1-2.1z" /></g>,
  backends: <g {...S}><rect x="3.5" y="5" width="17" height="6" rx="1.8" /><rect x="3.5" y="13" width="17" height="6" rx="1.8" /><path d="M7 8h.01M7 16h.01" /></g>,
  host:     <g {...S}><rect x="4" y="4" width="16" height="12" rx="1.8" /><path d="M9 20h6M12 16v4" /><path d="M7.5 7.5h6" /></g>,
  settings: <g {...S}><circle cx="12" cy="12" r="3" /><path d="M12 3v2.5M12 18.5V21M3 12h2.5M18.5 12H21M5.6 5.6l1.8 1.8M16.6 16.6l1.8 1.8M18.4 5.6l-1.8 1.8M7.4 16.6l-1.8 1.8" /></g>,
  // chrome / actions
  bell:     <g {...S}><path d="M6 9a6 6 0 0 1 12 0c0 5 1.5 6 1.5 6h-15S6 14 6 9z" /><path d="M10 19a2 2 0 0 0 4 0" /></g>,
  search:   <g {...S}><circle cx="11" cy="11" r="6.5" /><path d="M16 16l4 4" /></g>,
  plus:     <g {...S}><path d="M12 5v14M5 12h14" /></g>,
  stop:     <g {...S}><rect x="6.5" y="6.5" width="11" height="11" rx="2" /></g>,
  send:     <g {...S}><path d="M4 12h13M12 6l6 6-6 6" /></g>,
  cleanup:  <g {...S}><path d="M5 7h14M9 7V5h6v2M7 7l1 13h8l1-13" /></g>,
  chevR:    <g {...S}><path d="M9 5l7 7-7 7" /></g>,
  chevD:    <g {...S}><path d="M5 9l7 7 7-7" /></g>,
  chevL:    <g {...S}><path d="M15 5l-7 7 7 7" /></g>,
  close:    <g {...S}><path d="M6 6l12 12M18 6L6 18" /></g>,
  minus:    <g {...S}><path d="M5 12h14" /></g>,
  square:   <g {...S}><rect x="6" y="6" width="12" height="12" rx="1.4" /></g>,
  check:    <g {...S}><path d="M5 12.5l4.5 4.5L19 6.5" /></g>,
  copy:     <g {...S}><rect x="8" y="8" width="11" height="11" rx="2.2" /><path d="M5 16V6.2C5 5.5 5.5 5 6.2 5H16" /></g>,
  jumpdown: <g {...S}><path d="M12 5v12M6 12l6 6 6-6" /><path d="M5 20h14" /></g>,
  command:  <g {...S}><path d="M9 6.5A2.5 2.5 0 1 0 6.5 9H9V6.5zM15 6.5A2.5 2.5 0 1 1 17.5 9H15V6.5zM9 17.5A2.5 2.5 0 1 1 6.5 15H9v2.5zM15 17.5a2.5 2.5 0 1 0 2.5-2.5H15v2.5zM9 9h6v6H9z" /></g>,
  filter:   <g {...S}><path d="M4 6h16M7 12h10M10 18h4" /></g>,
  grid:     <g {...S}><rect x="4" y="4" width="7" height="7" rx="1.4" /><rect x="13" y="4" width="7" height="7" rx="1.4" /><rect x="4" y="13" width="7" height="7" rx="1.4" /><rect x="13" y="13" width="7" height="7" rx="1.4" /></g>,
  list:     <g {...S}><path d="M4 6h16M4 12h16M4 18h16" /></g>,
  table:    <g {...S}><rect x="4" y="5" width="16" height="14" rx="1.6" /><path d="M4 10h16M10 5v14" /></g>,
  input:    <g {...S}><path d="M5 7h14v10H5z" /><path d="M8 12h2M14 10v4" /></g>,
  cpu:      <g {...S}><rect x="7" y="7" width="10" height="10" rx="1.6" /><path d="M10 4v3M14 4v3M10 17v3M14 17v3M4 10h3M4 14h3M17 10h3M17 14h3" /></g>,
  mem:      <g {...S}><rect x="4" y="8" width="16" height="9" rx="1.6" /><path d="M7 17v2M12 17v2M17 17v2M8 11v3M12 11v3M16 11v3" /></g>,
  disk:     <g {...S}><circle cx="12" cy="12" r="8" /><circle cx="12" cy="12" r="2" /></g>,
  clock:    <g {...S}><circle cx="12" cy="12" r="8" /><path d="M12 7.5V12l3 2" /></g>,
  bolt:     <g {...S}><path d="M13 3L5 13h6l-1 8 8-10h-6z" /></g>,
  // backends / sources
  linux:    <g {...S}><ellipse cx="12" cy="14" rx="5.5" ry="6.5" /><path d="M9.5 11.5c.4-2 .8-4 2.5-4s2.1 2 2.5 4" /><circle cx="10.4" cy="10.5" r=".4" fill="currentColor" /><circle cx="13.6" cy="10.5" r=".4" fill="currentColor" /><path d="M11 13l1 1 1-1" /></g>,
  apple:    <g><path fill="currentColor" d="M16.4 12.6c0-1.9 1.5-2.8 1.6-2.9-.9-1.3-2.3-1.5-2.8-1.5-1.2-.1-2.3.7-2.9.7s-1.5-.7-2.5-.7c-1.3 0-2.5.8-3.1 2-1.3 2.3-.3 5.7 1 7.6.6 1 1.3 2 2.3 1.9.9 0 1.3-.6 2.4-.6s1.4.6 2.4.6 1.6-.9 2.2-1.8c.5-.8.8-1.6.8-1.6-.1 0-1.6-.6-1.6-2.3z" /><path fill="currentColor" d="M14.2 6.6c.5-.7.9-1.6.8-2.5-.8 0-1.7.5-2.3 1.2-.5.6-.9 1.4-.8 2.3.9.1 1.7-.4 2.3-1z" /></g>,
  tunnel:   <g {...S}><path d="M4 12c0-4 3.5-6 8-6s8 2 8 6" /><path d="M7 12v5M12 12v6M17 12v5" /><circle cx="12" cy="9" r="1.4" /></g>,
  shield:   <g {...S}><path d="M12 3l7 3v5c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6z" /><path d="M9 12l2 2 4-4" /></g>,
  globe:    <g {...S}><circle cx="12" cy="12" r="8" /><path d="M4 12h16M12 4c2.5 2.2 2.5 13.8 0 16M12 4c-2.5 2.2-2.5 13.8 0 16" /></g>,
  git:      <g {...S}><circle cx="7" cy="6" r="2" /><circle cx="7" cy="18" r="2" /><circle cx="17" cy="9" r="2" /><path d="M7 8v8M7 13h6a3 3 0 0 0 3-3v-.5" /></g>,
  doc:      <g {...S}><path d="M7 3h7l4 4v14H7z" /><path d="M14 3v4h4M10 12h5M10 16h5" /></g>,
  sun:      <g {...S}><circle cx="12" cy="12" r="4" /><path d="M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1.5 1.5M17.5 17.5L19 19M19 5l-1.5 1.5M6.5 17.5L5 19" /></g>,
  moon:     <g {...S}><path d="M20 13A8 8 0 0 1 9 4a8 8 0 1 0 11 9z" /></g>,
  warning:  <g {...S}><path d="M12 4.5 21 19H3z" /><path d="M12 10v4" /><circle cx="12" cy="16.5" r=".3" fill="currentColor" /></g>,
  info:     <g {...S}><circle cx="12" cy="12" r="8" /><path d="M12 11v5" /><circle cx="12" cy="8" r=".4" fill="currentColor" /></g>,
  dot:      <g><circle cx="12" cy="12" r="3" fill="currentColor" /></g>,
  spinner:  <g {...S}><path d="M12 4a8 8 0 1 0 8 8" /></g>,
  arrowR:   <g {...S}><path d="M5 12h14M13 6l6 6-6 6" /></g>,
  layout:   <g {...S}><rect x="4" y="4" width="16" height="16" rx="2" /><path d="M13 4v16" /></g>,
  refresh:  <g {...S}><path d="M20 11a8 8 0 0 0-14-4l-2 2M4 13a8 8 0 0 0 14 4l2-2" /><path d="M4 5v4h4M20 19v-4h-4" /></g>,
  pin:      <g {...S}><path d="M9 4h6l-1 6 3 3v2H7v-2l3-3z" /><path d="M12 15v5" /></g>,
};

function Icon({ name, size = 16, className = "", style }) {
  const body = PATHS[name];
  if (!body) return null;
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} className={className}
         style={style} aria-hidden="true" focusable="false">{body}</svg>
  );
}

function StatusGlyph({ status, size = 14 }) {
  const body = STATUS_GLYPH[status] || STATUS_GLYPH.stopped;
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">{body}</svg>
  );
}

Object.assign(window, { Icon, StatusGlyph });
