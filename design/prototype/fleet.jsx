/* DAEDALUS — Fleet / home. Layouts: grid · table · list. */

function FleetCard({ s, statusStyle, onOpen }) {
  const term = window.DATA.STATUS_LABEL;
  const attention = ["stalled", "failed", "unknown", "awaiting", "confirm"].includes(s.status);
  return (
    <article className={`fleet-card${attention ? " att" : ""}`} onClick={() => onOpen(s.id)} tabIndex={0}
      onKeyDown={(e) => { if (e.key === "Enter") onOpen(s.id); }}>
      <div className="fc-top">
        <div className="fc-tool">
          <span className="fc-toolname">{s.toolName}</span>
          <StatusBadge status={s.status} style={statusStyle} />
        </div>
        <button className="iconbtn fc-open" title="Open session" onClick={(e) => { e.stopPropagation(); onOpen(s.id); }}>
          <Icon name="arrowR" size={15} />
        </button>
      </div>
      <h3 className="fc-obj">{s.objective}</h3>
      <div className="fc-spec"><Icon name="doc" size={12} /> {s.spec}</div>
      <div className="fc-chips">
        {/* availability dot only when the host is degraded/down — quiet otherwise */}
        <SourceChip source={s.source} availability={window.DATA.sessionHostAvail(s)} />
        <BackendChip backend={s.backend} name={s.backendName} availability={window.DATA.sessionHostAvail(s)} />
        <Chip icon="git" className={s.envKind === "existing" ? "" : ""}>
          {s.envKind === "fresh" ? "fresh env" : s.envName}
        </Chip>
      </div>
      {s.note && <div className="fc-note"><Icon name="info" size={12} /> {s.note}</div>}
      <div className="fc-foot">
        <ProgressPill tasks={s.tasks} status={s.status} />
        <span className="fc-age"><Icon name="clock" size={11} /> {isLive(s.status) ? fmtDur(s.started) : fmtAgo(s.ended ?? s.lastEvent)}</span>
      </div>
      <div className="fc-res">
        <ResMeter icon="cpu" value={s.cpu} />
        <ResMeter icon="mem" value={s.mem} />
        <ResMeter icon="disk" value={s.disk} />
      </div>
    </article>
  );
}

function FleetRow({ s, statusStyle, onOpen }) {
  const attention = ["stalled", "failed", "unknown", "awaiting", "confirm"].includes(s.status);
  const { done, total } = taskCounts(s.tasks);
  return (
    <div className={`fleet-row${attention ? " att" : ""}`} onClick={() => onOpen(s.id)} tabIndex={0}
      onKeyDown={(e) => { if (e.key === "Enter") onOpen(s.id); }}>
      <div style={{ width: 130, flexShrink: 0 }}><StatusBadge status={s.status} style={statusStyle} /></div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div className="fr-obj">{s.objective}</div>
        <div className="fr-meta">{s.toolName} · {s.spec}</div>
      </div>
      <div className="fr-chips">
        <SourceChip source={s.source} availability={window.DATA.sessionHostAvail(s)} />
        <BackendChip backend={s.backend} name={s.backendName} availability={window.DATA.sessionHostAvail(s)} />
      </div>
      <div style={{ width: 110, flexShrink: 0 }}><ProgressPill tasks={s.tasks} status={s.status} /></div>
      <div className="fr-age" style={{ width: 76, flexShrink: 0, textAlign: "right" }}>
        {isLive(s.status) ? fmtDur(s.started) : fmtAgo(s.ended ?? s.lastEvent)}
      </div>
      <Icon name="chevR" size={14} className="fr-chev" />
    </div>
  );
}

// Rich multiline list row — harmonized with the Tasks list: identity + branch,
// exec-environment demoted to the detail view, "Needs you" surfaced inline.
function FleetRowRich({ s, statusStyle, onOpen }) {
  const needs = needsMeta(s.status) ? s.status : null;
  const live = isLive(s.status);
  // host reachability — dot shown only when degraded/down (quiet when healthy)
  const hostAvail = window.DATA.sessionHostAvail(s);
  return (
    <article className={`srow${needs ? " needs ny--" + needs : ""}`} onClick={() => onOpen(s.id)} tabIndex={0}
      onKeyDown={(e) => { if (e.key === "Enter") onOpen(s.id); }}>
      <div className="srow-status"><StatusBadge status={s.status} style={statusStyle} /></div>
      <div className="srow-main">
        <div className="srow-l1">
          <h3 className="srow-obj">{s.objective}</h3>
          {needs && <NeedsTag status={needs} />}
        </div>
        <div className="srow-sub">
          <span className="row-branch"><Icon name="git" size={11} /> {s.spec}</span>
          <span className="row-meta"><Icon name="tools" size={11} /> {s.toolName}</span>
          <span className="row-meta"><Icon name="backends" size={11} /> {s.backendName}{hostAvail !== "available" && <AvailDot state={hostAvail} />}</span>
          <IssueChip issue={s.issue} />
        </div>
      </div>
      <div className="srow-right">
        <div className="srow-prog"><ProgressPill tasks={s.tasks} status={s.status} /></div>
        <div className="srow-age"><Icon name="clock" size={11} /> {live ? fmtDur(s.started) : fmtAgo(s.ended ?? s.lastEvent)}</div>
      </div>
      <Icon name="chevR" size={15} className="srow-chev" />
    </article>
  );
}

