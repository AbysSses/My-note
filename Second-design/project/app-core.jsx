// app-core.jsx — Zero-Studio-inspired workspace, Quire aesthetic
// Three columns (icon rail + knowledge base + chat) + floating tasks panel.
// Exports: AppWorkspace, ThemeProvider, useTheme, TweaksPanel, Message, SAMPLE_CONVO

// ─── Design tokens ─────────────────────────────────────────────────────────
function makeTheme(t) {
  const { dark, accent, radius, density, bgTint, glow } = t;

  const bg = dark
    ? `oklch(${0.145 + bgTint * 0.02} ${0.006 + bgTint * 0.012} ${270 + bgTint * 20})`
    : `oklch(${0.985 - bgTint * 0.006} ${0.003 + bgTint * 0.002} ${70 - bgTint * 30})`;

  const surface = dark
    ? `oklch(${0.185 + bgTint * 0.015} ${0.008 + bgTint * 0.010} ${272 + bgTint * 18})`
    : `oklch(${0.995 - bgTint * 0.004} 0.002 ${70 - bgTint * 30})`;

  const surfaceRaised = dark
    ? `oklch(${0.215 + bgTint * 0.015} ${0.010 + bgTint * 0.010} ${275 + bgTint * 15})`
    : '#ffffff';

  const surfaceSunken = dark
    ? `oklch(${0.125 + bgTint * 0.01} ${0.007 + bgTint * 0.008} ${268 + bgTint * 20})`
    : `oklch(${0.97 - bgTint * 0.01} 0.003 ${70 - bgTint * 30})`;

  const fg = dark ? 'oklch(0.96 0.005 270)' : 'oklch(0.18 0.008 60)';
  const fgMuted = dark ? 'oklch(0.72 0.01 270)' : 'oklch(0.48 0.01 60)';
  const fgDim = dark ? 'oklch(0.52 0.012 270)' : 'oklch(0.62 0.008 60)';

  const line = dark ? 'rgba(255,255,255,0.055)' : 'rgba(40,30,20,0.07)';
  const lineStrong = dark ? 'rgba(255,255,255,0.10)' : 'rgba(40,30,20,0.12)';

  const accentHues = { terracotta: 38, violet: 290, cyan: 220, gold: 80, neutral: 70 };
  const accentChroma = { terracotta: 0.13, violet: 0.14, cyan: 0.11, gold: 0.11, neutral: 0.005 };
  const h = accentHues[accent] || 38;
  const c = accentChroma[accent] || 0.13;
  const accentColor = `oklch(${dark ? 0.72 : 0.62} ${c} ${h})`;
  const accentSoft = `oklch(${dark ? 0.72 : 0.62} ${c} ${h} / 0.18)`;
  const accentSofter = `oklch(${dark ? 0.72 : 0.62} ${c} ${h} / 0.08)`;
  const accentGlowCss = `0 0 ${20 + glow * 40}px -4px oklch(${dark ? 0.75 : 0.65} ${c} ${h} / ${0.3 + glow * 0.4})`;

  const r = {
    xs: Math.round(4 + radius * 3),
    sm: Math.round(8 + radius * 4),
    md: Math.round(12 + radius * 6),
    lg: Math.round(18 + radius * 8),
    xl: Math.round(24 + radius * 10),
    xxl: Math.round(30 + radius * 12),
  };

  const space = { tight: 0.75, balanced: 1, airy: 1.3 }[density] || 1;

  // Priority colors — used on the small triangle / dot markers
  const priority = {
    urgent: `oklch(${dark ? 0.7 : 0.58} 0.17 28)`,     // red-orange
    high: `oklch(${dark ? 0.75 : 0.62} 0.13 38)`,      // terracotta
    med: `oklch(${dark ? 0.78 : 0.68} 0.11 80)`,       // gold
    low: `oklch(${dark ? 0.68 : 0.58} 0.08 220)`,      // steel
    none: fgDim,
  };

  return {
    dark, accent, bg, surface, surfaceRaised, surfaceSunken, fg, fgMuted, fgDim,
    line, lineStrong, accentColor, accentSoft, accentSofter, accentGlow: accentGlowCss,
    r, space, priority,
    paneBorder: dark
      ? `inset 0 1px 0 rgba(255,255,255,0.06), inset 0 -1px 0 rgba(0,0,0,0.4), 0 1px 0 rgba(0,0,0,0.3)`
      : `inset 0 1px 0 rgba(255,255,255,0.9), 0 1px 2px rgba(40,30,20,0.04), 0 8px 24px -12px rgba(40,30,20,0.12)`,
    floatShadow: dark
      ? '0 24px 60px rgba(0,0,0,0.5), 0 0 0 0.5px rgba(255,255,255,0.08), inset 0 1px 0 rgba(255,255,255,0.06)'
      : '0 24px 60px rgba(40,30,20,0.14), 0 0 0 0.5px rgba(40,30,20,0.06), inset 0 1px 0 rgba(255,255,255,0.95)',
  };
}

const ThemeContext = React.createContext(null);
function useTheme() { return React.useContext(ThemeContext); }
function ThemeProvider({ value, children }) {
  const theme = React.useMemo(() => makeTheme(value), [value]);
  return <ThemeContext.Provider value={theme}>{children}</ThemeContext.Provider>;
}

