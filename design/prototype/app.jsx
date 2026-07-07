/* DAEDALUS — app shell: nav, global chrome, routing, shortcuts, tweaks */

const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "theme": "system",
  "skin": "yaru",
  "accent": "#E95420",
  "nav": "topbar",
  "tasksLayout": "list",
  "fleetLayout": "list",
  "sessionSplit": "terminal",
  "statusStyle": "iconled",
  "density": "regular",
  "windowFrame": true,
  "demoState": "normal",
  "needsPlacement": "view",
  "discoverState": "normal",
  "startState": "normal",
  "backendsState": "normal",
  "toolsEmpty": false
}/*EDITMODE-END*/;

const SKIN_ACCENT = { mac: "#E0901C", yaru: "#E95420" };

function NavItem({ item, active, counts, collapsed, onClick }) {
  const isNeeds = item.id === "needs";
  const badge = item.id === "fleet" && counts.attention > 0 ? counts.attention
    : isNeeds && counts.needs > 0 ? counts.needs : null;
  return (
    <button className={`nav-item${active ? " on" : ""}`} onClick={onClick} title={item.label}>
      <span className="nav-ic"><Icon name={item.icon} size={17} /></span>
      {!collapsed && <span className="nav-label">{item.label}</span>}
      {!collapsed && item.id === "fleet" && counts.running > 0 && <span className="nav-count">{counts.running}</span>}
      {badge && <span className={`nav-badge${isNeeds ? " await" : ""}`} title={isNeeds ? `${badge} waiting on you` : `${badge} need attention`}>{badge}</span>}
    </button>
  );
}

function Sidebar({ view, counts, onNav, hosts, navItems }) {
  return (
    <nav className="sidebar">
      <div className="side-section">
        {navItems.map((n) => (
          <NavItem key={n.id} item={n} active={view === n.id} counts={counts} onClick={() => onNav(n.id)} />
        ))}
      </div>
      <div className="tb-spacer" />
      <div className="side-backends">
        <div className="side-glabel">Hosts</div>
        {hosts.map((h) => (
          <div key={h.id} className="side-backend" title={h.name + " — " + h.availability}>
            <AvailDot state={h.availability === "idle" ? "degraded" : h.availability} />
            <span className="sb-name"><Icon name={h.kind === "tunnel" ? "tunnel" : h.kind === "mdns" ? "globe" : "host"} size={12} /> {h.name}</span>
          </div>
        ))}
      </div>
    </nav>
  );
}

function TopNav({ view, counts, onNav, navItems }) {
  return (
    <nav className="topnav">
      {navItems.map((n) => (
        <button key={n.id} className={`topnav-item${view === n.id ? " on" : ""}`} onClick={() => onNav(n.id)}>
          <Icon name={n.icon} size={15} /> {n.label}
          {n.id === "fleet" && counts.running > 0 && <span className="topnav-count">{counts.running}</span>}
          {n.id === "needs" && counts.needs > 0 && <span className="topnav-count await">{counts.needs}</span>}
        </button>
      ))}
    </nav>
  );
}

function GlobalStatus({ counts }) {
  return (
    <div className="gstatus">
      <span className="gs-item stat--running"><span className="stat-dot live" style={{ background: "var(--c)" }} />{counts.running}</span>
      {counts.await > 0 && <span className="gs-item stat--await" title={`${counts.await} waiting on you (questions + confirmations)`}><span className="stat-dot" style={{ background: "var(--c)" }} />{counts.await}</span>}
      {counts.stalled > 0 && <span className="gs-item stat--stalled"><span className="stat-dot" style={{ background: "var(--c)" }} />{counts.stalled}</span>}
      {counts.failed > 0 && <span className="gs-item stat--failed"><span className="stat-dot" style={{ background: "var(--c)" }} />{counts.failed}</span>}
    </div>
  );
}

// Topbar-mode hosts indicator (brief §6.1) — the sidebar carries host
// availability; when nav is the top bar this compact pill does, one dot per
// host, worst state first. Click for the full list (same row treatment as the
// sidebar); dismisses like NotifPopover (scrim click).
const AVAIL_RANK = { unavailable: 0, degraded: 1, idle: 1, available: 2 };
function HostsIndicator({ hosts }) {
  const [open, setOpen] = React.useState(false);
  const sorted = [...hosts].sort((a, b) => (AVAIL_RANK[a.availability] ?? 2) - (AVAIL_RANK[b.availability] ?? 2));
  return (
    <div style={{ position: "relative" }}>
      <button className={`hosts-pill${open ? " on" : ""}`} onClick={() => setOpen((o) => !o)} title="Hosts — availability">
        {sorted.map((h) => (
          <AvailDot key={h.id} state={h.availability === "idle" ? "degraded" : h.availability} />
        ))}
        <span>{hosts.length}</span>
      </button>
      {open && (
        <>
          <div className="pop-scrim" onMouseDown={() => setOpen(false)} />
          <div className="hosts-pop fadeup">
            <div className="notif-head"><b>Hosts</b></div>
            <div className="hosts-pop-list">
              {sorted.map((h) => (
                <div key={h.id} className="side-backend" title={h.note || h.name}>
                  <AvailDot state={h.availability === "idle" ? "degraded" : h.availability} />
                  <span className="sb-name"><Icon name={h.kind === "tunnel" ? "tunnel" : h.kind === "mdns" ? "globe" : "host"} size={12} /> {h.name}</span>
                  <span className="hosts-avail">{h.availability}</span>
                </div>
              ))}
            </div>
            <div className="hosts-pop-note">Availability from local checks, mDNS presence, and tunnel state.</div>
          </div>
        </>
      )}
    </div>
  );
}

