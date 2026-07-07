/* DAEDALUS — command palette (⌘K) + notifications popover */

function CommandPalette({ open, onClose, onNav, onOpenSession }) {
  const [q, setQ] = React.useState("");
  const [sel, setSel] = React.useState(0);
  const inputRef = React.useRef(null);

  const commands = React.useMemo(() => {
    const navCmds = window.DATA.NAV.map((n) => ({ type: "nav", id: n.id, icon: n.icon, label: "Go to " + n.label, hint: "Navigate" }));
    const actions = [
      { type: "action", id: "start", icon: "plus", label: "Start a session", hint: "Action", kbd: ["⌘", "N"] },
      { type: "action", id: "discover", icon: "discover", label: "Discover sessions", hint: "Action" },
      { type: "theme", id: "theme", icon: "moon", label: "Toggle theme", hint: "Action", kbd: ["⌘", "⇧", "L"] },
    ];
    const sessions = window.DATA.SESSIONS.map((s) => ({
      type: "session", id: s.id, status: s.status, label: s.objective, hint: s.toolName + " · " + window.DATA.STATUS_LABEL[s.status],
    }));
    return [...actions, ...navCmds, ...sessions];
  }, []);

  const filtered = commands.filter((c) => !q || c.label.toLowerCase().includes(q.toLowerCase()) || (c.hint || "").toLowerCase().includes(q.toLowerCase()));

  React.useEffect(() => { if (open) { setQ(""); setSel(0); setTimeout(() => inputRef.current?.focus(), 30); } }, [open]);
  React.useEffect(() => { setSel(0); }, [q]);

  const run = (c) => {
    if (!c) return;
    onClose();
    if (c.type === "nav") onNav(c.id);
    else if (c.type === "session") onOpenSession(c.id);
    else if (c.type === "action") { if (c.id === "start") onNav("start"); else if (c.id === "discover") onNav("discover"); }
    else if (c.type === "theme") onNav("__theme");
  };
  const onKey = (e) => {
    if (e.key === "ArrowDown") { e.preventDefault(); setSel((s) => Math.min(s + 1, filtered.length - 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setSel((s) => Math.max(s - 1, 0)); }
    else if (e.key === "Enter") { e.preventDefault(); run(filtered[sel]); }
    else if (e.key === "Escape") onClose();
  };

  if (!open) return null;
  return (
    <div className="palette-scrim" onMouseDown={onClose}>
      <div className="palette" onMouseDown={(e) => e.stopPropagation()}>
        <div className="palette-input">
          <Icon name="command" size={16} />
          <input ref={inputRef} value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={onKey}
            placeholder="Search sessions, run a command…" />
          <kbd>esc</kbd>
        </div>
        <div className="palette-list">
          {filtered.length === 0 && <div className="palette-empty">No matches</div>}
          {filtered.map((c, i) => (
            <button key={c.type + c.id} className={`palette-item${i === sel ? " on" : ""}`}
              onMouseEnter={() => setSel(i)} onClick={() => run(c)}>
              <span className="pi-icon">
                {c.type === "session" ? <span className={`stat stat--${c.status}`}><span className="stat-glyph" style={{ width: 15, height: 15 }}><StatusGlyph status={c.status} size={15} /></span></span>
                  : <Icon name={c.icon} size={16} />}
              </span>
              <span className="pi-label">{c.label}</span>
              <span className="pi-hint">{c.hint}</span>
              {c.kbd && <span className="pi-kbd">{c.kbd.map((k, j) => <kbd key={j}>{k}</kbd>)}</span>}
            </button>
          ))}
        </div>
        <div className="palette-foot">
          <span><kbd>↑</kbd><kbd>↓</kbd> navigate</span>
          <span><kbd>↵</kbd> open</span>
          <span><kbd>⌘</kbd><kbd>K</kbd> palette</span>
        </div>
      </div>
    </div>
  );
}

function NotifPopover({ open, onClose, onOpenSession }) {
  if (!open) return null;
  const items = window.DATA.NOTIFS;
  return (
    <>
      <div className="pop-scrim" onMouseDown={onClose} />
      <div className="notif-pop fadeup">
        <div className="notif-head"><b>Notifications</b><button className="btn sm ghost">Mark all read</button></div>
        <div className="notif-list">
          {items.map((n) => (
            <button key={n.id} className="notif-item" onClick={() => { onClose(); onOpenSession(n.session); }}>
              <span className={`stat stat--${n.kind} stat-glyph`} style={{ width: 17, height: 17, flex: "0 0 17px" }}><StatusGlyph status={n.kind} size={17} /></span>
              <div className="ni-body">
                <div className="ni-title">{n.title}</div>
                <div className="ni-text">{n.body}</div>
              </div>
              <span className="ni-time">{n.t}m</span>
            </button>
          ))}
        </div>
      </div>
    </>
  );
}

Object.assign(window, { CommandPalette, NotifPopover });