// ─── Icons ────────────────────────────────────────────────────────────────
const Icon = {
  chat: (c) => <svg width="18" height="18" viewBox="0 0 18 18" fill="none"><path d="M3 5a2 2 0 012-2h8a2 2 0 012 2v5a2 2 0 01-2 2H8l-4 3v-3H5a2 2 0 01-2-2V5z" stroke={c} strokeWidth="1.3" strokeLinejoin="round"/></svg>,
  book: (c) => <svg width="18" height="18" viewBox="0 0 18 18" fill="none"><path d="M3 3h5a2 2 0 012 2v10a2 2 0 00-2-2H3V3zM15 3h-5a2 2 0 00-2 2v10a2 2 0 012-2h5V3z" stroke={c} strokeWidth="1.2" strokeLinejoin="round"/></svg>,
  tasks: (c) => <svg width="18" height="18" viewBox="0 0 18 18" fill="none"><path d="M4 5l2 2 4-4M4 11l2 2 4-4M13 5h2M13 11h2" stroke={c} strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  calendar: (c) => <svg width="18" height="18" viewBox="0 0 18 18" fill="none"><rect x="3" y="4" width="12" height="11" rx="2" stroke={c} strokeWidth="1.2"/><path d="M3 7h12M6 2v3M12 2v3" stroke={c} strokeWidth="1.2" strokeLinecap="round"/></svg>,
  scissors: (c) => <svg width="18" height="18" viewBox="0 0 18 18" fill="none"><circle cx="5" cy="5" r="2" stroke={c} strokeWidth="1.2"/><circle cx="5" cy="13" r="2" stroke={c} strokeWidth="1.2"/><path d="M7 6l8 7M7 12l8-7" stroke={c} strokeWidth="1.2" strokeLinecap="round"/></svg>,
  pin: (c) => <svg width="18" height="18" viewBox="0 0 18 18" fill="none"><path d="M9 2v5l3 3H6l3-3V2M9 10v5" stroke={c} strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  search: (c) => <svg width="18" height="18" viewBox="0 0 18 18" fill="none"><circle cx="8" cy="8" r="4.5" stroke={c} strokeWidth="1.3"/><path d="M11.5 11.5l3 3" stroke={c} strokeWidth="1.3" strokeLinecap="round"/></svg>,
  settings: (c) => <svg width="18" height="18" viewBox="0 0 18 18" fill="none"><circle cx="9" cy="9" r="2" stroke={c} strokeWidth="1.2"/><path d="M9 2v1.5M9 14.5V16M2 9h1.5M14.5 9H16M3.5 3.5l1 1M13.5 13.5l1 1M3.5 14.5l1-1M13.5 4.5l1-1" stroke={c} strokeWidth="1.2" strokeLinecap="round"/></svg>,
  refresh: (c) => <svg width="14" height="14" viewBox="0 0 14 14" fill="none"><path d="M12 7a5 5 0 11-1.5-3.5M12 2v3h-3" stroke={c} strokeWidth="1.2" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  send: (c) => <svg width="14" height="14" viewBox="0 0 14 14" fill="none"><path d="M2 7l10-5-3 12-2-5-5-2z" stroke={c} strokeWidth="1.2" strokeLinejoin="round" fill="none"/></svg>,
  attach: (c) => <svg width="14" height="14" viewBox="0 0 14 14" fill="none"><path d="M9.5 5.5L5 10a2 2 0 11-3-3l5-5a3 3 0 014 4L6 11.5a1.5 1.5 0 01-2-2L8 5.5" stroke={c} strokeWidth="1.1" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  plus: (c) => <svg width="12" height="12" viewBox="0 0 12 12" fill="none"><path d="M6 1.5v9M1.5 6h9" stroke={c} strokeWidth="1.2" strokeLinecap="round"/></svg>,
  x: (c) => <svg width="12" height="12" viewBox="0 0 12 12" fill="none"><path d="M2 2l8 8M10 2l-8 8" stroke={c} strokeWidth="1.3" strokeLinecap="round"/></svg>,
  check: (c) => <svg width="12" height="12" viewBox="0 0 12 12" fill="none"><path d="M2 6l3 3 5-6" stroke={c} strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  chevron: (c) => <svg width="10" height="10" viewBox="0 0 10 10" fill="none"><path d="M3 2l4 3-4 3" stroke={c} strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round"/></svg>,
  eye: (c) => <svg width="14" height="14" viewBox="0 0 14 14" fill="none"><path d="M1 7s2.5-4 6-4 6 4 6 4-2.5 4-6 4-6-4-6-4z" stroke={c} strokeWidth="1.2"/><circle cx="7" cy="7" r="1.5" stroke={c} strokeWidth="1.2"/></svg>,
  cal: (c) => <svg width="14" height="14" viewBox="0 0 14 14" fill="none"><rect x="2" y="3" width="10" height="9" rx="1.5" stroke={c} strokeWidth="1.1"/><path d="M2 6h10M5 1.5v2M9 1.5v2" stroke={c} strokeWidth="1.1" strokeLinecap="round"/></svg>,
  bell: (c) => <svg width="14" height="14" viewBox="0 0 14 14" fill="none"><path d="M3 10V7a4 4 0 018 0v3l1 1H2l1-1zM6 12a1 1 0 002 0" stroke={c} strokeWidth="1.1" strokeLinejoin="round"/></svg>,
  vault: (c) => <svg width="14" height="14" viewBox="0 0 14 14" fill="none"><rect x="2" y="2" width="10" height="10" rx="1.5" stroke={c} strokeWidth="1.1"/><circle cx="7" cy="7" r="2" stroke={c} strokeWidth="1.1"/><path d="M7 3v1M7 10v1M3 7h1M10 7h1" stroke={c} strokeWidth="1.1" strokeLinecap="round"/></svg>,
};

