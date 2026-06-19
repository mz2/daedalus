/* DAEDALUS — shell screens: Start session, Discover, Tools, Environments, Settings */

function ScreenHead({ title, sub, action }) {
  return (
    <header className="screen-head">
      <div className="sh-title"><h1>{title}</h1>{sub && <div className="sh-sub">{sub}</div>}</div>
      {action}
    </header>
  );
}

/* ───────────────────────── START SESSION (single screen) ───────────────── */
function FormSection({ n, title, sub, done, children }) {
  return (
    <section className="fsec">
      <div className="fsec-head">
        <span className={`fsec-n${done ? " done" : ""}`}>{done ? <Icon name="check" size={13} /> : n}</span>
        <div><h3>{title}</h3>{sub && <div className="fsec-sub">{sub}</div>}</div>
      </div>
      <div className="fsec-body">{children}</div>
    </section>
  );
}

function ConfirmLaunchModal({ summary, onClose, onLaunch }) {
  return (
    <div className="palette-scrim" onMouseDown={onClose}>
      <div className="modal" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h2>Review &amp; launch</h2>
          <button className="iconbtn" onClick={onClose}><Icon name="close" size={16} /></button>
        </div>
        <div className="modal-body">
          <div className="grouped confirm">
            {summary.map((r) => (
              <div key={r.k} className="row"><span className="cf-k">{r.k}</span><span className="cf-v">{r.v}</span></div>
            ))}
          </div>
          <div className="banner banner-ok" style={{ margin: 0 }}>
            <Icon name="shield" size={15} />
            <div>Secrets are pre-provisioned in the environment — Daedalus never displays or captures them.</div>
          </div>
        </div>
        <div className="modal-foot">
          <button className="btn ghost" onClick={onClose}>Back</button>
          <div className="tb-spacer" />
          <button className="btn primary" onClick={onLaunch}><Icon name="bolt" size={14} /> Launch session</button>
        </div>
      </div>
    </div>
  );
}

