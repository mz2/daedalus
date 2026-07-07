/* DAEDALUS — Tasks (global). Every tracked task across the fleet.
   Session is one field among tool / backend / status — filterable + a column. */

const TASK_COLS = [
  { key: "doing", label: "In progress", dot: "var(--st-starting)" },
  { key: "blocked", label: "Blocked", dot: "var(--st-stalled)" },
  { key: "todo", label: "To do", dot: "var(--text-3)" },
  { key: "done", label: "Done", dot: "var(--st-running)" },
];
const TDOT = { todo: "var(--text-3)", doing: "var(--st-starting)", blocked: "var(--st-stalled)", done: "var(--st-running)" };

function TaskStatusBadge({ status, style }) {
  const label = window.DATA.TASK_STATUS_LABEL[status];
  if (style === "iconled") {
    return <span className="tsb" style={{ color: TDOT[status] }}><span className="tsb-dot" style={{ background: TDOT[status] }} />{label}</span>;
  }
  return (
    <span className="tsb-pill" data-st={status}>
      <span className="tsb-dot" style={{ background: TDOT[status] }} />{label}
    </span>
  );
}

// session reference line — the "field" the operator filters/jumps by
function SessionRef({ t, onOpen }) {
  const att = ["stalled", "failed", "unknown", "awaiting", "confirm"].includes(t.sessionStatus);
  return (
    <button className={`sess-ref${att ? " att" : ""}`} onClick={(e) => { e.stopPropagation(); onOpen(t.sessionId); }}
      title={"Open session · " + window.DATA.STATUS_LABEL[t.sessionStatus]}>
      <span className="sr-dot stat--{}" style={{ background: `var(--st-${t.sessionStatus})` }} />
      <span className="sr-obj">{t.objective}</span>
      <Icon name="arrowR" size={11} className="sr-go" />
    </button>
  );
}

function GlobalTaskCard({ t, onOpen }) {
  return (
    <div className={`gtask${t.attention ? " att" : ""} gt-${t.status}`} onClick={() => onOpen(t.sessionId)} tabIndex={0}
      onKeyDown={(e) => { if (e.key === "Enter") onOpen(t.sessionId); }}>
      <div className="gt-head">
        <span className="gt-id">{t.id}</span>
        <span className="gt-title">{t.title}</span>
        {t.attention && <span className="gt-flag"><Icon name="warning" size={11} /></span>}
      </div>
      {t.detail && <div className="gt-detail"><Icon name={t.status === "blocked" ? "warning" : "info"} size={11} /> {t.detail}</div>}
      <div className="gt-foot">
        <SessionRef t={t} onOpen={onOpen} />
        <div className="gt-meta"><Icon name="tools" size={11} /> {t.toolName}</div>
      </div>
    </div>
  );
}