// Tiny colored triangle priority marker (like 🔺)
function PriorityMark({ level, size = 10 }) {
  const t = useTheme();
  const color = t.priority[level] || t.priority.none;
  if (level === 'none') return null;
  return (
    <svg width={size} height={size} viewBox="0 0 10 10" style={{ flexShrink: 0 }}>
      <path d="M5 1 L9 8 L1 8 Z" fill={color} />
    </svg>
  );
}

// ─── Icon rail ─────────────────────────────────────────────────────────────
function IconRail({ active = 'book' }) {
  const t = useTheme();
  const items = [
    { k: 'chat', icon: Icon.chat },
    { k: 'book', icon: Icon.book },
    { k: 'tasks', icon: Icon.tasks },
    { k: 'calendar', icon: Icon.calendar },
    { k: 'scissors', icon: Icon.scissors },
    { k: 'pin', icon: Icon.pin },
    { k: 'search', icon: Icon.search },
  ];
  return (
    <div style={{
      width: 56, flexShrink: 0, height: '100%',
      padding: '10px 0 14px',
      display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 4,
      borderRight: `0.5px solid ${t.line}`,
      background: t.surfaceSunken,
    }}>
      {/* brand dot */}
      <div style={{
        width: 30, height: 30, borderRadius: 9,
        background: `linear-gradient(135deg, ${t.accentColor}, oklch(0.5 0.14 290))`,
        boxShadow: t.accentGlow,
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        color: '#fff', fontFamily: '"Fraunces", serif', fontWeight: 500, fontSize: 14,
        marginBottom: 12,
      }}>Q</div>
      {items.map(it => {
        const isActive = it.k === active;
        return (
          <button key={it.k} style={{
            width: 36, height: 36, borderRadius: 10, border: 'none',
            background: isActive ? (t.dark ? 'rgba(255,255,255,0.06)' : 'rgba(40,30,20,0.05)') : 'transparent',
            boxShadow: isActive ? `inset 0 1px 0 ${t.dark ? 'rgba(255,255,255,0.06)' : 'rgba(255,255,255,0.9)'}` : 'none',
            cursor: 'pointer',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            position: 'relative',
          }}>
            {isActive && (
              <div style={{
                position: 'absolute', left: -11, top: 8, bottom: 8, width: 2,
                borderRadius: 2, background: t.accentColor,
                boxShadow: t.accentGlow,
              }} />
            )}
            {it.icon(isActive ? t.fg : t.fgMuted)}
          </button>
        );
      })}
      <div style={{ flex: 1 }} />
      <button style={{
        width: 36, height: 36, borderRadius: 10, border: 'none',
        background: 'transparent', cursor: 'pointer',
        display: 'flex', alignItems: 'center', justifyContent: 'center',
      }}>{Icon.settings(t.fgMuted)}</button>
    </div>
  );
}

// ─── Knowledge-base (tasks) column ────────────────────────────────────────
const SAMPLE_TASKS = {
  today: [
    { t: 'Narrow the opening of the silence essay', date: '04-18', p: 'urgent' },
    { t: 'Confirm quote permissions — Berger',      date: '04-18', p: 'urgent' },
    { t: 'Organize meeting notes — 04-17',           date: '04-18', p: 'urgent' },
    { t: 'Review draft 3 with inline suggestions',   date: '04-18', p: 'med' },
    { t: "Pull forward the stairwell image",         date: '04-18', p: 'med' },
    { t: 'Tighten closing paragraph',                date: '04-18', p: 'med' },
    { t: 'Prepare agenda — Q2 lab memo',              date: '04-18', p: 'med' },
  ],
  upcoming: [
    { t: 'Send essay draft to R. for read',          date: '04-21', p: 'high' },
    { t: 'Collect quotes — Sontag, Cage',             date: '04-22', p: 'high' },
    { t: "Merge daily notes into weekly",            date: '04-22', p: 'low' },
    { t: 'Write lab memo — first pass',               date: '04-23', p: 'low' },
    { t: 'Editing pass — letter to R.',               date: '04-24', p: 'low' },
    { t: 'Outline book-notes on Berger',              date: '04-25', p: 'low' },
    { t: 'Polish essay — final pass',                 date: '04-26', p: 'low' },
    { t: 'Publish to notes vault',                    date: '04-26', p: 'low' },
    { t: 'Archive old drafts',                        date: '04-27', p: 'none' },
  ],
};

function FilterChip({ label, count, active = false, tone }) {
  const t = useTheme();
  return (
    <button style={{
      display: 'inline-flex', alignItems: 'center', gap: 6,
      padding: '4px 10px', borderRadius: 9999,
      background: active ? (t.dark ? 'rgba(255,255,255,0.05)' : 'rgba(40,30,20,0.04)') : 'transparent',
      border: `1px solid ${active ? t.lineStrong : t.line}`,
      color: active ? t.fg : t.fgMuted,
      fontSize: 11, fontFamily: 'inherit', cursor: 'pointer',
      letterSpacing: -0.1,
    }}>
      {tone && <PriorityMark level={tone} size={8} />}
      {label}
      {count != null && (
        <span style={{
          fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
          fontSize: 10, color: t.fgDim, marginLeft: 2,
        }}>{count}</span>
      )}
    </button>
  );
}

