/* DAEDALUS — "Needs you": unified queue of sessions blocked on the operator.
   One row component, rendered in three placements (a Tweak switches which):
     • view  — a dedicated full screen ("Needs you" nav item)
     • tray  — a header popover, triageable from anywhere
     • strip — a pinned band at the top of the Tasks home
   Answering = jump straight into that session's terminal, focused on the prompt. */

// Big "time waiting" readout — minutes/hours, calm but legible.
function waitLabel(sec) {
  if (sec == null) return "—";
  if (sec < 60) return sec + "s";
  const m = Math.floor(sec / 60);
  if (m < 60) return m + "m";
  const h = Math.floor(m / 60);
  return h + "h " + (m % 60) + "m";
}

// One queue entry. `tone` ∈ "card" | "line".
function NeedsRow({ item, onOpen, tone = "card" }) {
  const go = (e) => { if (e) e.stopPropagation(); onOpen(item.id, { answer: true }); };
  const isQ = item.reasonKind === "awaiting";
  const cost = item.live
    ? <span className="ny-cost" title="Sandbox time billed while this session sits blocked">idle · ${item.costUsd.toFixed(2)}</span>
    : <span className="ny-cost held" title="Session ended — its environment is still allocated">env held</span>;

  if (tone === "line") {
    return (
      <button className={`ny-line ny--${item.reasonKind}`} onClick={go}>
        <span className="ny-glyph stat-glyph"><StatusGlyph status={item.reasonKind} size={15} /></span>
        <span className="ny-line-main">
          <span className="ny-line-obj">{item.objective}</span>
          <span className={`ny-line-reason${isQ ? " q" : ""}`}>{item.reason}</span>
        </span>
        <span className="ny-line-wait"><Icon name="clock" size={11} /> {waitLabel(item.waitingSec)}</span>
        <Icon name="arrowR" size={13} className="ny-line-go" />
      </button>
    );
  }

  return (
    <article className={`ny-row ny--${item.reasonKind}`} onClick={go} tabIndex={0}
      onKeyDown={(e) => { if (e.key === "Enter") go(); }}>
      <span className="ny-rail" />
      <span className="ny-glyph stat-glyph"><StatusGlyph status={item.reasonKind} size={18} /></span>
      <div className="ny-main">
        <div className="ny-top">
          <span className="ny-cue">{item.cue}</span>
          <span className="ny-dot">·</span>
          <span className="ny-tool">{item.toolName}</span>
          <BackendChip backend={item.backend} name={item.backendName} />
          <IssueChip issue={item.issue} />
        </div>
        <div className="ny-obj">{item.objective}</div>
        <div className={`ny-reason${isQ ? " q" : ""}`}>
          {isQ && <span className="ny-q">“</span>}
          {item.reason}
          {isQ && <span className="ny-q">”</span>}
        </div>
      </div>
      <div className="ny-right">
        <div className="ny-wait" title="Waiting on you for this long"><Icon name="clock" size={12} /> {waitLabel(item.waitingSec)}</div>
        {cost}
        <button className="btn sm primary ny-go" onClick={go}>
          {item.reasonKind === "confirm"
            ? <><Icon name="check" size={12} /> Review &amp; confirm</>
            : <><Icon name="send" size={12} /> Answer in terminal</>}
        </button>
      </div>
    </article>
  );
}

function NeedsEmpty({ compact }) {
  return (
    <div className={`ny-empty${compact ? " compact" : ""}`}>
      <span className="ny-empty-ic stat--running stat-glyph"><StatusGlyph status="completed" size={compact ? 20 : 26} /></span>
      <div>
        <div className="ny-empty-t">You're all caught up</div>
        <div className="ny-empty-s">No agents are waiting on you. Daedalus will surface anything that asks for input or stalls.</div>
      </div>
    </div>
  );
}