function App() {
  const [t, setTweak] = useTweaks(TWEAK_DEFAULTS);
  const [view, setView] = React.useState("tasks");
  const [session, setSession] = React.useState(null);
  const [sessionOrigin, setSessionOrigin] = React.useState("tasks");
  const [fleetFilter, setFleetFilter] = React.useState(null);
  const [paletteOpen, setPaletteOpen] = React.useState(false);
  const [notifOpen, setNotifOpen] = React.useState(false);
  const [settingsOpen, setSettingsOpen] = React.useState(false);
  const [needsTrayOpen, setNeedsTrayOpen] = React.useState(false);
  const [answerMode, setAnswerMode] = React.useState(false);

  const SESSIONS = window.DATA.SESSIONS;
  const needsList = window.DATA.needsYou();
  const counts = {
    running: SESSIONS.filter((s) => s.status === "running").length,
    stalled: SESSIONS.filter((s) => s.status === "stalled").length,
    failed: SESSIONS.filter((s) => s.status === "failed").length,
    // blocked on the operator: a question (awaiting) or an unconfirmed clean exit (confirm)
    await: SESSIONS.filter((s) => ["awaiting", "confirm"].includes(s.status)).length,
    attention: SESSIONS.filter((s) => ["stalled", "failed", "unknown", "awaiting", "confirm"].includes(s.status)).length,
    needs: needsList.length,
  };

  // apply tweaks to root — theme "system" follows the OS appearance.
  React.useEffect(() => {
    const r = document.documentElement;
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => r.setAttribute("data-theme", t.theme === "system" ? (mq.matches ? "dark" : "light") : t.theme);
    apply();
    r.setAttribute("data-skin", t.skin);
    r.setAttribute("data-density", t.density);
    if (t.accent) { r.style.setProperty("--accent", t.accent); }
    if (t.theme === "system") { mq.addEventListener("change", apply); return () => mq.removeEventListener("change", apply); }
  }, [t.theme, t.skin, t.density, t.accent]);

  const openSession = (id, opts = {}) => {
    if (id === "__discover") { setView("discover"); return; }
    const s = SESSIONS.find((x) => x.id === id);
    if (s) {
      setSession(s);
      setAnswerMode(!!opts.answer);
      setSessionOrigin(view === "session" ? sessionOrigin : (view === "fleet" ? "fleet" : view === "needs" ? "needs" : "tasks"));
      setView("session");
    }
  };
  const nav = (id) => {
    if (id === "__theme") { toggleTheme(); return; }
    if (id === "start") { setView("start"); return; }
    if (id === "settings") { setSettingsOpen(true); return; }
    if (id === "fleet") setFleetFilter(null);
    setAnswerMode(false);
    setView(id); setSession(null);
  };
  const goToSessions = (filter) => { setFleetFilter(filter || null); setView("fleet"); setSession(null); };
  const toggleTheme = () => {
    const cur = t.theme === "system" ? (window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light") : t.theme;
    setTweak("theme", cur === "dark" ? "light" : "dark");
  };
  // Switching platform skin also adopts that platform's signature accent,
  // so Yaru reads as Ubuntu (orange) and macOS as amber unless overridden.
  const setSkin = (v) => setTweak({ skin: v, accent: SKIN_ACCENT[v] });

  // keyboard shortcuts
  React.useEffect(() => {
    const onKey = (e) => {
      const meta = e.metaKey || e.ctrlKey;
      if (meta && e.key.toLowerCase() === "k") { e.preventDefault(); setPaletteOpen((o) => !o); }
      else if (meta && e.key.toLowerCase() === "n") { e.preventDefault(); setView("start"); }
      else if (meta && e.shiftKey && e.key.toLowerCase() === "l") { e.preventDefault(); toggleTheme(); }
      else if (meta && e.key === "[") { e.preventDefault(); if (view === "session") { setView(sessionOrigin); setSession(null); } }
      else if (e.key === "Escape" && view === "session") { /* keep */ }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [view, t.theme, sessionOrigin]);

  let screen;
  if (view === "tasks") screen = <TasksView layout={t.tasksLayout} statusStyle={t.statusStyle} onOpen={openSession} onStart={() => setView("start")} needsStrip={t.needsPlacement === "strip"} />;
  else if (view === "needs") screen = <NeedsView onOpen={openSession} />;
  else if (view === "fleet") screen = <Fleet layout={t.fleetLayout} statusStyle={t.statusStyle} demoState={t.demoState} onOpen={openSession} onStart={() => setView("start")} density={t.density} filter={fleetFilter} onClearFilter={() => setFleetFilter(null)} />;
  else if (view === "session" && session) screen = <SessionDetail session={session} split={t.sessionSplit} statusStyle={t.statusStyle} answer={answerMode} onBack={() => { setView(sessionOrigin); setSession(null); }} />;
  else if (view === "start") screen = <StartSession onLaunch={(id) => openSession(id)} onCancel={() => setView("fleet")} demoState={t.startState} />;
  else if (view === "discover") screen = <Discover statusStyle={t.statusStyle} onOpen={openSession} demoState={t.discoverState} onSettings={() => setSettingsOpen(true)} />;
  else if (view === "tools") screen = <Tools empty={t.toolsEmpty} />;
  else if (view === "backends") screen = <Backends onOpenSessions={goToSessions} demoState={t.backendsState} onDiscover={() => nav("discover")} onStart={() => setView("start")} />;

  const navView = view === "start" ? "tasks" : view === "session" ? sessionOrigin : view;
  const navItems = t.needsPlacement === "view" ? [{ id: "needs", label: "Needs you", icon: "inbox" }, ...window.DATA.NAV] : window.DATA.NAV;

  return (
    <div className="desktop">
      <div className={`win${t.windowFrame ? " framed" : ""}`}>
        {/* TITLEBAR */}
        <div className="titlebar">
          {t.skin === "mac" ? (
            <div className="lights"><span className="light r" /><span className="light y" /><span className="light g" /></div>
          ) : null}
          <div className="tb-brand">
            <span className="brand-mark"><Icon name="maze" size={15} /></span>
            <span className="tb-title">Daedalus</span>
          </div>
          {t.nav === "topbar" && <TopNav view={navView} counts={counts} onNav={nav} navItems={navItems} />}
          <div className="tb-spacer" />
          <GlobalStatus counts={counts} />
          {t.nav === "topbar" && <HostsIndicator hosts={window.DATA.HOSTS} />}
          {t.needsPlacement === "tray" && (
            <div style={{ position: "relative" }}>
              <button className={`iconbtn${needsTrayOpen ? " on" : ""}`} onClick={() => setNeedsTrayOpen((o) => !o)} title="Needs you">
                <Icon name="inbox" size={16} />
                {counts.needs > 0 && <span className="notif-badge await">{counts.needs}</span>}
              </button>
              <NeedsTray open={needsTrayOpen} onClose={() => setNeedsTrayOpen(false)} onOpen={openSession} />
            </div>
          )}
          <button className="cmd-trigger" onClick={() => setPaletteOpen(true)}>
            <Icon name="search" size={13} /> <span>Search</span> <span className="cmd-kbd"><kbd>⌘</kbd><kbd>K</kbd></span>
          </button>
          <div style={{ position: "relative" }}>
            <button className={`iconbtn${notifOpen ? " on" : ""}`} onClick={() => setNotifOpen((o) => !o)} title="Notifications">
              <Icon name="bell" size={16} />
              {counts.attention > 0 && <span className="notif-badge">{counts.attention}</span>}
            </button>
            <NotifPopover open={notifOpen} onClose={() => setNotifOpen(false)} onOpenSession={openSession} />
          </div>
          <button className={`iconbtn${settingsOpen ? " on" : ""}`} onClick={() => setSettingsOpen(true)} title="Settings">
            <Icon name="settings" size={16} />
          </button>
          {t.skin === "yaru" ? (
            <div className="lights"><span className="light"><Icon name="minus" size={11} /></span><span className="light"><Icon name="square" size={10} /></span><span className="light r"><Icon name="close" size={11} /></span></div>
          ) : null}
        </div>

        {/* BODY */}
        <div className="win-body">
          {t.nav === "sidebar" && <Sidebar view={navView} counts={counts} onNav={nav} hosts={window.DATA.HOSTS} navItems={navItems} />}
          <main className="content">{screen}</main>
        </div>
      </div>

      <CommandPalette open={paletteOpen} onClose={() => setPaletteOpen(false)} onNav={nav} onOpenSession={openSession} />
      {settingsOpen && <SettingsModal tweaks={t} setTweak={setTweak} onClose={() => setSettingsOpen(false)} />}

      {/* TWEAKS */}
      <TweaksPanel title="Tweaks">
        <TweakSection label="Theme & platform" />
        <TweakRadio label="Theme" value={t.theme} options={[{ value: "system", label: "System" }, { value: "light", label: "Light" }, { value: "dark", label: "Dark" }]} onChange={(v) => setTweak("theme", v)} />
        <TweakRadio label="Platform skin" value={t.skin} options={[{ value: "mac", label: "macOS" }, { value: "yaru", label: "Linux" }]} onChange={setSkin} />
        <TweakColor label="Accent" value={t.accent} options={["#E0901C", "#0A84FF", "#26B5A8", "#E95420", "#7A5AE0"]} onChange={(v) => setTweak("accent", v)} />
        <TweakToggle label="Window frame" value={t.windowFrame} onChange={(v) => setTweak("windowFrame", v)} />

        <TweakSection label="Navigation & density" />
        <TweakRadio label="Navigation" value={t.nav} options={[{ value: "sidebar", label: "Sidebar" }, { value: "topbar", label: "Top bar" }]} onChange={(v) => setTweak("nav", v)} />
        <TweakRadio label="Density" value={t.density} options={["compact", "regular", "comfy"]} onChange={(v) => setTweak("density", v)} />

        <TweakSection label="Status language" />
        <TweakSelect label="Status style" value={t.statusStyle} options={[{ value: "pill", label: "Filled pill + icon" }, { value: "dotlabel", label: "Dot + label" }, { value: "iconled", label: "Icon-led" }]} onChange={(v) => setTweak("statusStyle", v)} />

        <TweakSection label="Needs you" />
        <TweakSelect label="Surface" value={t.needsPlacement} options={[{ value: "view", label: "Dedicated screen + nav" }, { value: "tray", label: "Header tray popover" }, { value: "strip", label: "Strip on Tasks home" }]} onChange={(v) => setTweak("needsPlacement", v)} />
        <TweakButton label="Answer a waiting session →" secondary onClick={() => openSession("s-909", { answer: true })} />

        <TweakSection label="Tasks (home)" />
        <TweakRadio label="Task layout" value={t.tasksLayout} options={[{ value: "board", label: "Board" }, { value: "list", label: "List" }, { value: "table", label: "Table" }]} onChange={(v) => setTweak("tasksLayout", v)} />

        <TweakSection label="Fleet layout" />
        <TweakRadio label="Layout" value={t.fleetLayout} options={[{ value: "list", label: "List" }, { value: "table", label: "Table" }]} onChange={(v) => setTweak("fleetLayout", v)} />
        <TweakSelect label="Fleet state" value={t.demoState} options={[{ value: "normal", label: "Populated" }, { value: "empty", label: "Empty (first run)" }, { value: "loading", label: "Loading" }]} onChange={(v) => setTweak("demoState", v)} />

        <TweakSection label="Session detail" />
        <TweakSelect label="Split" value={t.sessionSplit} options={[{ value: "terminal", label: "Terminal-primary" }, { value: "split", label: "Balanced split" }, { value: "board", label: "Board-primary" }]} onChange={(v) => setTweak("sessionSplit", v)} />
        <TweakButton label="Open a stalled session →" secondary onClick={() => openSession("s-902")} />

        {/* Screen states — each select also jumps to its screen so the state is visible */}
        <TweakSection label="Screen states" />
        <TweakSelect label="Discover state" value={t.discoverState}
          options={[{ value: "normal", label: "Normal" }, { value: "scanning", label: "Scanning" }, { value: "empty", label: "Empty" }, { value: "source-dropped", label: "Source dropped (tunnel)" }]}
          onChange={(v) => { setTweak("discoverState", v); nav("discover"); }} />
        <TweakSelect label="Start flow state" value={t.startState}
          options={[{ value: "normal", label: "Normal" }, { value: "validation", label: "Validation errors" }, { value: "provisioning-failed", label: "Provisioning failed" }, { value: "limit-reached", label: "Concurrency limit reached" }, { value: "env-unreachable", label: "Environment unreachable" }, { value: "worktree-failed", label: "Worktree creation failed" }]}
          onChange={(v) => { setTweak("startState", v); nav("start"); }} />
        <TweakSelect label="Backends state" value={t.backendsState}
          options={[{ value: "normal", label: "Normal" }, { value: "host-down", label: "Host down (tunnel)" }, { value: "no-envs", label: "No environments" }]}
          onChange={(v) => { setTweak("backendsState", v); nav("backends"); }} />
        <TweakToggle label="Tools empty (first run)" value={t.toolsEmpty}
          onChange={(v) => { setTweak("toolsEmpty", v); nav("tools"); }} />
        <TweakButton label="Review a session to confirm →" secondary onClick={() => openSession("s-911")} />
      </TweaksPanel>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(<App />);