function TaskRow({ task, compact = false }) {
  const t = useTheme();
  return (
    <div style={{
      display: 'flex', alignItems: 'center', gap: 10,
      padding: compact ? '7px 10px' : '8px 12px',
      borderRadius: t.r.sm,
      cursor: 'pointer',
    }}
      onMouseEnter={e => e.currentTarget.style.background = t.dark ? 'rgba(255,255,255,0.025)' : 'rgba(40,30,20,0.025)'}
      onMouseLeave={e => e.currentTarget.style.background = 'transparent'}
    >
      <div style={{
        width: 14, height: 14, borderRadius: 4,
        border: `1.2px solid ${t.lineStrong}`, flexShrink: 0,
      }} />
      <div style={{
        flex: 1, minWidth: 0,
        fontSize: 12.5, color: t.fg, letterSpacing: -0.1,
        whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis',
      }}>{task.t}</div>
      <div style={{
        fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
        fontSize: 10, color: t.fgDim, flexShrink: 0,
      }}>{task.date}</div>
      <PriorityMark level={task.p} />
    </div>
  );
}

function KnowledgeColumn() {
  const t = useTheme();
  return (
    <div style={{
      width: 300, flexShrink: 0, height: '100%',
      borderRight: `0.5px solid ${t.line}`,
      display: 'flex', flexDirection: 'column',
      background: t.bg,
    }}>
      {/* header */}
      <div style={{ padding: '14px 16px 6px', display: 'flex', alignItems: 'center', gap: 8 }}>
        <div style={{
          fontFamily: '"Fraunces", Georgia, serif',
          fontSize: 15, fontWeight: 500, color: t.fg, letterSpacing: -0.2,
        }}>Knowledge base</div>
        <div style={{ flex: 1 }} />
        <button style={{
          width: 24, height: 24, borderRadius: 7, border: 'none',
          background: 'transparent', cursor: 'pointer',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
        }}>{Icon.refresh(t.fgMuted)}</button>
      </div>

      {/* tabs */}
      <div style={{ padding: '4px 16px 10px', display: 'flex', gap: 6 }}>
        {[
          { l: 'Notes', active: false },
          { l: 'Tasks', active: true },
          { l: 'Projects', active: false },
        ].map(tab => (
          <div key={tab.l} style={{
            padding: '5px 12px', borderRadius: 9999,
            background: tab.active ? (t.dark ? 'rgba(255,255,255,0.06)' : 'rgba(40,30,20,0.05)') : 'transparent',
            boxShadow: tab.active ? `inset 0 1px 0 ${t.dark ? 'rgba(255,255,255,0.07)' : 'rgba(255,255,255,0.9)'}` : 'none',
            fontSize: 12, fontWeight: tab.active ? 500 : 400,
            color: tab.active ? t.fg : t.fgMuted,
            letterSpacing: -0.1,
          }}>{tab.l}</div>
        ))}
      </div>

      {/* priority filter — clean segmented bar */}
      <div style={{ padding: '2px 16px 14px' }}>
        <div style={{
          display: 'flex', alignItems: 'stretch',
          height: 30, borderRadius: 9,
          background: t.dark ? 'rgba(255,255,255,0.035)' : 'rgba(40,30,20,0.035)',
          boxShadow: `inset 0 1px 0 ${t.dark ? 'rgba(255,255,255,0.04)' : 'rgba(255,255,255,0.85)'}`,
          padding: 2,
          gap: 1,
        }}>
          {[
            { k: 'all', label: 'All', count: 16, dot: null, active: true },
            { k: 'urgent', label: 'Urgent', count: 3, dot: t.priority.urgent },
            { k: 'high', label: 'High', count: 4, dot: t.priority.high },
            { k: 'med', label: 'Med', count: 9, dot: t.priority.med },
            { k: 'low', label: 'Low', count: 0, dot: t.priority.low },
          ].map(p => (
            <button key={p.k} style={{
              flex: 1, minWidth: 0, padding: '0 6px',
              borderRadius: 7, border: 'none', cursor: 'pointer',
              background: p.active ? (t.dark ? 'rgba(255,255,255,0.06)' : '#ffffff') : 'transparent',
              boxShadow: p.active
                ? (t.dark
                    ? 'inset 0 1px 0 rgba(255,255,255,0.07), 0 1px 2px rgba(0,0,0,0.25)'
                    : 'inset 0 1px 0 rgba(255,255,255,1), 0 1px 2px rgba(40,30,20,0.08)')
                : 'none',
              color: p.active ? t.fg : t.fgMuted,
              display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 5,
              fontFamily: 'inherit', fontSize: 11, fontWeight: p.active ? 500 : 400,
              letterSpacing: -0.1,
              transition: 'background 0.15s ease',
            }}>
              {p.dot && (
                <span style={{
                  width: 5, height: 5, borderRadius: '50%',
                  background: p.dot, flexShrink: 0,
                }} />
              )}
              <span style={{ whiteSpace: 'nowrap' }}>{p.label}</span>
              <span style={{
                fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
                fontSize: 10, color: p.active ? t.fgMuted : t.fgDim,
                fontWeight: 400,
              }}>{p.count}</span>
            </button>
          ))}
        </div>
      </div>

      {/* list */}
      <div style={{ flex: 1, overflowY: 'auto', padding: '0 10px 16px' }}>
        <div style={{
          padding: '10px 6px 6px',
          fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
          fontSize: 10, letterSpacing: 0.8, color: t.fgMuted, textTransform: 'uppercase',
          display: 'flex', alignItems: 'center', gap: 6,
        }}>
          <div style={{ width: 3, height: 3, borderRadius: '50%', background: t.accentColor, boxShadow: t.accentGlow }} />
          Today · 7
        </div>
        {SAMPLE_TASKS.today.map((task, i) => <TaskRow key={i} task={task} />)}

        <div style={{
          padding: '14px 6px 6px',
          fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
          fontSize: 10, letterSpacing: 0.8, color: t.fgMuted, textTransform: 'uppercase',
        }}>Upcoming · 9</div>
        {SAMPLE_TASKS.upcoming.map((task, i) => <TaskRow key={i} task={task} />)}
      </div>
    </div>
  );
}