function StartSession({ onLaunch, onCancel }) {
  const [tool, setTool] = React.useState(null);
  const [repo, setRepo] = React.useState("acme/monorepo");
  const [specMode, setSpecMode] = React.useState("existing");
  const [specBranch, setSpecBranch] = React.useState(null);
  const [newSpec, setNewSpec] = React.useState("");
  const [envMode, setEnvMode] = React.useState("fresh"); // fresh | existing
  const [host, setHost] = React.useState("local");
  const [btype, setBtype] = React.useState("workshop");
  const [env, setEnv] = React.useState(null);
  const [reviewing, setReviewing] = React.useState(false);

  const TOOLS = window.DATA.TOOLS, HOSTS = window.DATA.HOSTS, TYPES = window.DATA.BACKEND_TYPES, place = window.DATA.placement;
  const existing = window.DATA.ENVIRONMENTS.filter((e) => e.kind === "existing").map((e) => ({ ...e, host: place(e.backend).host, type: place(e.backend).type }));
  const hostObj = HOSTS.find((h) => h.id === host) || HOSTS[0];
  React.useEffect(() => { if (!hostObj.supports.includes(btype)) setBtype(hostObj.supports[0]); }, [host]);
  const typeName = (id) => (TYPES.find((t) => t.id === id) || {}).name || id;
  const hostName = (id) => (HOSTS.find((h) => h.id === id) || {}).name || id;

  const REPOS = ["acme/monorepo", "acme/web-app", "acme/infra", "acme/gateway"];
  const SPEC_BRANCHES = {
    "acme/monorepo": ["specs/044-device-flow", "specs/039-async-billing", "specs/051-sdk-gen"],
    "acme/web-app": ["specs/047-auth-locks", "specs/050-event-docs"],
    "acme/infra": ["specs/048-ratelimit", "specs/021-dep-sweep"],
    "acme/gateway": ["specs/053-webhook-retry"],
  };
  const branches = SPEC_BRANCHES[repo] || [];
  const slug = newSpec.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").split("-").slice(0, 4).join("-") || "new-objective";

  const toolOk = !!tool;
  const objectiveOk = !!repo && (specMode === "existing" ? !!specBranch : !!newSpec.trim());
  const envOk = envMode === "fresh" ? !!btype : !!env;
  const valid = toolOk && objectiveOk && envOk;

  const summary = [
    { k: "Tool", v: TOOLS.find((t) => t.id === tool)?.name || "—" },
    { k: "Repository", v: <span className="mono">{repo}</span> },
    { k: "Objective", v: specMode === "existing"
      ? <span className="mono">{specBranch} · 9 tasks</span>
      : <>New spec → <span className="mono">specs/{slug}</span> <span style={{ color: "var(--text-3)" }}>(runs <code>specify</code>)</span></> },
    { k: "Environment", v: envMode === "fresh"
      ? <>Fresh · {typeName(btype)} on {hostName(host)}</>
      : <>{existing.find((e) => e.id === env)?.name} · worktree</> },
  ];

  return (
    <div className="screen">
      <ScreenHead title="Start a session" sub="Launch an agentic tool against an objective inside an environment."
        action={<button className="btn ghost" onClick={onCancel}><Icon name="close" size={14} /> Cancel</button>} />
      <div className="start-wrap screen-scroll">
        <div className="start-form">
          {/* 1 — Objective */}
          <FormSection n="1" title="Objective" sub="SpecKit convention — lives on a specs/… branch, decomposes into tracked tasks." done={objectiveOk}>
            <label className="sp-label">Repository</label>
            <div className="repo-grid">
              {REPOS.map((r) => (
                <button key={r} className={`repo-opt${repo === r ? " on" : ""}`} onClick={() => { setRepo(r); setSpecBranch(null); }}>
                  <Icon name="git" size={14} /> <span className="mono">{r}</span>
                  {repo === r && <Icon name="check" size={14} style={{ marginLeft: "auto" }} />}
                </button>
              ))}
            </div>
            <div className="seg" style={{ width: "100%", margin: "16px 0 14px" }}>
              <button style={{ flex: 1, justifyContent: "center" }} aria-pressed={specMode === "existing"} onClick={() => setSpecMode("existing")}><Icon name="doc" size={13} /> Existing spec</button>
              <button style={{ flex: 1, justifyContent: "center" }} aria-pressed={specMode === "new"} onClick={() => setSpecMode("new")}><Icon name="plus" size={13} /> New spec</button>
            </div>
            {specMode === "existing" ? (
              <>
                <label className="sp-label">Spec branch <span style={{ color: "var(--text-3)", fontWeight: 400 }}>· follows <code>specs/xxx-…</code></span></label>
                {branches.length ? (
                  <div className="env-list">
                    {branches.map((b) => (
                      <button key={b} className={`env-opt${specBranch === b ? " on" : ""}`} onClick={() => setSpecBranch(b)}>
                        <Icon name="doc" size={14} />
                        <div style={{ flex: 1, textAlign: "left" }}><div className="eo-name mono">{b}</div><div className="eo-branch">spec.md · plan.md · tasks.md</div></div>
                        {specBranch === b && <Icon name="check" size={15} />}
                      </button>
                    ))}
                  </div>
                ) : (
                  <div className="disc-empty"><Icon name="info" size={13} /> No <code>specs/…</code> branches here yet — create a new spec instead.</div>
                )}
                {specBranch && <p className="sp-detected" style={{ marginTop: 12 }}><b>9 tracked tasks</b> in <code>{specBranch}/tasks.md</code>. The agent attaches to an isolated worktree on this branch.</p>}
              </>
            ) : (
              <>
                <label className="sp-label">Describe the objective</label>
                <textarea className="input sp-textarea" rows={3} placeholder="e.g. Add OAuth device-flow to the gateway, with token polling and rate-limiting." value={newSpec} onChange={(e) => setNewSpec(e.target.value)} />
                <div className="spec-plan">
                  <div className="spec-plan-h"><Icon name="bolt" size={13} /> What happens on launch</div>
                  <ol className="spec-steps">
                    <li>Splice a fresh <b>git worktree</b> off <span className="mono">{repo}</span> on a new branch <span className="mono">specs/{slug}</span>.</li>
                    <li>Start <b>{TOOLS.find((t) => t.id === tool)?.name || "the tool"}</b> in that worktree and run:
                      <div className="spec-cmd mono"><span className="spec-prompt">$</span> specify "{newSpec.trim() || "…"}"</div>
                    </li>
                    <li>SpecKit generates <code>spec.md → plan.md → tasks.md</code>; tracked tasks populate the board live.</li>
                  </ol>
                </div>
              </>
            )}
          </FormSection>

          {/* 2 — Environment & agentic tool */}
          <FormSection n="2" title="Environment & agentic tool" sub="Where the agent runs — a backend type on one of your hosts. Tool availability depends on the environment." done={envOk && toolOk}>
            <div className="seg" style={{ marginBottom: 14 }}>
              <button aria-pressed={envMode === "fresh"} onClick={() => setEnvMode("fresh")}><Icon name="bolt" size={13} /> Fresh</button>
              <button aria-pressed={envMode === "existing"} onClick={() => setEnvMode("existing")}><Icon name="git" size={13} /> Pre-existing</button>
            </div>
            {envMode === "fresh" ? (
              <>
                <div className="env-cols">
                  <div className="field">
                    <label className="sp-label">Host</label>
                    <select className="input" value={host} onChange={(e) => setHost(e.target.value)}>
                      {HOSTS.map((h) => <option key={h.id} value={h.id} disabled={h.availability === "unavailable"}>{h.name} · {h.os}</option>)}
                    </select>
                  </div>
                  <div className="field">
                    <label className="sp-label">Backend type</label>
                    <div className="seg type-seg">
                      {TYPES.filter((t) => hostObj.supports.includes(t.id)).map((t) => (
                        <button key={t.id} aria-pressed={btype === t.id} onClick={() => setBtype(t.id)}><Icon name={t.icon} size={13} /> {t.name.replace("Canonical ", "").replace("NVIDIA ", "")}</button>
                      ))}
                    </div>
                  </div>
                </div>
                <p className="sp-note" style={{ marginTop: 12 }}><Icon name="bolt" size={13} /> Daedalus provisions a clean <b>{typeName(btype)}</b> environment on <b>{hostObj.name}</b> for this session.</p>
              </>
            ) : (
              <>
                <div className="env-list">
                  {existing.map((e) => (
                    <button key={e.id} className={`env-opt${env === e.id ? " on" : ""}`} onClick={() => setEnv(e.id)}>
                      <Icon name="git" size={14} />
                      <div style={{ flex: 1, textAlign: "left" }}><div className="eo-name">{e.name}</div><div className="eo-branch">{typeName(e.type)} · {hostName(e.host)} · branch {e.branch}</div></div>
                      {env === e.id && <Icon name="check" size={15} />}
                    </button>
                  ))}
                </div>
                <div className="banner banner-info" style={{ margin: "13px 0 0" }}>
                  <Icon name="shield" size={15} />
                  <div>Work happens in an <b>isolated git worktree</b> — your existing checkout and uncommitted work are never disturbed.</div>
                </div>
              </>
            )}
            <div className="sform-div" />
            <label className="sp-label">Agentic tool <span style={{ color: "var(--text-3)", fontWeight: 400 }}>· available on {envMode === "fresh" ? typeName(btype) : "this environment"}</span></label>
            <div className="tool-grid">
              {TOOLS.map((t) => (
                <button key={t.id} className={`tool-opt${tool === t.id ? " on" : ""}`} onClick={() => setTool(t.id)}>
                  <div className="to-top">
                    <span className="to-ic" style={t.tint ? { color: t.tint, background: `color-mix(in oklab, ${t.tint} 15%, transparent)` } : undefined}>{t.mono || t.name[0]}</span>
                    <span className="to-name">{t.name}</span><span className="chip mono">v{t.version}</span>
                  </div>
                  <div className="to-desc">{t.desc}</div>
                  <div className="to-caps">{t.speckit && <span className="chip speckit"><Icon name="check" size={11} /> SpecKit</span>}<span className={`chip ${t.interactive ? "okSoft" : ""}`}>{t.interactive ? "accepts input" : "headless"}</span></div>
                </button>
              ))}
            </div>
          </FormSection>
        </div>
      </div>

      <div className="start-footer">
        <span className="start-foot-hint">{valid ? <><Icon name="check" size={13} /> Ready to launch</> : "Complete tool, objective, and environment"}</span>
        <button className="btn primary" disabled={!valid} onClick={() => setReviewing(true)}>Review &amp; launch <Icon name="chevR" size={14} /></button>
      </div>

      {reviewing && <ConfirmLaunchModal summary={summary} onClose={() => setReviewing(false)} onLaunch={() => onLaunch("s-905")} />}
    </div>
  );
}

