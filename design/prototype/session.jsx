/* DAEDALUS — Session detail. Terminal + task board + telemetry rail. */

const TLINE_CLASS = {
  "": "tl", dim: "tl tl-dim", ok: "tl tl-ok", warn: "tl tl-warn",
  err: "tl tl-err", accent: "tl tl-accent", prompt: "tl tl-prompt", cmd: "tl tl-cmd",
};

// Extra zellij tabs (the agent created these inside its own session).
// Daedalus doesn't manage these — zellij does; we just render what it draws.
const SHELL_LINES = [
  ["dim", "operator@workshop:~/acme/monorepo$ git status"],
  ["", "On branch agent/044-device-flow"],
  ["", "Changes not staged for commit:"],
  ["ok", "  modified:   gateway/middleware/device_flow.go"],
  ["ok", "  modified:   store/redis/device_codes.go"],
  ["", ""],
  ["dim", "operator@workshop:~/acme/monorepo$ "],
  ["__cursor__", ""],
];
const LOG_LINES = [
  ["dim", "$ tail -f .daedalus/run.log"],
  ["", "12:04:18  T004 wire gateway middleware — done"],
  ["", "12:04:51  T005 persist device codes — started"],
  ["warn", "12:06:02  redis TTL assertion off by 1s — retry"],
  ["", "12:06:09  re-running store/redis tests"],
  ["dim", "…"],
  ["__cursor__", ""],
];

function Terminal({ session, interactive, ended }) {
  const baseLines = window.DATA.pickTerminal(session.id);
  const tabs = [
    { name: "agent", lines: baseLines },
    { name: "shell", lines: SHELL_LINES },
    { name: "logs", lines: LOG_LINES },
  ];
  const scrollRef = React.useRef(null);
  const [atBottom, setAtBottom] = React.useState(true);
  const [input, setInput] = React.useState("");
  const [tab, setTab] = React.useState(0);

  React.useEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
    setAtBottom(true);
  }, [session.id, tab]);

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    setAtBottom(el.scrollHeight - el.scrollTop - el.clientHeight < 30);
  };
  const jump = () => {
    const el = scrollRef.current;
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  };
  const lines = tabs[tab].lines;

  return (
    <section className="term">
      <div className="term-bar">
        <div className="term-bar-l">
          <span className="term-title"><Icon name="bolt" size={12} /> Terminal</span>
          {!ended && <span className="term-live"><span className="stat-dot live" style={{ background: "var(--st-running)" }} /> live</span>}
          {ended && <span className="chip">persisted output</span>}
        </div>
        <div className="term-bar-r">
          <button className="iconbtn" title={"Live zellij session. Tabs and panes are managed by zellij itself — switch tabs with Alt+←/→ (or click below), split panes with Ctrl+p. Daedalus just streams the view and passes your keystrokes through."}><Icon name="info" size={14} /></button>
          <button className="iconbtn" title="Search output (⌘F)"><Icon name="search" size={14} /></button>
          <button className="iconbtn" title="Copy all"><Icon name="copy" size={14} /></button>
          <button className="iconbtn" title="Jump to latest (⌘↓)" onClick={jump}><Icon name="jumpdown" size={14} /></button>
        </div>
      </div>
      <div className="term-body" ref={scrollRef} onScroll={onScroll}>
        <pre>
          {lines.map((ln, i) => {
            if (ln[0] === "__cursor__") return <div key={i} className="tl"><span className="term-cursor" /></div>;
            return <div key={i} className={TLINE_CLASS[ln[0]] || "tl"}>{ln[1] || "\u00A0"}</div>;
          })}
        </pre>
      </div>
      {!atBottom && <button className="term-jump" onClick={jump}><Icon name="jumpdown" size={13} /> Jump to latest</button>}
      {/* zellij's own tab bar, drawn inside the terminal — switching is a zellij action */}
      <div className="zbar" title="zellij tab bar — these tabs live inside the session, not in Daedalus">
        <span className="zlogo">zellij</span>
        {tabs.map((tb, i) => (
          <button key={tb.name} className={`ztab${i === tab ? " active" : ""}`} onClick={() => setTab(i)}>
            <span className="znum">{i + 1}</span> {tb.name}
          </button>
        ))}
        <span className="zspacer" />
        <span className="zhint">Alt&nbsp;←/→ tabs · Ctrl&nbsp;p pane</span>
      </div>
      <div className="term-input-wrap">
        {interactive && !ended ? (
          <div className="term-input">
            <span className="term-prompt-glyph">›</span>
            <input value={input} onChange={(e) => setInput(e.target.value)} placeholder="Send input to the focused pane…  (↵ to send)"
              onKeyDown={(e) => { if (e.key === "Enter") setInput(""); }} />
            <button className="btn sm tinted" disabled={!input.trim()}><Icon name="send" size={12} /> Send</button>
          </div>
        ) : (
          <div className="term-input disabled">
            <Icon name="info" size={13} />
            <span>{ended ? "Session ended — output is read-only (review mode)." : `${session.toolName} does not accept interactive input.`}</span>
          </div>
        )}
      </div>
    </section>
  );
}