// ─── Chat — with attachment-cards above messages ──────────────────────────
const SAMPLE_CONVO = [
  {
    role: 'user',
    cards: [
      {
        kind: 'cal', title: '1 event',
        line1: 'Online meeting: discuss draft of the silence essay',
        line2: '04-18 · 16:00',
      },
      {
        kind: 'bell', title: '2 reminders',
        line1: "Today's meeting — silence essay review at 16:00",
        line2: 'Label: draft 3 · 30 min · online',
      },
      {
        kind: 'vault', title: 'Wrote to Vault',
        line1: 'Daily Notes/2026-04-18.md',
        line2: 'Notes/2026-04-18 (stairwell-2).md',
      },
    ],
    time: '15:07',
  },
  {
    role: 'user',
    text: "OK — please send me today's notes; I'll fold them into progress updates for the draft in Obsidian format.",
    time: '17:58',
  },
  {
    role: 'ai',
    text: 'You can just dictate like this and I\'ll keep it brief:',
    bullets: [
      { label: 'Project name', value: '' },
      { label: 'Current status', value: 'in progress / blocked / done' },
      { label: "What got done today", value: '' },
      { label: 'Next steps', value: '' },
      { label: 'Questions for me', value: '' },
    ],
    time: '17:58',
  },
];

function AttachCard({ card }) {
  const t = useTheme();
  const iconMap = { cal: Icon.cal, bell: Icon.bell, vault: Icon.vault };
  return (
    <div style={{
      padding: '12px 14px',
      borderRadius: t.r.md,
      background: t.surfaceRaised,
      boxShadow: t.paneBorder,
      display: 'flex', gap: 12, alignItems: 'flex-start',
    }}>
      <div style={{
        width: 28, height: 28, borderRadius: 9,
        background: t.accentSofter,
        color: t.accentColor,
        display: 'flex', alignItems: 'center', justifyContent: 'center',
        flexShrink: 0,
      }}>{iconMap[card.kind](t.accentColor)}</div>
      <div style={{ flex: 1, minWidth: 0 }}>
        <div style={{
          fontSize: 13, fontWeight: 500, color: t.fg,
          letterSpacing: -0.1, marginBottom: 2,
        }}>{card.title}</div>
        <div style={{ fontSize: 12, color: t.fgMuted, lineHeight: 1.45 }}>{card.line1}</div>
        <div style={{
          fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
          fontSize: 10.5, color: t.fgDim, marginTop: 2,
        }}>{card.line2}</div>
      </div>
    </div>
  );
}

function Message({ m, compact = false }) {
  const t = useTheme();
  const isUser = m.role === 'user';

  if (m.cards) {
    return (
      <div style={{
        display: 'flex', flexDirection: 'column',
        alignItems: 'flex-end', marginBottom: 18 * t.space, gap: 8,
      }}>
        {m.cards.map((c, i) => <div key={i} style={{ width: compact ? '92%' : '76%' }}><AttachCard card={c} /></div>)}
        {m.time && (
          <div style={{
            fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
            fontSize: 10, color: t.fgDim, marginTop: 2,
          }}>{m.time}</div>
        )}
      </div>
    );
  }

  return (
    <div style={{
      display: 'flex', flexDirection: 'column',
      alignItems: isUser ? 'flex-end' : 'flex-start',
      marginBottom: 18 * t.space,
    }}>
      {!isUser && (
        <div style={{
          display: 'flex', alignItems: 'center', gap: 8,
          marginBottom: 8, opacity: 0.75,
          fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
          fontSize: 10, letterSpacing: 0.8, textTransform: 'uppercase', color: t.fgMuted,
        }}>
          <div style={{ width: 6, height: 6, borderRadius: '50%', background: t.accentColor, boxShadow: t.accentGlow }} />
          Assistant
        </div>
      )}
      <div style={{
        maxWidth: compact ? '92%' : '78%',
        padding: compact ? '10px 14px' : '14px 18px',
        borderRadius: t.r.lg,
        background: isUser ? t.surfaceRaised : 'transparent',
        boxShadow: isUser ? t.paneBorder : 'none',
        color: t.fg,
        fontSize: compact ? 14 : 14.5,
        lineHeight: 1.55, letterSpacing: -0.1,
      }}>
        {m.text}
        {m.bullets && (
          <div style={{ marginTop: 12, display: 'flex', flexDirection: 'column', gap: 8 }}>
            {m.bullets.map((b, i) => (
              <div key={i} style={{ display: 'flex', alignItems: 'flex-start', gap: 10, lineHeight: 1.5 }}>
                <div style={{ width: 4, height: 4, borderRadius: '50%', background: t.accentColor, flexShrink: 0, marginTop: 8 }} />
                <div style={{ flex: 1, minWidth: 0 }}><strong style={{ fontWeight: 500 }}>{b.label}</strong>{b.value && <span style={{ color: t.fgMuted }}>: {b.value}</span>}</div>
              </div>
            ))}
          </div>
        )}
      </div>
      {m.time && (
        <div style={{
          fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
          fontSize: 10, color: t.fgDim, marginTop: 4,
        }}>{m.time}</div>
      )}
    </div>
  );
}