function FleetTable({ sessions, statusStyle, onOpen }) {
  return (
    <div className="fleet-table card">
      <div className="ft-head">
        <span style={{ width: 124 }}>Status</span>
        <span style={{ flex: 1 }}>Objective</span>
        <span style={{ width: 130 }}>Tool</span>
        <span style={{ width: 150 }}>Backend</span>
        <span style={{ width: 86 }}>Env</span>
        <span style={{ width: 92 }}>Tasks</span>
        <span style={{ width: 64, textAlign: "right" }}>CPU</span>
        <span style={{ width: 64, textAlign: "right" }}>Mem</span>
        <span style={{ width: 78, textAlign: "right" }}>Age</span>
      </div>
      {sessions.map((s) => {
        const attention = ["stalled", "failed", "unknown", "awaiting", "confirm"].includes(s.status);
        return (
          <div key={s.id} className={`ft-row${attention ? " att" : ""}`} onClick={() => onOpen(s.id)} tabIndex={0}
            onKeyDown={(e) => { if (e.key === "Enter") onOpen(s.id); }}>
            <span style={{ width: 124 }}><StatusBadge status={s.status} style={statusStyle} /></span>
            <span style={{ flex: 1, minWidth: 0 }} className="ft-obj">{s.objective}</span>
            <span style={{ width: 130 }} className="ft-dim">{s.toolName}</span>
            <span style={{ width: 150 }} className="ft-dim">{s.backendName}</span>
            <span style={{ width: 86 }} className="ft-dim mono">{s.envKind === "fresh" ? "fresh" : s.envName.split("/").pop()}</span>
            <span style={{ width: 92 }}><ProgressPill tasks={s.tasks} status={s.status} showBar={false} /></span>
            <span style={{ width: 64, textAlign: "right" }} className="ft-num">{s.cpu == null ? "—" : s.cpu + "%"}</span>
            <span style={{ width: 64, textAlign: "right" }} className="ft-num">{s.mem == null ? "—" : s.mem + "%"}</span>
            <span style={{ width: 78, textAlign: "right" }} className="ft-num">{isLive(s.status) ? fmtDur(s.started) : fmtAgo(s.ended ?? s.lastEvent)}</span>
          </div>
        );
      })}
    </div>
  );
}

const STATUS_FILTERS = ["running", "awaiting", "confirm", "stalled", "failed", "completed", "stopped", "unknown"];