// ── Task board ───────────────────────────────────────────────────────────────
const COLUMNS = [
  { key: "todo", label: "To do", match: ["todo"] },
  { key: "doing", label: "In progress", match: ["doing"] },
  { key: "blocked", label: "Blocked", match: ["blocked"] },
  { key: "done", label: "Done", match: ["done"] },
];
const TASK_DOT = { todo: "var(--text-3)", doing: "var(--st-starting)", blocked: "var(--st-stalled)", done: "var(--st-running)" };

function TaskCard({ t }) {
  return (
    <div className={`task-card tc-${t.status}`}>
      <div className="tc-head">
        <span className="tc-id">{t.id}</span>
        <span className="tc-dot" style={{ background: TASK_DOT[t.status] }} />
      </div>
      <div className="tc-title">{t.title}</div>
      {t.detail && <div className="tc-detail"><Icon name={t.status === "blocked" ? "warning" : "info"} size={11} /> {t.detail}</div>}
    </div>
  );
}

function TaskBoard({ session, compact, ended }) {
  const { done, total } = taskCounts(session.tasks);
  const pct = total ? Math.round((done / total) * 100) : 0;
  const [collapsed, setCollapsed] = React.useState(false);
  const cols = COLUMNS.map((c) => ({ ...c, items: session.tasks.filter((t) => c.match.includes(t.status)) }))
    .filter((c) => c.items.length > 0 || c.key === "todo" || c.key === "doing" || c.key === "done");
  return (
    <section className="board">
      <div className="board-head">
        <div className="board-title">
          <span>Tasks</span>
          <span className="board-src">tracked from <code>tasks.md</code> + output</span>
        </div>
        <div className="board-head-r">
          <div className="board-prog">
            <span className="board-prog-num">{done}<span style={{ color: "var(--text-3)" }}>/{total}</span></span>
            <div className={`meter ${session.status === "failed" ? "bad" : session.status === "completed" ? "ok" : ""}`} style={{ width: 72 }}>
              <i style={{ width: pct + "%" }} />
            </div>
          </div>
          <button className="board-toggle" onClick={() => setCollapsed((c) => !c)}
            title={collapsed ? "Expand to board" : "Collapse to list"} aria-label={collapsed ? "Expand to board" : "Collapse to list"}>
            <Icon name={collapsed ? "board" : "list"} size={15} />
          </button>
        </div>
      </div>
      {collapsed ? (
        <BoardChecklist session={session} />
      ) : (
        <div className={`board-cols${compact ? " compact" : ""}`}>
          {cols.map((c) => (
            <div key={c.key} className={`board-col col-${c.key}`}>
              <div className="bc-head">
                <span className="bc-dot" style={{ background: TASK_DOT[c.key] }} />
                <span className="bc-label">{c.label}</span>
                <span className="bc-count">{c.items.length}</span>
              </div>
              <div className="bc-items">
                {c.items.map((t) => <TaskCard key={t.id} t={t} />)}
                {c.items.length === 0 && <div className="bc-empty">—</div>}
              </div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

// Compact, sequential checklist (collapsed Tasks view) — mirrors tasks.md order.
const CHECK_LABEL = { todo: "To do", doing: "In progress", blocked: "Blocked", done: "Done" };
function BoardChecklist({ session }) {
  return (
    <div className="board-list">
      {session.tasks.map((t) => (
        <div key={t.id} className={`bl-row bl-${t.status}`} title={CHECK_LABEL[t.status] + (t.detail ? " — " + t.detail : "")}>
          <span className="bl-glyph">
            {t.status === "done" ? <Icon name="check" size={13} />
              : t.status === "blocked" ? <Icon name="warning" size={13} />
              : t.status === "doing" ? <span className="bl-doing" />
              : <span className="bl-todo" />}
          </span>
          <span className="bl-id">{t.id}</span>
          <span className="bl-title">{t.title}</span>
          {t.status === "doing" && <span className="bl-tag">In progress</span>}
          {t.status === "blocked" && <span className="bl-tag warn">Blocked</span>}
        </div>
      ))}
    </div>
  );
}

// ── Collapsible resources/telemetry footer (sits under the Tasks board) ──────
function SessionMetrics({ session, ended }) {
  const [open, setOpen] = React.useState(false);
  const unknown = session.status === "unknown";
  const events = [
    { t: ended ? "now" : "now", label: ended ? (session.exit || "Session ended") : "Persisting device codes (T005)", tone: ended ? "dim" : "accent" },
    { t: fmtAgo(session.lastEvent), label: session.status === "stalled" ? "No output — possible stall" : "T004 completed", tone: session.status === "stalled" ? "warn" : "ok" },
    { t: fmtDur(session.started) + " ago", label: "Session started", tone: "dim" },
  ];
  return (
    <div className={`smetrics${open ? " open" : ""}`}>
      <button className="sm-bar" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
        <Icon name="cpu" size={13} />
        <span className="sm-title">Resources</span>
        {unknown ? (
          <span className="sm-sum dim">metrics unavailable</span>
        ) : (
          <span className="sm-sum">
            <span><b>{session.cpu}%</b> cpu</span>
            <span><b>{session.mem}%</b> mem</span>
            <span><b>{session.disk}%</b> disk</span>
            <span className="sm-rt"><Icon name="clock" size={11} /> {fmtDur(session.started)}</span>
          </span>
        )}
        <span className="sm-spacer" />
        <Icon name="chevD" size={14} className="sm-chev" style={{ transform: open ? "rotate(180deg)" : "none", transition: "transform .15s var(--ease)" }} />
      </button>
      {open && (
        <div className="sm-body">
          {unknown ? (
            <div className="rail-unknown"><Icon name="warning" size={13} /> Live metrics unavailable — connection lost. Showing last-known state only.</div>
          ) : (
            <>
              <div className="sm-meters">
                <ResLabeled icon="cpu" label="CPU" value={session.cpu} />
                <ResLabeled icon="mem" label="Memory" value={session.mem} />
                <ResLabeled icon="disk" label="Disk" value={session.disk} />
              </div>
              <div className="sm-events">
                {events.map((e, i) => (
                  <div key={i} className={`sm-ev sm-ev-${e.tone}`}>
                    <span className="sm-evdot" />
                    <span className="sm-evlabel">{e.label}</span>
                    <span className="sm-evt">{e.t}</span>
                  </div>
                ))}
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}

// ── Telemetry rail ───────────────────────────────────────────────────────────
function Sparkline({ seed = 1, color = "var(--accent)" }) {
  const pts = React.useMemo(() => {
    let v = 40 + (seed % 5) * 6;
    return Array.from({ length: 28 }, (_, i) => {
      v += (Math.sin(i * 0.7 + seed) * 9) + (((i * seed) % 7) - 3);
      v = Math.max(8, Math.min(92, v));
      return v;
    });
  }, [seed]);
  const w = 100, h = 34;
  const d = pts.map((p, i) => `${(i / (pts.length - 1)) * w},${h - (p / 100) * h}`).join(" ");
  return (
    <svg viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" style={{ width: "100%", height: 34 }}>
      <polyline points={d} fill="none" stroke={color} strokeWidth="1.5" vectorEffect="non-scaling-stroke" />
      <polyline points={`0,${h} ${d} ${w},${h}`} fill={color} opacity="0.08" stroke="none" />
    </svg>
  );
}

function TelemetryRail({ session, ended }) {
  const unknown = session.status === "unknown";
  const events = [
    { t: "now", label: ended ? "Session ended" : "Persisting device codes (T005)", tone: ended ? "dim" : "accent" },
    { t: fmtAgo(session.lastEvent), label: session.status === "stalled" ? "No output — possible stall" : "T004 completed", tone: session.status === "stalled" ? "warn" : "ok" },
    { t: "8m ago", label: "Gateway middleware wired", tone: "" },
    { t: "12m ago", label: "Environment provisioned", tone: "" },
    { t: fmtDur(session.started) + " ago", label: "Session started", tone: "dim" },
  ];
  return (
    <aside className="rail">
      <div className="rail-sect">
        <GroupLabel>Resources</GroupLabel>
        {unknown ? (
          <div className="rail-unknown"><Icon name="warning" size={13} /> Live metrics unavailable — connection lost.</div>
        ) : (
          <div className="rail-res card" style={{ padding: "12px 13px", display: "grid", gap: 11 }}>
            <ResLabeled icon="cpu" label="CPU" value={session.cpu} seed={3} />
            <ResLabeled icon="mem" label="Memory" value={session.mem} seed={5} />
            <ResLabeled icon="disk" label="Disk" value={session.disk} unit="%" seed={2} />
            <div className="rail-time"><Icon name="clock" size={12} /> Runtime <b>{fmtDur(session.started)}</b></div>
          </div>
        )}
      </div>
      <div className="rail-sect">
        <GroupLabel>Timeline</GroupLabel>
        <div className="timeline">
          {events.map((e, i) => (
            <div key={i} className={`tlx tlx-${e.tone}`}>
              <span className="tlx-node" />
              <div className="tlx-body"><span className="tlx-label">{e.label}</span><span className="tlx-time">{e.t}</span></div>
            </div>
          ))}
        </div>
      </div>
    </aside>
  );
}
function ResLabeled({ icon, label, value, unit = "%", seed }) {
  const v = value;
  const cl = v == null ? "" : v > 85 ? "bad" : v > 65 ? "warn" : "";
  return (
    <div>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 4 }}>
        <span style={{ display: "inline-flex", alignItems: "center", gap: 5, fontSize: 11.5, color: "var(--text-2)", fontWeight: 500 }}>
          <Icon name={icon} size={12} /> {label}
        </span>
        <span style={{ fontFamily: "var(--font-mono)", fontSize: 11, color: "var(--text-1)", fontVariantNumeric: "tabular-nums" }}>
          {v == null ? "—" : v + unit}
        </span>
      </div>
      <div className={`meter ${cl}`}><i style={{ width: (v ?? 0) + "%" }} /></div>
    </div>
  );
}

// ── State banner ────────────────────────────────────────────────────────────
function StateBanner({ session }) {
  const s = session.status;
  if (s === "stalled") return (
    <div className="banner banner-warn">
      <Icon name="warning" size={16} />
      <div><b>No progress for {fmtAgo(session.lastEvent).replace(" ago", "")}.</b> The agent may be stuck waiting on a fixture or input. Consider sending input or stopping the session.</div>
      <div className="banner-acts"><button className="btn sm"><Icon name="send" size={12} /> Send input</button><button className="btn sm danger"><Icon name="stop" size={12} /> Stop</button></div>
    </div>
  );
  if (s === "failed") return (
    <div className="banner banner-err">
      <Icon name="warning" size={16} />
      <div><b>Session failed.</b> {session.exit}</div>
      <div className="banner-acts"><button className="btn sm"><Icon name="refresh" size={12} /> Start similar</button><button className="btn sm"><Icon name="cleanup" size={12} /> Clean up</button></div>
    </div>
  );
  if (s === "unknown") return (
    <div className="banner banner-unknown">
      <Icon name="warning" size={16} />
      <div><b>Connection lost.</b> {session.note} Showing last-known state — this is <i>not</i> a healthy session.</div>
      <div className="banner-acts"><button className="btn sm"><Icon name="refresh" size={12} /> Retry connection</button></div>
    </div>
  );
  if (s === "completed") return (
    <div className="banner banner-ok">
      <Icon name="check" size={16} />
      <div><b>Completed.</b> {session.exit} You're viewing persisted output in review mode.</div>
      <div className="banner-acts"><button className="btn sm"><Icon name="cleanup" size={12} /> Clean up env</button></div>
    </div>
  );
  if (s === "stopped") return (
    <div className="banner banner-neutral">
      <Icon name="stop" size={16} />
      <div><b>Stopped by operator.</b> Output persisted — review mode. The environment is still allocated.</div>
      <div className="banner-acts"><button className="btn sm"><Icon name="cleanup" size={12} /> Clean up env</button></div>
    </div>
  );
  if (s === "starting") return (
    <div className="banner banner-info">
      <span className="stat-glyph spin" style={{ width: 16, height: 16, color: "var(--st-starting)" }}><StatusGlyph status="starting" size={16} /></span>
      <div><b>Provisioning environment…</b> A fresh sandbox is being created on {session.backendName}. The agent will start automatically.</div>
    </div>
  );
  return null;
}

// ── Session detail shell ─────────────────────────────────────────────────────
function SessionDetail({ session, split, statusStyle, onBack }) {
  const ended = ["completed", "failed", "stopped"].includes(session.status);
  const interactive = (window.DATA.TOOLS.find((t) => t.id === session.tool) || {}).interactive;
  const ratio = { terminal: "1fr 2.5fr", split: "1fr 1.6fr", board: "1.35fr 1fr" }[split] || "1fr 1.6fr";

  return (
    <div className="screen session-screen">
      <header className="sess-head">
        <button className="iconbtn" onClick={onBack} title="Back to sessions (⌘[)"><Icon name="chevL" size={18} /></button>
        <div className="sess-id">
          <div className="sess-top">
            <span className="sess-tool">{session.toolName}</span>
            <StatusBadge status={session.status} style={statusStyle} />
            {session.status === "unknown" && <span className="chip warnsoft">last-known: {window.DATA.STATUS_LABEL[session.lastKnown]}</span>}
          </div>
          <h1 className="sess-obj">{session.objective}</h1>
          <div className="sess-chips">
            <Chip icon="doc" mono>{session.spec}</Chip>
            <IssueChip issue={session.issue} />
            <SourceChip source={session.source} />
            <BackendChip backend={session.backend} name={session.backendName} />
            <Chip icon="git">{session.envKind === "fresh" ? "fresh env" : session.envName}</Chip>
            {session.envKind === "existing" && <Chip icon="shield" className="okSoft">worktree-isolated</Chip>}
          </div>
        </div>
        <div className="sess-controls">
          <button className="btn" disabled={!interactive || ended} title={interactive ? "Send input (i)" : "This tool does not accept input"}>
            <Icon name="send" size={14} /> Send input
          </button>
          <button className="btn cleanup-btn" disabled={isLive(session.status)} title="Release the environment">
            <Icon name="cleanup" size={14} /> Clean up
          </button>
          {!ended ? (
            <button className="btn danger"><Icon name="stop" size={14} /> Stop</button>
          ) : (
            <button className="btn primary"><Icon name="refresh" size={14} /> Start similar</button>
          )}
        </div>
      </header>

      <StateBanner session={session} />

      <div className="sess-body">
        <div className="sess-main" style={{ gridTemplateColumns: ratio }}>
          <TaskBoard session={session} compact={split === "terminal"} ended={ended} />
          <Terminal session={session} interactive={interactive} ended={ended} />
        </div>
      </div>
    </div>
  );
}

Object.assign(window, { SessionDetail });