// ── Variation A — dedicated full screen ──────────────────────────────────────
function NeedsView({ onOpen }) {
  const items = window.DATA.needsYou();
  const by = (k) => items.filter((i) => i.reasonKind === k).length;
  const parts = [
    { k: "awaiting", tone: "await", label: "awaiting answer" },
    { k: "confirm", tone: "confirm", label: "to confirm" },
    { k: "stalled", tone: "stalled", label: "stalled" },
    { k: "failed", tone: "failed", label: "failed" },
    { k: "unknown", tone: "unknown", label: "disconnected" },
  ].map((p) => ({ ...p, n: by(p.k) })).filter((p) => p.n > 0);
  return (
    <div className="screen">
      <header className="screen-head">
        <div className="sh-title">
          <h1>Needs you</h1>
          <div className="sh-sub">Sessions blocked on you — questions to answer, stalls and failures to clear. Open one to drop straight into its terminal.</div>
          <div className="sh-stats">
            {parts.map((p) => (
              <span key={p.k} className={`stat--${p.tone}`}><span className="stat-dot" style={{ background: "var(--c)" }} /> {p.n} {p.label}</span>
            ))}
          </div>
        </div>
      </header>
      <div className="screen-scroll">
        {items.length === 0 ? <NeedsEmpty /> : (
          <div className="ny-list">
            {items.map((it) => <NeedsRow key={it.id} item={it} onOpen={onOpen} />)}
          </div>
        )}
      </div>
    </div>
  );
}

// ── Variation B — header popover / tray ──────────────────────────────────────
function NeedsTray({ open, onClose, onOpen }) {
  if (!open) return null;
  const items = window.DATA.needsYou();
  const aw = items.filter((i) => i.reasonKind === "awaiting").length;
  return (
    <>
      <div className="pop-scrim" onMouseDown={onClose} />
      <div className="ny-tray fadeup">
        <div className="ny-tray-head">
          <b>Needs you</b>
          <span className="ny-tray-sub">{aw} awaiting · {items.length} total</span>
        </div>
        <div className="ny-tray-list">
          {items.length === 0 ? <NeedsEmpty compact /> : items.map((it) => (
            <NeedsRow key={it.id} item={it} tone="line" onOpen={(id, o) => { onClose(); onOpen(id, o); }} />
          ))}
        </div>
      </div>
    </>
  );
}

// ── Variation C — pinned strip on the Tasks home ─────────────────────────────
function NeedsStrip({ onOpen }) {
  const items = window.DATA.needsYou();
  const [collapsed, setCollapsed] = React.useState(false);
  if (items.length === 0) return null;
  const aw = items.filter((i) => i.reasonKind === "awaiting").length;
  const cf = items.filter((i) => i.reasonKind === "confirm").length;
  const rest = items.length - aw - cf;
  const sub = [aw > 0 && `${aw} asked a question`, cf > 0 && `${cf} to confirm`, rest > 0 && `${rest} stalled or failed`]
    .filter(Boolean).join(" · ");
  return (
    <section className={`ny-strip${collapsed ? " collapsed" : ""}`}>
      <div className="ny-strip-head" onClick={() => setCollapsed((c) => !c)}>
        <span className="ny-strip-pulse stat--await"><span className="stat-dot live" style={{ background: "var(--c)" }} /></span>
        <b>{items.length} {items.length === 1 ? "session needs" : "sessions need"} you</b>
        <span className="ny-strip-sub">{sub}</span>
        <span className="ny-strip-spacer" />
        <button className="btn sm ghost" onClick={(e) => { e.stopPropagation(); setCollapsed((c) => !c); }}>
          <Icon name={collapsed ? "chevD" : "chevD"} size={13} style={{ transform: collapsed ? "rotate(-90deg)" : "none", transition: "transform .18s" }} />
          {collapsed ? "Show" : "Hide"}
        </button>
      </div>
      {!collapsed && (
        <div className="ny-strip-rail">
          {items.map((it) => (
            <button key={it.id} className={`ny-chip ny--${it.reasonKind}`} onClick={() => onOpen(it.id, { answer: true })}>
              <span className="ny-glyph stat-glyph"><StatusGlyph status={it.reasonKind} size={14} /></span>
              <span className="ny-chip-body">
                <span className="ny-chip-obj">{it.objective}</span>
                <span className={`ny-chip-reason${it.reasonKind === "awaiting" ? " q" : ""}`}>{it.reason}</span>
              </span>
              <span className="ny-chip-wait">{waitLabel(it.waitingSec)}</span>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}

Object.assign(window, { NeedsView, NeedsTray, NeedsStrip, NeedsRow });
