/* DAEDALUS — primitives. StatusBadge (3 styles), chips, meters, helpers. */

// ── helpers ─────────────────────────────────────────────────────────────────
function fmtDur(sec) {
  if (sec == null) return "—";
  if (sec < 60) return sec + "s";
  const m = Math.floor(sec / 60), s = sec % 60;
  if (m < 60) return s ? `${m}m ${s}s` : `${m}m`;
  const h = Math.floor(m / 60);
  return `${h}h ${m % 60}m`;
}
function fmtAgo(sec) {
  if (sec == null) return "—";
  if (sec < 5) return "just now";
  if (sec < 60) return sec + "s ago";
  const m = Math.floor(sec / 60);
  if (m < 60) return m + "m ago";
  const h = Math.floor(m / 60);
  return h + "h ago";
}
const STATUS_LABEL = () => window.DATA.STATUS_LABEL;
const isLive = (s) => s === "running" || s === "starting";

// ── StatusBadge ───────────────────────────────────────────────────────────
// style ∈ "pill" | "dotlabel" | "iconled"  (a Tweak switches the whole app)
function StatusBadge({ status, style = "pill", label }) {
  const txt = label || window.DATA.STATUS_LABEL[status] || status;
  const live = status === "running";
  const spin = status === "starting";

  if (style === "dotlabel") {
    return (
      <span className={`stat stat--${status} dotlabel`}>
        <span className={`stat-dot${live ? " live" : ""}`} />
        <span>{txt}</span>
      </span>
    );
  }
  if (style === "iconled") {
    return (
      <span className={`stat stat--${status} iconled`}>
        <span className={`stat-glyph${spin ? " spin" : ""}`}><StatusGlyph status={status} /></span>
        <span>{txt}</span>
      </span>
    );
  }
  // pill (default)
  return (
    <span className={`stat stat--${status} pill`}>
      <span className={`stat-glyph${spin ? " spin" : ""}`} style={{ width: 12, height: 12 }}>
        <StatusGlyph status={status} size={12} />
      </span>
      <span>{txt}</span>
    </span>
  );
}

// ── Chips ────────────────────────────────────────────────────────────────────
function Chip({ icon, children, mono, className = "", dotColor }) {
  return (
    <span className={`chip ${mono ? "mono" : ""} ${className}`}>
      {dotColor && <span className="dotled" style={{ background: dotColor }} />}
      {icon && <Icon name={icon} size={12} />}
      {children}
    </span>
  );
}

const SOURCE_META = {
  local:  { icon: "dot", label: "Local" },
  mdns:   { icon: "globe", label: "mDNS" },
  tunnel: { icon: "tunnel", label: "Tunnel" },
};
function SourceChip({ source }) {
  const m = SOURCE_META[source] || SOURCE_META.local;
  return <Chip icon={m.icon}>{m.label}</Chip>;
}

const BACKEND_META = {
  workshop: { icon: "linux", label: "Workshop" },
  "workshop-remote": { icon: "tunnel", label: "Workshop · remote" },
  macos: { icon: "apple", label: "macOS" },
};
function BackendChip({ backend, name }) {
  const m = BACKEND_META[backend] || { icon: "backends", label: name || backend };
  return <Chip icon={m.icon}>{name || m.label}</Chip>;
}

// Originating GitHub / Jira issue (only rendered when present)
const TRACKER_META = {
  github: { label: "GitHub", color: "#7d7a8c" },
  jira: { label: "Jira", color: "#5f7aa3" },
};
function IssueChip({ issue, onClick }) {
  if (!issue) return null;
  const m = TRACKER_META[issue.tracker] || { label: issue.tracker, color: "var(--text-2)" };
  return (
    <a className="chip issue-chip mono" href="#" title={`${m.label} · ${issue.ref}`}
      onClick={(e) => { e.preventDefault(); e.stopPropagation(); onClick && onClick(); }}>
      <span className="issue-dot" style={{ background: m.color }} />
      <Icon name="tag" size={11} />
      {issue.ref}
    </a>
  );
}

// availability dot
function AvailDot({ state }) {
  const c = state === "available" ? "var(--st-running)" : state === "degraded" ? "var(--st-stalled)" : "var(--st-failed)";
  return <span className="stat-dot" style={{ background: c, width: 7, height: 7 }} />;
}

// ── Progress ─────────────────────────────────────────────────────────────────
function taskCounts(tasks) {
  const done = tasks.filter((t) => t.status === "done").length;
  return { done, total: tasks.length };
}
function ProgressPill({ tasks, showBar = true, status }) {
  const { done, total } = taskCounts(tasks);
  const pct = total ? Math.round((done / total) * 100) : 0;
  const cls = status === "failed" ? "bad" : status === "stalled" ? "warn" : status === "completed" ? "ok" : "";
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8, minWidth: 0 }}>
      <span style={{ fontSize: 11.5, fontWeight: 600, fontVariantNumeric: "tabular-nums",
        color: "var(--text-2)", whiteSpace: "nowrap" }}>{done}/{total}</span>
      {showBar && <div className={`meter ${cls}`} style={{ flex: 1, minWidth: 36 }}><i style={{ width: pct + "%" }} /></div>}
    </div>
  );
}

// ── Resource meter ─────────────────────────────────────────────────────────
function ResMeter({ icon, label, value, unit = "%", cls = "" }) {
  const v = value == null ? null : value;
  const cl = v == null ? "" : v > 85 ? "bad" : v > 65 ? "warn" : "";
  return (
    <div className="resmeter">
      <span className="lbl" style={{ display: "inline-flex", alignItems: "center", gap: 4, color: "var(--text-2)" }}>
        <Icon name={icon} size={12} />
      </span>
      <div className={`meter ${cl || cls}`}><i style={{ width: (v == null ? 0 : v) + "%" }} /></div>
      <span className="val">{v == null ? "—" : v + unit}</span>
    </div>
  );
}

// ── Section header (settings/discover groups) ───────────────────────────────
function GroupLabel({ children, right }) {
  return (
    <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between",
      padding: "0 4px 7px", fontSize: 11, fontWeight: 600, letterSpacing: ".04em",
      textTransform: "uppercase", color: "var(--text-3)" }}>
      <span>{children}</span>{right}
    </div>
  );
}

// ── Empty / loading / error state pattern ───────────────────────────────────
function EmptyState({ icon = "info", title, body, action, tone = "neutral" }) {
  const tint = tone === "error" ? "var(--st-failed)" : tone === "warn" ? "var(--st-stalled)" : "var(--text-3)";
  return (
    <div className="fadein" style={{ display: "flex", flexDirection: "column", alignItems: "center",
      justifyContent: "center", textAlign: "center", padding: "60px 24px", gap: 14, height: "100%" }}>
      <div style={{ width: 56, height: 56, borderRadius: 16, display: "grid", placeItems: "center",
        background: "var(--fill-1)", color: tint }}>
        <Icon name={icon} size={28} />
      </div>
      <div style={{ maxWidth: 380 }}>
        <div style={{ fontSize: 15, fontWeight: 600, marginBottom: 5 }}>{title}</div>
        <div style={{ fontSize: 12.5, color: "var(--text-2)", lineHeight: 1.5 }}>{body}</div>
      </div>
      {action}
    </div>
  );
}

Object.assign(window, {
  fmtDur, fmtAgo, isLive, taskCounts,
  StatusBadge, Chip, SourceChip, BackendChip, AvailDot, IssueChip,
  ProgressPill, ResMeter, GroupLabel, EmptyState,
});