function Composer({ compact = false }) {
  const t = useTheme();
  const [v, setV] = React.useState('');
  const [focus, setFocus] = React.useState(false);
  return (
    <div style={{ padding: compact ? '8px 14px 14px' : '10px 26px 18px' }}>
      <div style={{
        borderRadius: t.r.lg,
        background: t.surfaceRaised,
        boxShadow: focus
          ? `${t.paneBorder}, 0 0 0 2px ${t.accentSoft}, ${t.accentGlow}`
          : t.paneBorder,
        transition: 'box-shadow 0.25s ease',
        padding: '10px 14px',
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <button style={iconBtn(t)}>{Icon.attach(t.fgMuted)}</button>
          <input
            value={v}
            onChange={e => setV(e.target.value)}
            onFocus={() => setFocus(true)}
            onBlur={() => setFocus(false)}
            placeholder="Describe your agenda, tasks, or project…"
            style={{
              flex: 1, background: 'transparent', border: 'none', outline: 'none',
              color: t.fg, fontSize: compact ? 13 : 14,
              fontFamily: 'inherit', letterSpacing: -0.1,
              caretColor: t.accentColor,
            }}
          />
          <button style={{
            width: 30, height: 30, borderRadius: '50%',
            background: v ? t.accentColor : (t.dark ? 'rgba(255,255,255,0.06)' : 'rgba(40,30,20,0.05)'),
            border: 'none', cursor: 'pointer',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            color: v ? '#fff' : t.fgDim,
            boxShadow: v ? t.accentGlow : 'none',
            transition: 'all 0.2s ease',
          }}>{Icon.send('currentColor')}</button>
        </div>
      </div>
      <div style={{
        fontSize: 11, color: t.fgDim, textAlign: 'center', marginTop: 10,
        letterSpacing: -0.1,
      }}>AI automatically organizes content into notes, tasks, and events in your Vault.</div>
    </div>
  );
}

function iconBtn(t) {
  return {
    width: 28, height: 28, borderRadius: 8,
    background: 'transparent', border: 'none', cursor: 'pointer',
    display: 'flex', alignItems: 'center', justifyContent: 'center',
    color: t.fgMuted,
  };
}

function ChatColumn() {
  const t = useTheme();
  return (
    <div style={{ flex: 1, minWidth: 0, height: '100%', display: 'flex', flexDirection: 'column' }}>
      {/* header */}
      <div style={{
        padding: '14px 26px 10px',
        display: 'flex', flexDirection: 'row', alignItems: 'center', gap: 8,
        flexShrink: 0, whiteSpace: 'nowrap',
      }}>
        {Icon.chat(t.fgMuted)}
        <span style={{
          fontSize: 13, color: t.fg, fontWeight: 500, letterSpacing: -0.1,
        }}>AI Chat</span>
        <div style={{ flex: 1 }} />
      </div>
      <div style={{ height: 0.5, background: t.line, margin: '0 26px' }} />

      <div style={{ flex: 1, overflowY: 'auto', padding: '14px 26px 8px' }}>
        {SAMPLE_CONVO.map((m, i) => <Message key={i} m={m} />)}
      </div>
      <Composer />
    </div>
  );
}

// ─── Floating Today Tasks panel ──────────────────────────────────────────
const TODAY_TASKS = [
  {
    title: 'Confirm online meeting time — silence essay review',
    time: '09:00', label: 'today', tone: 'urgent',
    ref: 'Daily Notes/2026-04-18.md',
    note: 'Confirm online meeting time with R. ▲',
  },
  {
    title: 'Confirm participants for the review meeting',
    time: '09:30', label: 'today', tone: 'urgent',
    ref: 'Daily Notes/2026-04-18.md',
    note: 'Check meeting attendees list',
  },
  {
    title: 'Organize the essay writing question list',
    time: '11:00', label: 'today', tone: 'med',
    ref: 'Daily Notes/2026-04-18.md',
  },
];

function TodayTasksPanel({ visible = true, onClose }) {
  const t = useTheme();
  if (!visible) return null;

  return (
    <div style={{
      position: 'absolute', top: 18, right: 18, zIndex: 50,
      width: 340,
      borderRadius: t.r.xl,
      background: t.dark ? 'oklch(0.2 0.012 275 / 0.92)' : 'rgba(255,255,255,0.95)',
      backdropFilter: 'blur(24px) saturate(160%)',
      WebkitBackdropFilter: 'blur(24px) saturate(160%)',
      boxShadow: t.floatShadow,
      color: t.fg,
      overflow: 'hidden',
    }}>
      {/* glow blob in corner */}
      <div aria-hidden style={{
        position: 'absolute', top: -60, right: -60,
        width: 200, height: 200, borderRadius: '50%',
        background: t.accentColor, opacity: t.dark ? 0.10 : 0.06,
        filter: 'blur(70px)', pointerEvents: 'none',
      }} />

      {/* header */}
      <div style={{
        padding: '16px 18px 10px',
        display: 'flex', alignItems: 'flex-start', gap: 10,
        position: 'relative',
      }}>
        <div style={{ flex: 1 }}>
          <div style={{
            fontFamily: '"Fraunces", Georgia, serif',
            fontSize: 16, fontWeight: 500, color: t.fg, letterSpacing: -0.2,
          }}>Today's tasks</div>
          <div style={{
            fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
            fontSize: 10.5, color: t.fgMuted, marginTop: 2,
            letterSpacing: 0.3,
          }}>7 to handle</div>
        </div>
        <button onClick={onClose} style={{
          width: 24, height: 24, borderRadius: 7, border: 'none',
          background: 'transparent', cursor: 'pointer',
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          color: t.fgMuted,
        }}>{Icon.x(t.fgMuted)}</button>
      </div>

      <div style={{
        padding: '0 18px 8px',
        fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
        fontSize: 10, color: t.fgDim, textTransform: 'uppercase', letterSpacing: 0.8,
      }}>Today</div>

      {/* tasks */}
      <div style={{ padding: '0 10px 8px' }}>
        {TODAY_TASKS.map((task, i) => (
          <div key={i} style={{
            padding: '10px 12px',
            borderRadius: t.r.md,
            display: 'flex', gap: 10, alignItems: 'flex-start',
            position: 'relative',
          }}
            onMouseEnter={e => e.currentTarget.style.background = t.dark ? 'rgba(255,255,255,0.025)' : 'rgba(40,30,20,0.025)'}
            onMouseLeave={e => e.currentTarget.style.background = 'transparent'}
          >
            <div style={{
              width: 26, height: 26, borderRadius: 8,
              background: t.accentSofter, color: t.accentColor,
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              flexShrink: 0, marginTop: 2,
            }}>{Icon.cal(t.accentColor)}</div>
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8 }}>
                <div style={{
                  flex: 1, minWidth: 0,
                  fontSize: 12.5, fontWeight: 500, color: t.fg,
                  letterSpacing: -0.1, lineHeight: 1.35,
                }}>{task.title}</div>
                <button style={{
                  padding: '3px 9px', borderRadius: 9999,
                  border: `1px solid ${t.dark ? 'rgba(100,220,140,0.25)' : 'rgba(30,140,70,0.2)'}`,
                  background: 'transparent',
                  color: `oklch(${t.dark ? 0.78 : 0.55} 0.13 145)`,
                  fontSize: 10, fontWeight: 500, fontFamily: 'inherit', cursor: 'pointer',
                  display: 'inline-flex', alignItems: 'center', gap: 4,
                  flexShrink: 0, whiteSpace: 'nowrap',
                }}>
                  {Icon.check('currentColor')} Done
                </button>
              </div>
              <div style={{
                display: 'flex', alignItems: 'center', gap: 8, marginTop: 5, flexWrap: 'wrap',
              }}>
                <span style={{ fontFamily: 'ui-monospace, "JetBrains Mono", monospace', fontSize: 10, color: t.fgMuted }}>{task.time}</span>
                <span style={{ fontSize: 10, color: t.fgDim }}>{task.label}</span>
                <span style={{
                  fontSize: 9.5, padding: '1px 6px', borderRadius: 4,
                  background: `oklch(${t.dark ? 0.7 : 0.58} 0.17 28 / 0.15)`,
                  color: t.priority[task.tone],
                  fontWeight: 500, letterSpacing: 0.2, textTransform: 'uppercase',
                }}>{task.tone}</span>
              </div>
              <div style={{
                fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
                fontSize: 10, color: t.fgDim, marginTop: 5,
                whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis',
              }}>{task.ref}</div>
              {task.note && (
                <div style={{
                  fontSize: 11, color: t.fgMuted, marginTop: 3,
                  whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis',
                }}>{task.note}</div>
              )}
            </div>
          </div>
        ))}
      </div>

      <div style={{
        padding: '10px 18px',
        borderTop: `0.5px solid ${t.line}`,
        display: 'flex', alignItems: 'center', gap: 10,
      }}>
        <div style={{ fontSize: 11.5, color: t.fgMuted, flex: 1 }}>Other tasks</div>
        <div style={{
          fontFamily: 'ui-monospace, monospace', fontSize: 11, color: t.fgDim,
        }}>4</div>
      </div>
      <button style={{
        width: '100%', padding: '12px 18px',
        background: t.dark ? 'rgba(255,255,255,0.03)' : 'rgba(40,30,20,0.025)',
        border: 'none', borderTop: `0.5px solid ${t.line}`,
        color: t.fgMuted, fontSize: 12, fontFamily: 'inherit',
        cursor: 'pointer',
        display: 'flex', alignItems: 'center', justifyContent: 'center', gap: 6,
        letterSpacing: -0.1,
      }}>
        View all tasks {Icon.chevron(t.fgMuted)}
      </button>
    </div>
  );
}