function TaskBoardGlobal({ tasks, onOpen }) {
  return (
    <div className="gboard">
      {TASK_COLS.map((c) => {
        const items = tasks.filter((t) => t.status === c.key);
        return (
          <div key={c.key} className="gcol">
            <div className="gcol-head">
              <span className="gcol-dot" style={{ background: c.dot }} />
              <span className="gcol-label">{c.label}</span>
              <span className="gcol-count">{items.length}</span>
            </div>
            <div className="gcol-items">
              {items.map((t, i) => (
                <div key={t.key} className="fadeup" style={{ animationDelay: Math.min(i * 22, 180) + "ms" }}>
                  <GlobalTaskCard t={t} onOpen={onOpen} />
                </div>
              ))}
              {items.length === 0 && <div className="gcol-empty">No tasks</div>}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function TaskTableGlobal({ tasks, onOpen, statusStyle }) {
  return (
    <div className="gtable card">
      <div className="gt-headrow">
        <span style={{ width: 116 }}>Status</span>
        <span style={{ width: 54 }}>Task</span>
        <span style={{ flex: 1 }}>Description</span>
        <span style={{ width: 230 }}>Session</span>
        <span style={{ width: 116 }}>Tool</span>
        <span style={{ width: 132 }}>Backend</span>
      </div>
      {tasks.map((t) => (
        <div key={t.key} className={`gt-row${t.attention ? " att" : ""}`} onClick={() => onOpen(t.sessionId)} tabIndex={0}
          onKeyDown={(e) => { if (e.key === "Enter") onOpen(t.sessionId); }}>
          <span style={{ width: 116 }}><TaskStatusBadge status={t.status} /></span>
          <span style={{ width: 54 }} className="gtr-id">{t.id}</span>
          <span style={{ flex: 1, minWidth: 0 }} className="gtr-title">
            {t.title}
            {t.detail && <span className="gtr-detail"> — {t.detail}</span>}
          </span>
          <span style={{ width: 230, minWidth: 0 }}>
            <button className="gtr-sess" onClick={(e) => { e.stopPropagation(); onOpen(t.sessionId); }}>
              <span className="sr-dot" style={{ background: `var(--st-${t.sessionStatus})` }} />
              <span className="sr-obj">{t.objective}</span>
            </button>
          </span>
          <span style={{ width: 116 }} className="gtr-dim">{t.toolName}</span>
          <span style={{ width: 132 }} className="gtr-dim">{t.backendName}</span>
        </div>
      ))}
    </div>
  );
}

// Grouped multiline list — between the board and the dense table.
function TaskListGrouped({ tasks, onOpen, statusStyle }) {
  return (
    <div className="tlist">
      {TASK_COLS.map((c) => {
        const items = tasks.filter((t) => t.status === c.key);
        if (items.length === 0) return null;
        return (
          <div key={c.key} className="tlist-group">
            <div className="tlist-glabel">
              <span className="gcol-dot" style={{ background: c.dot }} />
              {c.label}<span className="tlist-gn">{items.length}</span>
            </div>
            <div className="grouped">
              {items.map((t) => {
                const needs = (t.status === "doing" || t.status === "blocked") && needsMeta(t.sessionStatus) ? t.sessionStatus : null;
                return (
                <div key={t.key} className={`row tlist-row${needs ? " needs ny--" + needs : ""}`} onClick={() => onOpen(t.sessionId)} tabIndex={0}
                  onKeyDown={(e) => { if (e.key === "Enter") onOpen(t.sessionId); }}>
                  <span className="tlr-glyph"><TaskStatusBadge status={t.status} style={statusStyle} /></span>
                  <span className="tlr-id">{t.id}</span>
                  <div className="tlr-main">
                    <div className="tlr-title">{t.title}{t.detail && <span className="tlr-detail"> — {t.detail}</span>}{needs && <NeedsTag status={needs} />}</div>
                    <div className="tlr-sub">
                      <span className="row-branch"><Icon name="git" size={11} /> {t.spec}</span>
                      <span className="row-meta"><Icon name="tools" size={11} /> {t.toolName}</span>
                      <span className="row-meta"><Icon name="backends" size={11} /> {t.backendName}</span>
                    </div>
                  </div>
                  <SessionRef t={t} onOpen={onOpen} />
                  <Icon name="chevR" size={14} className="tlr-chev" />
                </div>
                );
              })}
            </div>
          </div>
        );
      })}
    </div>
  );
}

const TFILTERS = [
  { k: "doing", label: "In progress" }, { k: "blocked", label: "Blocked" },
  { k: "todo", label: "To do" }, { k: "done", label: "Done" },
];

function TasksView({ layout, statusStyle, onOpen, onStart, needsStrip }) {
  const all = window.DATA.allTasks();
  const [q, setQ] = React.useState("");
  const [active, setActive] = React.useState([]);     // task-status filters
  const [sessionF, setSessionF] = React.useState("all");
  const [toolF, setToolF] = React.useState("all");
  const [backendF, setBackendF] = React.useState("all");
  const [attentionOnly, setAttentionOnly] = React.useState(false);

  const toggle = (k) => setActive((a) => a.includes(k) ? a.filter((x) => x !== k) : [...a, k]);

  let tasks = all.filter((t) => {
    if (q && !(`${t.title} ${t.id} ${t.objective}`.toLowerCase().includes(q.toLowerCase()))) return false;
    if (active.length && !active.includes(t.status)) return false;
    if (sessionF !== "all" && t.sessionId !== sessionF) return false;
    if (toolF !== "all" && t.tool !== toolF) return false;
    if (backendF !== "all" && t.backend !== backendF) return false;
    if (attentionOnly && !t.attention) return false;
    return true;
  });

  const counts = {
    doing: all.filter((t) => t.status === "doing").length,
    blocked: all.filter((t) => t.status === "blocked").length,
    todo: all.filter((t) => t.status === "todo").length,
    done: all.filter((t) => t.status === "done").length,
    attention: all.filter((t) => t.attention).length,
  };
  const sessions = window.DATA.SESSIONS;
  const filtersOn = q || active.length || sessionF !== "all" || toolF !== "all" || backendF !== "all" || attentionOnly;

  return (
    <div className="screen">
      <header className="screen-head">
        <div className="sh-title">
          <h1>Tasks</h1>
          <div className="sh-stats">
            <span className="stat--starting"><span className="stat-dot live" style={{ background: "var(--c)" }} /> {counts.doing} in progress</span>
            {counts.blocked > 0 && <span className="stat--stalled"><span className="stat-dot" style={{ background: "var(--c)" }} /> {counts.blocked} blocked</span>}
            <span className="stat--stopped"><span className="stat-dot" style={{ background: "var(--c)" }} /> {counts.todo} to do</span>
            <span className="stat--running"><span className="stat-dot" style={{ background: "var(--c)" }} /> {counts.done} done</span>
          </div>
        </div>
        <button className="btn primary" onClick={onStart}><Icon name="plus" size={15} /> Start session</button>
      </header>

      {needsStrip && <NeedsStrip onOpen={onOpen} />}

      <div className="fleet-toolbar">
        <div className="searchbox">
          <Icon name="search" size={14} />
          <input placeholder="Filter tasks…" value={q} onChange={(e) => setQ(e.target.value)} />
        </div>
        <div className="filter-chips">
          {TFILTERS.map((f) => (
            <button key={f.k} className={`fchip${active.includes(f.k) ? " on" : ""}`} onClick={() => toggle(f.k)}
              style={{ "--c": TDOT[f.k] }}>
              <span className="stat-dot" style={{ background: TDOT[f.k] }} />{f.label}
            </button>
          ))}
          <button className={`fchip${attentionOnly ? " on" : ""}`} onClick={() => setAttentionOnly((v) => !v)} style={{ "--c": "var(--st-stalled)" }}>
            <Icon name="warning" size={12} /> Needs attention {counts.attention > 0 && `(${counts.attention})`}
          </button>
        </div>
        <div className="tb-spacer" />
        <select className="mini-select sess-select" value={sessionF} onChange={(e) => setSessionF(e.target.value)} title="Filter by session">
          <option value="all">All sessions</option>
          {sessions.map((s) => <option key={s.id} value={s.id}>{s.objective}</option>)}
        </select>
        <select className="mini-select" value={toolF} onChange={(e) => setToolF(e.target.value)}>
          <option value="all">All tools</option>
          {window.DATA.TOOLS.map((t) => <option key={t.id} value={t.id}>{t.name}</option>)}
        </select>
        <select className="mini-select" value={backendF} onChange={(e) => setBackendF(e.target.value)}>
          <option value="all">All backends</option>
          {window.DATA.BACKENDS.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
        </select>
      </div>

      <div className="screen-scroll">
        {tasks.length === 0 ? (
          <EmptyState icon="board" title="No tasks match"
            body="Try clearing filters or search across the fleet."
            action={filtersOn ? <button className="btn" onClick={() => { setQ(""); setActive([]); setSessionF("all"); setToolF("all"); setBackendF("all"); setAttentionOnly(false); }}>Clear filters</button> : null} />
        ) : layout === "table" ? (
          <TaskTableGlobal tasks={tasks} onOpen={onOpen} statusStyle={statusStyle} />
        ) : layout === "list" ? (
          <TaskListGrouped tasks={tasks} onOpen={onOpen} statusStyle={statusStyle} />
        ) : (
          <TaskBoardGlobal tasks={tasks} onOpen={onOpen} />
        )}
      </div>
    </div>
  );
}

Object.assign(window, { TasksView });