function Fleet({ layout, statusStyle, demoState, onOpen, onStart, density, filter, onClearFilter }) {
  const all = window.DATA.SESSIONS;
  const [q, setQ] = React.useState("");
  const [active, setActive] = React.useState([]); // status filters
  const [backend, setBackend] = React.useState((filter && filter.backend) || "all");
  const [sort, setSort] = React.useState("attention");
  const envFilter = filter && filter.env;
  const hostFilter = filter && filter.host;
  const envName = envFilter ? (window.DATA.ENVIRONMENTS.find((e) => e.id === envFilter) || {}).name : null;
  const hostName = hostFilter ? (window.DATA.HOSTS.find((h) => h.id === hostFilter) || {}).name : null;

  const toggle = (st) => setActive((a) => a.includes(st) ? a.filter((x) => x !== st) : [...a, st]);

  let sessions = all.filter((s) => {
    if (q && !(`${s.objective} ${s.toolName} ${s.spec}`.toLowerCase().includes(q.toLowerCase()))) return false;
    if (active.length && !active.includes(s.status)) return false;
    if (backend !== "all" && s.backend !== backend) return false;
    if (envFilter && s.env !== envFilter) return false;
    if (hostFilter && window.DATA.placement(s.backend).host !== hostFilter) return false;
    return true;
  });
  const order = { awaiting: 0, confirm: 1, stalled: 2, failed: 3, unknown: 4, starting: 5, running: 6, completed: 7, stopped: 8 };
  if (sort === "attention") sessions = [...sessions].sort((a, b) => (order[a.status] - order[b.status]) || (a.started - b.started));
  else if (sort === "recent") sessions = [...sessions].sort((a, b) => (a.lastEvent ?? 0) - (b.lastEvent ?? 0));
  else if (sort === "name") sessions = [...sessions].sort((a, b) => a.objective.localeCompare(b.objective));

  const counts = {
    running: all.filter((s) => s.status === "running").length,
    stalled: all.filter((s) => s.status === "stalled").length,
    failed: all.filter((s) => s.status === "failed").length,
  };

  // demo states
  if (demoState === "empty") {
    return (
      <div className="screen">
        <FleetHeader counts={counts} onStart={onStart} />
        <EmptyState icon="fleet" title="No sessions yet"
          body="Start an agent against an objective, or discover sessions running on other hosts. Daedalus keeps every run isolated in its own sandbox."
          action={<div style={{ display: "flex", gap: 9 }}>
            <button className="btn primary" onClick={onStart}><Icon name="plus" size={14} /> Start a session</button>
            <button className="btn" onClick={() => onOpen("__discover")}><Icon name="discover" size={14} /> Discover</button>
          </div>} />
      </div>
    );
  }
  if (demoState === "loading") {
    return (
      <div className="screen">
        <FleetHeader counts={counts} onStart={onStart} />
        <div className="fleet-grid" style={{ padding: "0 22px 22px" }}>
          {[0,1,2,3,4,5].map((i) => <div key={i} className="fleet-card skeleton" style={{ animationDelay: i*60 + "ms" }} />)}
        </div>
      </div>
    );
  }

  return (
    <div className="screen">
      <FleetHeader counts={counts} onStart={onStart} />
      {(envName || hostName || (backend !== "all" && filter && filter.backend)) && (
        <div className="fleet-context">
          <span className="fctx-pill">
            <Icon name={envName ? "git" : hostName ? "host" : "backends"} size={12} />
            {envName ? <>Environment <b>{envName}</b></> : hostName ? <>Host <b>{hostName}</b></> : <>Backend <b>{(window.DATA.BACKENDS.find((b) => b.id === backend) || {}).name}</b></>}
            <button className="fctx-x" onClick={() => { setBackend("all"); onClearFilter && onClearFilter(); }} title="Clear"><Icon name="close" size={11} /></button>
          </span>
          <span className="fctx-count">{sessions.length} session{sessions.length === 1 ? "" : "s"}</span>
        </div>
      )}
      {/* filter / toolbar */}
      <div className="fleet-toolbar">
        <div className="searchbox">
          <Icon name="search" size={14} />
          <input placeholder="Filter sessions…" value={q} onChange={(e) => setQ(e.target.value)} />
        </div>
        <div className="filter-chips">
          {STATUS_FILTERS.map((st) => (
            <button key={st} className={`fchip stat--${st}${active.includes(st) ? " on" : ""}`} onClick={() => toggle(st)}>
              <span className="stat-dot" style={{ background: "var(--c)" }} />
              {window.DATA.STATUS_LABEL[st]}
            </button>
          ))}
        </div>
        <div className="tb-spacer" />
        <select className="mini-select" value={backend} onChange={(e) => setBackend(e.target.value)}>
          <option value="all">All backends</option>
          {window.DATA.BACKENDS.map((b) => <option key={b.id} value={b.id}>{b.name}</option>)}
        </select>
        <select className="mini-select" value={sort} onChange={(e) => setSort(e.target.value)}>
          <option value="attention">Sort: Attention</option>
          <option value="recent">Sort: Recent activity</option>
          <option value="name">Sort: Objective</option>
        </select>
      </div>

      <div className="screen-scroll">
        {sessions.length === 0 ? (
          <EmptyState icon="search" title="No sessions match"
            body="Try clearing filters or search."
            action={<button className="btn" onClick={() => { setQ(""); setActive([]); setBackend("all"); onClearFilter && onClearFilter(); }}>Clear filters</button>} />
        ) : layout === "table" ? (
          <FleetTable sessions={sessions} statusStyle={statusStyle} onOpen={onOpen} />
        ) : (
          <div className="fleet-list">
            {sessions.map((s, i) => (
              <div key={s.id} className="fadeup" style={{ animationDelay: Math.min(i * 26, 180) + "ms" }}>
                <FleetRowRich s={s} statusStyle={statusStyle} onOpen={onOpen} />
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function FleetHeader({ counts, onStart }) {
  return (
    <header className="screen-head">
      <div className="sh-title">
        <h1>Sessions</h1>
        <div className="sh-stats">
          <span className="stat--running"><span className="stat-dot live" style={{ background: "var(--c)" }} /> {counts.running} running</span>
          {counts.stalled > 0 && <span className="stat--stalled"><span className="stat-dot" style={{ background: "var(--c)" }} /> {counts.stalled} stalled</span>}
          {counts.failed > 0 && <span className="stat--failed"><span className="stat-dot" style={{ background: "var(--c)" }} /> {counts.failed} failed</span>}
        </div>
      </div>
      <button className="btn primary" onClick={onStart}><Icon name="plus" size={15} /> Start session</button>
    </header>
  );
}

Object.assign(window, { Fleet });