/* ───────────────────────── DISCOVER (now a section of Environments) ─────── */
const DISC_GROUPS = [
  { key: "local", label: "Local host", icon: "dot" },
  { key: "mdns", label: "mDNS-advertised hosts", icon: "globe" },
  { key: "tunnel", label: "Tunneled Workshops", icon: "tunnel" },
];
function DiscoverSection({ statusStyle, onOpen }) {
  const items = window.DATA.DISCOVERED;
  const [scanning, setScanning] = React.useState(false);
  return (
    <div className="disc-section">
      <div className="disc-section-head">
        <div>
          <GroupLabel>Discover sessions</GroupLabel>
          <div className="set-note" style={{ paddingTop: 0 }}><Icon name="info" size={12} /> Sessions running on this host, mDNS hosts, and tunneled Workshops — attach to any.</div>
        </div>
        <button className={`btn sm${scanning ? " tinted" : ""}`} onClick={() => { setScanning(true); setTimeout(() => setScanning(false), 1400); }}>
          <Icon name="refresh" size={13} className={scanning ? "stat-glyph spin" : ""} /> {scanning ? "Scanning…" : "Rescan"}
        </button>
      </div>
      {DISC_GROUPS.map((g) => {
        const list = items.filter((d) => d.group === g.key && !d.dupOf);
        return (
          <div key={g.key} className="disc-group">
            <div className="disc-grp-label"><Icon name={g.icon} size={12} /> {g.label} <span className="disc-grp-n">{list.length}</span></div>
            {list.length === 0 ? (
              <div className="disc-empty">
                <Icon name="info" size={13} /> {g.key === "mdns" ? "No hosts advertising on the local network." : "Nothing discovered here."}
              </div>
            ) : (
              <div className="grouped">
                {list.map((d) => (
                  <div key={d.id} className="row disc-row">
                    <StatusBadge status={d.status} style={statusStyle} />
                    <div style={{ flex: 1, minWidth: 0 }}>
                      <div className="disc-obj">{d.objective}</div>
                      <div className="disc-meta">{d.tool} · {d.host}</div>
                    </div>
                    {d.attachable ? (
                      <button className="btn sm tinted" onClick={() => onOpen("s-906")}><Icon name="bolt" size={12} /> Connect</button>
                    ) : (
                      <div className="disc-noattach" title={d.reason}><Icon name="warning" size={12} /> {d.status === "completed" ? "Review only" : "Not attachable"}</div>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>
        );
      })}
      <div className="disc-dedup"><Icon name="info" size={12} /> 1 session advertised from multiple sources was de-duplicated.</div>
    </div>
  );
}

/* ───────────────────────── TOOLS (list) ───────────────────────── */
const TOOL_TINTS = ["oklch(0.66 0.045 45)", "oklch(0.64 0.045 250)", "oklch(0.64 0.045 155)", "oklch(0.63 0.05 305)", "oklch(0.64 0.045 25)", "oklch(0.62 0.02 260)"];
function ToolRegisterModal({ onClose, onAdd }) {
  const [name, setName] = React.useState("");
  const [invoke, setInvoke] = React.useState("");
  const [version, setVersion] = React.useState("");
  const [interactive, setInteractive] = React.useState(true);
  const [tint, setTint] = React.useState(TOOL_TINTS[0]);
  const mono = (name.trim()[0] || "?").toUpperCase();
  const valid = name.trim() && invoke.trim();
  return (
    <div className="palette-scrim" onMouseDown={onClose}>
      <div className="modal" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h2>Register tool</h2>
          <button className="iconbtn" onClick={onClose}><Icon name="close" size={16} /></button>
        </div>
        <div className="modal-body">
          <div className="treg-preview">
            <div className="tool-icon" style={{ color: tint, background: `color-mix(in oklab, ${tint} 15%, transparent)`, width: 40, height: 40 }}>
              <span className="tool-mono" style={{ fontSize: 18 }}>{mono}</span>
            </div>
            <div>
              <div className="treg-prev-name">{name.trim() || "New tool"}</div>
              <div className="treg-prev-sub">SpecKit agent · {interactive ? "accepts interactive input" : "headless only"}{version.trim() ? ` · v${version.trim()}` : ""}</div>
            </div>
          </div>
          <div className="treg-note"><Icon name="info" size={13} /><span>Daedalus only orchestrates <b>SpecKit-compatible</b> agents. The tool is launched in the feature's worktree and runs the SpecKit implement pass — progress is read back from <code>tasks.md</code> and the session output.</span></div>
          <div className="env-cols">
            <div className="field" style={{ flex: 2 }}>
              <label className="sp-label">Name</label>
              <input className="input" placeholder="Claude Code" value={name} onChange={(e) => setName(e.target.value)} autoFocus />
            </div>
            <div className="field" style={{ flex: 1 }}>
              <label className="sp-label">Version</label>
              <input className="input" placeholder="1.0" value={version} onChange={(e) => setVersion(e.target.value)} />
            </div>
          </div>
          <div className="field">
            <label className="sp-label">Launch command <span style={{ color: "var(--text-3)", fontWeight: 400 }}>· run in the feature worktree</span></label>
            <input className="input" style={{ fontFamily: "var(--font-mono)", fontSize: 12 }} placeholder={'agy -p "/speckit.implement"'} value={invoke} onChange={(e) => setInvoke(e.target.value)} />
            <div className="treg-hint">The agent must implement the SpecKit workflow — e.g. <code>claude</code>, <code>agy</code>, <code>opencode</code>. It runs the implement pass over the generated spec.</div>
          </div>
          <div className="field">
            <label className="sp-label">Progress tracking</label>
            <div className="treg-track"><Icon name="info" size={13} /><span>Read from <code>tasks.md</code> + session output. No extra integration — Daedalus follows the file and the agent's own task announcements.</span></div>
          </div>
          <div className="field">
            <label className="sp-label">Icon color</label>
            <div className="treg-swatches">
              {TOOL_TINTS.map((c) => (
                <button key={c} className={`treg-sw${tint === c ? " on" : ""}`} style={{ background: c }} onClick={() => setTint(c)} aria-label="color">{tint === c && <Icon name="check" size={12} />}</button>
              ))}
            </div>
          </div>
          <div className="grouped">
            <div className="row"><div style={{ flex: 1 }}><div className="set-k">Accepts interactive input</div><div className="set-d">Operators can send input to the agent mid-run.</div></div>
              <button className="ios-toggle" data-on={interactive ? "1" : "0"} onClick={() => setInteractive((v) => !v)}><i /></button></div>
          </div>
        </div>
        <div className="modal-foot">
          <button className="btn ghost" onClick={onClose}>Cancel</button>
          <div className="tb-spacer" />
          <button className="btn primary" disabled={!valid} onClick={() => { onAdd({ id: "tool-" + Date.now(), name: name.trim(), invoke: invoke.trim(), version: version.trim() || "1.0", interactive, speckit: true, track: "tasks.md + output", mono, tint, desc: "Operator-registered SpecKit agent." }); onClose(); }}>
            <Icon name="plus" size={14} /> Register tool
          </button>
        </div>
      </div>
    </div>
  );
}

function Tools() {
  const [extra, setExtra] = React.useState([]);
  const [registering, setRegistering] = React.useState(false);
  const TOOLS = [...window.DATA.TOOLS, ...extra];
  return (
    <div className="screen">
      <ScreenHead title="Tools" sub="SpecKit-compatible agents. Daedalus launches each on a feature spec and tracks progress by watching its tasks.md and session output — no per-tool integration."
        action={<button className="btn primary" onClick={() => setRegistering(true)}><Icon name="plus" size={14} /> Register tool</button>} />
      <div className="screen-scroll">
        <div className="grouped">
          {TOOLS.map((t) => (
            <div key={t.id} className="row tool-row">
              <div className="tool-icon" style={t.tint ? { color: t.tint, background: `color-mix(in oklab, ${t.tint} 15%, transparent)` } : undefined}>
                {t.icon ? <Icon name={t.icon} size={17} /> : t.mono ? <span className="tool-mono">{t.mono}</span> : <Icon name="tools" size={17} />}
              </div>
              <div className="tool-main">
                <div className="tool-l1">
                  <span className="tool-name">{t.name}</span>
                  <span className="chip mono">v{t.version}</span>
                  {t.speckit && <span className="chip speckit"><Icon name="check" size={11} /> SpecKit</span>}
                  <span className={`chip ${t.interactive ? "okSoft" : ""}`}>{t.interactive ? "accepts input" : "headless"}</span>
                </div>
                <div className="tool-desc">{t.desc}</div>
                <div className="tool-invoke"><span className="ti-label">launch</span><code>{t.invoke}</code></div>
                {t.track && <div className="tool-track"><Icon name="info" size={11} /> progress tracked from <code>{t.track}</code></div>}
              </div>
              <button className="btn sm ghost">Edit</button>
            </div>
          ))}
        </div>
      </div>
      {registering && <ToolRegisterModal onClose={() => setRegistering(false)} onAdd={(tool) => setExtra((x) => [...x, tool])} />}
    </div>
  );
}

/* ───────────────────────── ENVIRONMENTS (hosts + backend types) ─────────── */
const HOST_KIND = { local: { icon: "host", label: "Local" }, tunnel: { icon: "tunnel", label: "Tunnel" }, mdns: { icon: "globe", label: "mDNS" } };

function CreateEnvModal({ onClose, onCreate, hostId }) {
  const HOSTS = window.DATA.HOSTS, TYPES = window.DATA.BACKEND_TYPES;
  const [host, setHost] = React.useState(hostId || "local");
  const hostObj = HOSTS.find((h) => h.id === host) || HOSTS[0];
  const supports = hostObj.supports;
  const [type, setType] = React.useState(supports[0]);
  React.useEffect(() => { if (!supports.includes(type)) setType(supports[0]); }, [host]);
  const [name, setName] = React.useState("");
  const typeObj = TYPES.find((t) => t.id === type) || TYPES[0];
  const direct = typeObj.kind === "direct";

  const create = () => {
    const finalName = name.trim() || (direct ? "host-env" : type + "-env");
    onCreate({ id: "env-" + Date.now(), name: finalName, host, type, kind: "created", fresh: true });
    onClose();
  };

  return (
    <div className="palette-scrim" onMouseDown={onClose}>
      <div className="modal" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h2>New environment</h2>
          <button className="iconbtn" onClick={onClose}><Icon name="close" size={16} /></button>
        </div>
        <div className="modal-body">
          <div className="field">
            <label className="sp-label">Host</label>
            <div className="backend-list">
              {HOSTS.map((h) => (
                <button key={h.id} className={`backend-opt${host === h.id ? " on" : ""}`} disabled={h.availability === "unavailable"} onClick={() => setHost(h.id)}>
                  <Icon name={HOST_KIND[h.kind].icon} size={18} />
                  <div style={{ flex: 1, textAlign: "left" }}>
                    <div className="bo-name">{h.name} <AvailDot state={h.availability} /></div>
                    <div className="bo-limit">{HOST_KIND[h.kind].label} · {h.os}</div>
                  </div>
                  {host === h.id && <Icon name="check" size={15} />}
                </button>
              ))}
            </div>
          </div>
          <div className="field">
            <label className="sp-label">Backend type <span style={{ color: "var(--text-3)", fontWeight: 400 }}>· available on {hostObj.name}</span></label>
            <div className="backend-list">
              {TYPES.filter((t) => supports.includes(t.id)).map((t) => (
                <button key={t.id} className={`backend-opt${type === t.id ? " on" : ""}`} onClick={() => setType(t.id)}>
                  <Icon name={t.icon} size={18} />
                  <div style={{ flex: 1, textAlign: "left" }}>
                    <div className="bo-name">{t.name} {t.kind === "sandbox" ? <span className="chip okSoft">sandbox</span> : <span className="chip warnsoft">direct</span>}</div>
                    <div className="bo-limit">{t.desc}</div>
                  </div>
                  {type === t.id && <Icon name="check" size={15} />}
                </button>
              ))}
            </div>
          </div>
          <div className="field">
            <label className="sp-label">Name <span style={{ color: "var(--text-3)", fontWeight: 400 }}>(optional)</span></label>
            <input className="input" placeholder="scratch-1" value={name} onChange={(e) => setName(e.target.value)} />
          </div>
          <div className={`banner ${direct ? "banner-warn" : "banner-ok"}`} style={{ margin: 0 }}>
            <Icon name={direct ? "warning" : "shield"} size={15} />
            <div>{direct
              ? <>Runs <b>directly on {hostObj.name}</b> — no isolation; shares the host filesystem and resources.</>
              : <>An <b>isolated {typeObj.name}</b> sandbox on <b>{hostObj.name}</b>, separated from the host and other environments.</>}</div>
          </div>
        </div>
        <div className="modal-foot">
          <button className="btn ghost" onClick={onClose}>Cancel</button>
          <div className="tb-spacer" />
          <button className="btn primary" onClick={create}><Icon name="plus" size={14} /> Create environment</button>
        </div>
      </div>
    </div>
  );
}

function Backends({ onOpenSessions, statusStyle, onOpen }) {
  const HOSTS = window.DATA.HOSTS, TYPES = window.DATA.BACKEND_TYPES, place = window.DATA.placement;
  const [extra, setExtra] = React.useState([]);
  const [creating, setCreating] = React.useState(false);
  const [createHost, setCreateHost] = React.useState(null);
  const typeName = (id) => (TYPES.find((t) => t.id === id) || {}).name || id;
  const typeIcon = (id) => (TYPES.find((t) => t.id === id) || {}).icon || "backends";
  const typeKind = (id) => (TYPES.find((t) => t.id === id) || {}).kind;
  const hostName = (id) => (HOSTS.find((h) => h.id === id) || {}).name || id;

  // existing seeded environments → derive host+type from their legacy backend
  const baseEnvs = window.DATA.ENVIRONMENTS.filter((e) => e.kind === "existing")
    .map((e) => ({ ...e, host: place(e.backend).host, type: place(e.backend).type }));
  const ENVS = [...baseEnvs, ...extra];

  const sessByHost = (hid) => window.DATA.SESSIONS.filter((s) => place(s.backend).host === hid);
  const sessByEnv = (id) => window.DATA.SESSIONS.filter((s) => s.env === id);
  const envsByHost = (hid) => ENVS.filter((e) => e.host === hid);

  const openCreate = (hid) => { setCreateHost(hid); setCreating(true); };

  return (
    <div className="screen">
      <ScreenHead title="Environments" sub="Hosts you can reach, and the environments running on them. Each environment is of a backend type — Workshop, OpenShell, macOS sandbox, or directly on host."
        action={<button className="btn primary" onClick={() => openCreate(null)}><Icon name="plus" size={14} /> New environment</button>} />
      <div className="screen-scroll">
        <GroupLabel>Hosts</GroupLabel>
        <div className="grouped">
          {HOSTS.map((h) => {
            const ses = sessByHost(h.id);
            const active = ses.filter((s) => isLive(s.status) || s.status === "stalled").length;
            const envs = envsByHost(h.id);
            const k = HOST_KIND[h.kind];
            return (
              <div key={h.id} className={`row backend-row avail-${h.availability}`}>
                <div className="brow-icon"><Icon name={k.icon} size={19} /></div>
                <div className="brow-main">
                  <div className="brow-l1">
                    <span className="brow-name">{h.name}</span>
                    <span className="chip">{k.label}</span>
                    <span className="brow-plat">{h.os}</span>
                    <span className={`chip ${h.availability === "available" ? "okSoft" : h.availability === "idle" ? "" : "warnsoft"}`}>
                      <AvailDot state={h.availability === "idle" ? "degraded" : h.availability} /> {h.availability}
                    </span>
                  </div>
                  <div className="host-supports">
                    {h.supports.map((tid) => (
                      <span key={tid} className="chip"><Icon name={typeIcon(tid)} size={11} /> {typeName(tid)}</span>
                    ))}
                  </div>
                  <div className="brow-meta"><Icon name="backends" size={11} /> {envs.length} environment{envs.length === 1 ? "" : "s"} · {ses.length} session{ses.length === 1 ? "" : "s"}{active > 0 && <span className="bcard-active"> · {active} active</span>}</div>
                </div>
                <div className="brow-right">
                  <button className="btn sm tinted" onClick={() => openCreate(h.id)}><Icon name="plus" size={12} /> New environment</button>
                  {ses.length > 0 && <button className="btn sm" onClick={() => onOpenSessions({ host: h.id })}>View sessions <Icon name="arrowR" size={12} /></button>}
                </div>
              </div>
            );
          })}
        </div>

        <div style={{ height: 22 }} />
        <GroupLabel right={<button className="glabel-add" onClick={() => openCreate(null)}><Icon name="plus" size={12} /> New</button>}>Environments</GroupLabel>
        <div className="grouped">
          {ENVS.map((e) => {
            const ses = sessByEnv(e.id);
            const direct = typeKind(e.type) === "direct";
            return (
              <button key={e.id} className="row env-row" onClick={() => onOpenSessions({ env: e.id })}>
                <Icon name={typeIcon(e.type)} size={15} style={{ color: "var(--text-2)" }} />
                <div style={{ flex: 1, textAlign: "left" }}>
                  <div style={{ fontSize: 13, fontWeight: 500, display: "flex", alignItems: "center", gap: 8 }}>{e.name}
                    {e.fresh && <span className="chip">new</span>}</div>
                  <div style={{ fontSize: 11.5, color: "var(--text-2)" }}>{typeName(e.type)} · {hostName(e.host)}{e.branch ? ` · branch ${e.branch}` : ""}</div>
                </div>
                {e.branch
                  ? <span className="chip okSoft"><Icon name="shield" size={11} /> worktree-isolated</span>
                  : direct
                    ? <span className="chip warnsoft"><Icon name="host" size={11} /> on host</span>
                    : <span className="chip okSoft"><Icon name="shield" size={11} /> sandboxed</span>}
                <span className="env-sess">{ses.length} session{ses.length === 1 ? "" : "s"}</span>
                <Icon name="chevR" size={14} style={{ color: "var(--text-3)" }} />
              </button>
            );
          })}
          <div className="row"><button className="btn sm tinted" onClick={() => openCreate(null)}><Icon name="plus" size={12} /> New environment</button></div>
        </div>

        <div style={{ height: 8 }} />
        <DiscoverSection statusStyle={statusStyle} onOpen={onOpen} />
        <div style={{ height: 24 }} />
      </div>
      {creating && <CreateEnvModal hostId={createHost} onClose={() => setCreating(false)} onCreate={(env) => setExtra((x) => [...x, env])} />}
    </div>
  );
}

/* ───────────────────────── SETTINGS ───────────────────────── */
function TunnelModal({ onClose, onAdd }) {
  const [label, setLabel] = React.useState("");
  const [endpoint, setEndpoint] = React.useState("");
  const valid = label.trim() && endpoint.trim();
  return (
    <div className="palette-scrim" onMouseDown={onClose}>
      <div className="modal" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h2>Add tunnel</h2>
          <button className="iconbtn" onClick={onClose}><Icon name="close" size={16} /></button>
        </div>
        <div className="modal-body">
          <div className="field">
            <label className="sp-label">Label</label>
            <input className="input" placeholder="eu-fra-1" value={label} onChange={(e) => setLabel(e.target.value)} autoFocus />
          </div>
          <div className="field">
            <label className="sp-label">Workshop endpoint</label>
            <input className="input" placeholder="wss://workshop.eu-fra-1.internal" value={endpoint} onChange={(e) => setEndpoint(e.target.value)} style={{ fontFamily: "var(--font-mono)", fontSize: 12 }} />
          </div>
          <div className="banner banner-ok" style={{ margin: 0 }}>
            <Icon name="shield" size={15} />
            <div><b>Outbound only.</b> Daedalus opens no inbound listener. The tunnel authenticates with your Workshop credentials from the system keychain — the token is never displayed or stored by Daedalus.</div>
          </div>
          <div className="field">
            <label className="sp-label">Once connected, this makes available</label>
            <div className="grouped">
              <div className="row"><Icon name="tunnel" size={14} style={{ color: "var(--text-2)" }} /><span className="set-k" style={{ flex: 1 }}>The remote Workshop as a backend</span></div>
              <div className="row"><Icon name="git" size={14} style={{ color: "var(--text-2)" }} /><span className="set-k" style={{ flex: 1 }}>Its environments — selectable when starting a session</span></div>
              <div className="row"><Icon name="discover" size={14} style={{ color: "var(--text-2)" }} /><span className="set-k" style={{ flex: 1 }}>Its running sessions — in Discover</span></div>
            </div>
          </div>
        </div>
        <div className="modal-foot">
          <button className="btn ghost" onClick={onClose}>Cancel</button>
          <div className="tb-spacer" />
          <button className="btn primary" disabled={!valid} onClick={() => { onAdd({ id: "tun-" + Date.now(), name: label.trim(), endpoint: endpoint.trim(), status: "active" }); onClose(); }}>
            <Icon name="tunnel" size={14} /> Add &amp; connect
          </button>
        </div>
      </div>
    </div>
  );
}

function SettingsModal({ tweaks, setTweak, onClose }) {
  const [conc, setConc] = React.useState(8);
  const [tunnels, setTunnels] = React.useState([
    { id: "tun-eu", name: "eu-fra-1", endpoint: "wss://workshop.eu-fra-1.internal", status: "active" },
    { id: "tun-us", name: "us-east-2", endpoint: "wss://workshop.us-east-2.internal", status: "idle" },
  ]);
  const [addingTunnel, setAddingTunnel] = React.useState(false);

  return (
    <div className="palette-scrim" onMouseDown={onClose}>
      <div className="modal wide" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h2>Settings</h2>
          <button className="iconbtn" onClick={onClose}><Icon name="close" size={16} /></button>
        </div>
        <div className="modal-body">
        <div className="banner banner-ok" style={{ margin: 0 }}>
          <Icon name="shield" size={18} />
          <div><b>Local-first.</b> Daedalus runs no open network listener by default — nothing is reachable from outside this host unless you explicitly enable a tunnel below. Secrets live only in your environments and are never displayed.</div>
        </div>

        <div>
        <GroupLabel right={<button className="glabel-add" onClick={() => setAddingTunnel(true)}><Icon name="plus" size={12} /> Add tunnel</button>}>Tunnels</GroupLabel>
        <div className="set-note"><Icon name="info" size={12} /> Each active tunnel adds a remote Workshop backend and makes its environments and sessions available — only while connected.</div>
        <div className="grouped">
          {tunnels.map((tn) => (
            <div key={tn.id} className="row">
              <div style={{ flex: 1 }}>
                <div className="set-k">{tn.name}</div>
                <div className="set-d" style={{ fontFamily: "var(--font-mono)", fontSize: 11 }}>{tn.endpoint}</div>
              </div>
              {tn.status === "active"
                ? <span className="chip okSoft"><AvailDot state="available" /> active</span>
                : <button className="btn sm" onClick={() => setTunnels((ts) => ts.map((x) => x.id === tn.id ? { ...x, status: "active" } : x))}>Connect</button>}
            </div>
          ))}
          <div className="row"><button className="btn sm tinted" onClick={() => setAddingTunnel(true)}><Icon name="plus" size={12} /> Add tunnel</button></div>
        </div>
        </div>

        <div>
        <GroupLabel>Concurrency</GroupLabel>
        <div className="grouped">
          <div className="row"><div style={{ flex: 1 }}><div className="set-k">Max concurrent sessions</div><div className="set-d">New sessions beyond this limit are rejected with a clear message.</div></div>
            <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
              <input type="range" min="1" max="16" value={conc} onChange={(e) => setConc(+e.target.value)} style={{ width: 130, accentColor: "var(--accent)" }} />
              <span className="mono" style={{ width: 18, textAlign: "right", fontWeight: 600 }}>{conc}</span>
            </div></div>
        </div>
        </div>

        <div>
        <GroupLabel>Notifications</GroupLabel>
        <div className="grouped">
          {[["Session stalled", true], ["Session failed", true], ["Session completed", true], ["Connection lost", true], ["Resource pressure", false]].map(([k, v]) => (
            <div key={k} className="row"><div style={{ flex: 1 }} className="set-k">{k}</div><FakeToggle on={v} /></div>
          ))}
        </div>
        </div>

        <div>
        <GroupLabel>Appearance</GroupLabel>
        <div className="grouped">
          <div className="row"><div style={{ flex: 1 }} className="set-k">Theme</div>
            <div className="seg">
              <button aria-pressed={tweaks.theme === "light"} onClick={() => setTweak("theme", "light")}><Icon name="sun" size={13} /> Light</button>
              <button aria-pressed={tweaks.theme === "dark"} onClick={() => setTweak("theme", "dark")}><Icon name="moon" size={13} /> Dark</button>
            </div></div>
          <div className="row"><div style={{ flex: 1 }} className="set-k">Platform skin</div>
            <div className="seg">
              <button aria-pressed={tweaks.skin === "mac"} onClick={() => setTweak({ skin: "mac", accent: "#E0901C" })}><Icon name="apple" size={13} /> macOS</button>
              <button aria-pressed={tweaks.skin === "yaru"} onClick={() => setTweak({ skin: "yaru", accent: "#E95420" })}><Icon name="linux" size={13} /> Linux</button>
            </div></div>
        </div>
        </div>
        </div>
      </div>
      {addingTunnel && <TunnelModal onClose={() => setAddingTunnel(false)} onAdd={(tn) => setTunnels((ts) => [...ts, tn])} />}
    </div>
  );
}
function FakeToggle({ on: initial }) {
  const [on, setOn] = React.useState(initial);
  return <button className="ios-toggle" data-on={on ? "1" : "0"} onClick={() => setOn(!on)}><i /></button>;
}

Object.assign(window, { StartSession, Tools, Backends, SettingsModal });