// ─── Workspace ────────────────────────────────────────────────────────────
function AppWorkspace() {
  const t = useTheme();

  return (
    <div style={{
      width: '100%', height: '100%', display: 'flex',
      background: t.bg, color: t.fg, overflow: 'hidden',
      fontFamily: '"Inter Tight", "Inter", system-ui, sans-serif',
      position: 'relative',
      transition: 'background 0.4s ease, color 0.3s ease',
    }}>
      <IconRail active="book" />
      <KnowledgeColumn />
      <ChatColumn />
    </div>
  );
}

// ─── Tweaks panel ─────────────────────────────────────────────────────────
function TweaksPanel({ config, setConfig, visible }) {
  if (!visible) return null;
  const accents = ['terracotta', 'violet', 'cyan', 'gold', 'neutral'];
  const accentSwatch = {
    terracotta: '#c96442', violet: '#7c6aef', cyan: '#4aa3c7',
    gold: '#d4b86a', neutral: '#b8b8ba',
  };

  const group = { marginBottom: 18 };
  const label = {
    fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
    fontSize: 10, letterSpacing: 0.8, textTransform: 'uppercase',
    color: 'rgba(255,255,255,0.5)', marginBottom: 8,
    display: 'flex', justifyContent: 'space-between',
  };
  const row = { display: 'flex', gap: 6, alignItems: 'center' };

  return (
    <div style={{
      position: 'fixed', bottom: 24, right: 24, zIndex: 9999,
      width: 280, borderRadius: 24,
      background: 'oklch(0.18 0.012 275 / 0.88)',
      backdropFilter: 'blur(20px) saturate(140%)',
      WebkitBackdropFilter: 'blur(20px) saturate(140%)',
      boxShadow: 'inset 0 1px 0 rgba(255,255,255,0.08), 0 20px 60px rgba(0,0,0,0.4)',
      color: '#f0f0f2', padding: 20,
      fontFamily: '"Inter Tight", "Inter", system-ui, sans-serif',
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 20 }}>
        <div style={{ width: 6, height: 6, borderRadius: '50%', background: accentSwatch[config.accent] }} />
        <div style={{ fontSize: 13, fontWeight: 500, letterSpacing: -0.1 }}>Tweaks</div>
        <div style={{ flex: 1 }} />
        <div style={{ fontFamily: 'ui-monospace, monospace', fontSize: 10, color: 'rgba(255,255,255,0.4)' }}>live</div>
      </div>

      <div style={group}>
        <div style={label}><span>Mode</span><span style={{ color: 'rgba(255,255,255,0.3)' }}>{config.dark ? 'dark' : 'light'}</span></div>
        <div style={row}>
          {[{ k: false, l: 'Light' }, { k: true, l: 'Dark' }].map(o => (
            <button key={String(o.k)} onClick={() => setConfig({ ...config, dark: o.k })} style={segBtn(config.dark === o.k)}>{o.l}</button>
          ))}
        </div>
      </div>

      <div style={group}>
        <div style={label}><span>Accent</span><span style={{ color: 'rgba(255,255,255,0.3)' }}>{config.accent}</span></div>
        <div style={row}>
          {accents.map(a => (
            <button key={a} onClick={() => setConfig({ ...config, accent: a })} style={{
              width: 32, height: 32, borderRadius: 10, cursor: 'pointer',
              background: 'transparent',
              border: config.accent === a ? `1px solid rgba(255,255,255,0.4)` : `1px solid rgba(255,255,255,0.08)`,
              display: 'flex', alignItems: 'center', justifyContent: 'center',
              boxShadow: config.accent === a ? `0 0 16px -4px ${accentSwatch[a]}` : 'none',
            }}>
              <div style={{ width: 14, height: 14, borderRadius: '50%', background: accentSwatch[a] }} />
            </button>
          ))}
        </div>
      </div>

      <div style={group}>
        <div style={label}><span>Radius</span><span style={{ color: 'rgba(255,255,255,0.3)' }}>{config.radius.toFixed(2)}</span></div>
        <input type="range" min={0} max={1.5} step={0.05} value={config.radius}
          onChange={e => setConfig({ ...config, radius: parseFloat(e.target.value) })} style={sliderStyle} />
      </div>

      <div style={group}>
        <div style={label}><span>Glow</span><span style={{ color: 'rgba(255,255,255,0.3)' }}>{config.glow.toFixed(2)}</span></div>
        <input type="range" min={0} max={1} step={0.05} value={config.glow}
          onChange={e => setConfig({ ...config, glow: parseFloat(e.target.value) })} style={sliderStyle} />
      </div>

      <div style={group}>
        <div style={label}><span>Bg tint</span><span style={{ color: 'rgba(255,255,255,0.3)' }}>{config.bgTint.toFixed(2)}</span></div>
        <input type="range" min={0} max={1} step={0.05} value={config.bgTint}
          onChange={e => setConfig({ ...config, bgTint: parseFloat(e.target.value) })} style={sliderStyle} />
      </div>

      <div style={group}>
        <div style={label}><span>Density</span></div>
        <div style={row}>
          {['tight', 'balanced', 'airy'].map(d => (
            <button key={d} onClick={() => setConfig({ ...config, density: d })} style={segBtn(config.density === d)}>{d}</button>
          ))}
        </div>
      </div>
    </div>
  );
}

function segBtn(active) {
  return {
    flex: 1, padding: '8px 10px', borderRadius: 10,
    background: active ? 'rgba(255,255,255,0.09)' : 'transparent',
    color: active ? '#fff' : 'rgba(255,255,255,0.55)',
    border: `1px solid ${active ? 'rgba(255,255,255,0.14)' : 'rgba(255,255,255,0.06)'}`,
    boxShadow: active ? 'inset 0 1px 0 rgba(255,255,255,0.08)' : 'none',
    fontSize: 11, letterSpacing: 0.2, textTransform: 'capitalize',
    fontFamily: 'inherit', cursor: 'pointer',
  };
}

const sliderStyle = { width: '100%', accentColor: '#c96442', height: 4 };

Object.assign(window, { AppWorkspace, ThemeProvider, useTheme, TweaksPanel, Message, SAMPLE_CONVO });
