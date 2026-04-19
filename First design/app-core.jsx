// app-core.jsx — The AI chat + writing workspace
// Theme tokens, chat pane, document pane, composer, and the Tweaks panel.
// Exports: AppWorkspace, ThemeProvider, useTheme, TweaksPanel

// ─── Design tokens ─────────────────────────────────────────────────────────
function makeTheme(t) {
  const { dark, accent, radius, density, bgTint, glow } = t;

  // Background: warm paper (light) / deep blue-violet near-black (dark)
  const bg = dark
    ? `oklch(${0.145 + bgTint * 0.02} ${0.006 + bgTint * 0.012} ${270 + bgTint * 20})`
    : `oklch(${0.985 - bgTint * 0.006} ${0.003 + bgTint * 0.002} ${70 - bgTint * 30})`;

  const surface = dark
    ? `oklch(${0.185 + bgTint * 0.015} ${0.008 + bgTint * 0.010} ${272 + bgTint * 18})`
    : `oklch(${0.995 - bgTint * 0.004} 0.002 ${70 - bgTint * 30})`;

  const surfaceRaised = dark
    ? `oklch(${0.215 + bgTint * 0.015} ${0.010 + bgTint * 0.010} ${275 + bgTint * 15})`
    : '#ffffff';

  const fg = dark ? 'oklch(0.96 0.005 270)' : 'oklch(0.18 0.008 60)';
  const fgMuted = dark ? 'oklch(0.72 0.01 270)' : 'oklch(0.48 0.01 60)';
  const fgDim = dark ? 'oklch(0.52 0.012 270)' : 'oklch(0.62 0.008 60)';

  // Hairline highlight (dark) / soft ink line (light)
  const line = dark
    ? 'rgba(255,255,255,0.055)'
    : 'rgba(40,30,20,0.07)';
  const lineStrong = dark
    ? 'rgba(255,255,255,0.10)'
    : 'rgba(40,30,20,0.12)';

  const accentHues = { terracotta: 38, violet: 290, cyan: 220, gold: 80, neutral: 70 };
  const accentChroma = { terracotta: 0.13, violet: 0.14, cyan: 0.11, gold: 0.11, neutral: 0.005 };
  const h = accentHues[accent] || 38;
  const c = accentChroma[accent] || 0.13;
  const accentColor = `oklch(${dark ? 0.72 : 0.62} ${c} ${h})`;
  const accentSoft = `oklch(${dark ? 0.72 : 0.62} ${c} ${h} / 0.18)`;
  const accentGlowCss = `0 0 ${20 + glow * 40}px -4px oklch(${dark ? 0.75 : 0.65} ${c} ${h} / ${0.3 + glow * 0.4})`;

  // Radii scale
  const r = {
    xs: Math.round(4 + radius * 3),
    sm: Math.round(8 + radius * 4),
    md: Math.round(12 + radius * 6),
    lg: Math.round(18 + radius * 8),
    xl: Math.round(24 + radius * 10),
    xxl: Math.round(30 + radius * 12),
  };

  const space = { tight: 0.75, balanced: 1, airy: 1.3 }[density] || 1;

  return {
    dark, accent, bg, surface, surfaceRaised, fg, fgMuted, fgDim,
    line, lineStrong, accentColor, accentSoft, accentGlow: accentGlowCss,
    r, space,
    // the 'highlight' — hairline top glow + bottom shade used to separate panes
    paneBorder: dark
      ? `inset 0 1px 0 rgba(255,255,255,0.06), inset 0 -1px 0 rgba(0,0,0,0.4), 0 1px 0 rgba(0,0,0,0.3)`
      : `inset 0 1px 0 rgba(255,255,255,0.9), 0 1px 2px rgba(40,30,20,0.04), 0 8px 24px -12px rgba(40,30,20,0.12)`,
  };
}

const ThemeContext = React.createContext(null);
function useTheme() { return React.useContext(ThemeContext); }

function ThemeProvider({ value, children }) {
  const theme = React.useMemo(() => makeTheme(value), [value]);
  return <ThemeContext.Provider value={theme}>{children}</ThemeContext.Provider>;
}

// ─── Sample content ────────────────────────────────────────────────────────
const SAMPLE_CONVO = [
  { role: 'user', text: 'Help me sharpen the opening of my essay on silence. It feels flat.' },
  { role: 'ai', text: "Share what you have and I'll read it first — I won't rewrite until I know what you're reaching for." },
  { role: 'user', text: '"Silence is underrated. We treat it like an empty room, but it\'s actually full of things we haven\'t let ourselves notice."', attachment: 'Essay — draft 3.md' },
  { role: 'ai', text: "The idea is good — the phrasing concedes too early. ‘Underrated’ asks the reader to agree before they've felt anything. Try leading with the image, then the claim. Draft on the right — three openings, each tuned differently.", suggestions: ['Tighten further', 'Make it funnier', 'Add a quieter version'] },
];

// ─── Icons (all hairline strokes) ──────────────────────────────────────────
const Icon = {
  spark: (c = 'currentColor') => (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
      <path d="M7 1v5M7 8v5M1 7h5M8 7h5" stroke={c} strokeWidth="1.2" strokeLinecap="round"/>
    </svg>
  ),
  send: (c = 'currentColor') => (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
      <path d="M2 7l10-5-3 12-2-5-5-2z" stroke={c} strokeWidth="1.2" strokeLinejoin="round" fill="none"/>
    </svg>
  ),
  sun: (c = 'currentColor') => (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
      <circle cx="7" cy="7" r="2.5" stroke={c} strokeWidth="1.2"/>
      <path d="M7 1v1.5M7 11.5V13M1 7h1.5M11.5 7H13M2.8 2.8l1 1M10.2 10.2l1 1M2.8 11.2l1-1M10.2 3.8l1-1" stroke={c} strokeWidth="1.2" strokeLinecap="round"/>
    </svg>
  ),
  moon: (c = 'currentColor') => (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none">
      <path d="M12 8.5A5 5 0 016 2a5 5 0 106 6.5z" stroke={c} strokeWidth="1.2" strokeLinejoin="round"/>
    </svg>
  ),
  plus: (c = 'currentColor') => (
    <svg width="12" height="12" viewBox="0 0 12 12" fill="none"><path d="M6 1.5v9M1.5 6h9" stroke={c} strokeWidth="1.2" strokeLinecap="round"/></svg>
  ),
  attach: (c = 'currentColor') => (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none"><path d="M9.5 5.5L5 10a2 2 0 11-3-3l5-5a3 3 0 014 4L6 11.5a1.5 1.5 0 01-2-2L8 5.5" stroke={c} strokeWidth="1.1" strokeLinecap="round" strokeLinejoin="round"/></svg>
  ),
  menu: (c = 'currentColor') => (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none"><path d="M2 4h10M2 7h10M2 10h7" stroke={c} strokeWidth="1.2" strokeLinecap="round"/></svg>
  ),
};

// ─── Chat message bubble ──────────────────────────────────────────────────
function Message({ m, compact = false }) {
  const t = useTheme();
  const isUser = m.role === 'user';
  return (
    <div style={{
      display: 'flex', flexDirection: 'column',
      alignItems: isUser ? 'flex-end' : 'flex-start',
      marginBottom: 18 * t.space,
    }}>
      {!isUser && (
        <div style={{
          display: 'flex', alignItems: 'center', gap: 8,
          marginBottom: 8, opacity: 0.7,
          fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
          fontSize: 10, letterSpacing: 0.8, textTransform: 'uppercase',
          color: t.fgMuted,
        }}>
          <div style={{
            width: 6, height: 6, borderRadius: '50%',
            background: t.accentColor,
            boxShadow: t.accentGlow,
          }} />
          Assistant
        </div>
      )}
      <div style={{
        maxWidth: compact ? '92%' : '78%',
        padding: compact ? `${10 * t.space}px ${14 * t.space}px` : `${14 * t.space}px ${18 * t.space}px`,
        borderRadius: t.r.lg,
        background: isUser ? t.surfaceRaised : 'transparent',
        boxShadow: isUser ? t.paneBorder : 'none',
        color: t.fg,
        fontSize: compact ? 14 : 15,
        lineHeight: 1.55,
        letterSpacing: -0.1,
        fontFamily: '"Inter Tight", "Inter", system-ui, sans-serif',
      }}>
        {m.text}
        {m.attachment && (
          <div style={{
            marginTop: 10, padding: '6px 10px',
            borderRadius: t.r.sm,
            background: t.dark ? 'rgba(255,255,255,0.04)' : 'rgba(40,30,20,0.04)',
            fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
            fontSize: 11, color: t.fgMuted,
            display: 'inline-flex', alignItems: 'center', gap: 6,
          }}>
            <span style={{ opacity: 0.5 }}>📎</span>{m.attachment}
          </div>
        )}
      </div>
      {m.suggestions && (
        <div style={{ display: 'flex', gap: 6, marginTop: 10, flexWrap: 'wrap', maxWidth: '78%' }}>
          {m.suggestions.map(s => (
            <button key={s} className="chip" style={{
              padding: '6px 12px', borderRadius: 9999,
              background: 'transparent',
              border: `1px solid ${t.line}`,
              color: t.fgMuted,
              fontSize: 12, cursor: 'pointer',
              fontFamily: 'inherit',
              transition: 'all 0.2s ease',
            }}>{s}</button>
          ))}
        </div>
      )}
    </div>
  );
}

// ─── Composer ─────────────────────────────────────────────────────────────
function Composer({ compact = false }) {
  const t = useTheme();
  const [v, setV] = React.useState('');
  const [focus, setFocus] = React.useState(false);
  return (
    <div style={{
      padding: compact ? 12 : `${16 * t.space}px ${20 * t.space}px ${20 * t.space}px`,
    }}>
      <div style={{
        borderRadius: t.r.lg,
        background: t.surfaceRaised,
        boxShadow: focus
          ? `${t.paneBorder}, 0 0 0 2px ${t.accentSoft}, ${t.accentGlow}`
          : t.paneBorder,
        transition: 'box-shadow 0.25s ease',
        padding: compact ? 10 : 14,
      }}>
        <textarea
          value={v}
          onChange={e => setV(e.target.value)}
          onFocus={() => setFocus(true)}
          onBlur={() => setFocus(false)}
          placeholder="Reply, or ask for another angle…"
          style={{
            width: '100%', background: 'transparent', border: 'none', outline: 'none',
            resize: 'none', color: t.fg, fontSize: compact ? 13 : 15,
            fontFamily: '"Inter Tight", "Inter", system-ui, sans-serif',
            lineHeight: 1.55, minHeight: compact ? 36 : 52,
            letterSpacing: -0.1,
            caretColor: t.accentColor,
          }}
        />
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginTop: 6 }}>
          <button style={iconBtnStyle(t)} title="Attach">{Icon.attach(t.fgMuted)}</button>
          <button style={iconBtnStyle(t)} title="Mention">{Icon.plus(t.fgMuted)}</button>
          <div style={{ flex: 1 }} />
          <div style={{
            fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
            fontSize: 10, color: t.fgDim, marginRight: 4, letterSpacing: 0.4,
          }}>⌘ ↵</div>
          <button style={{
            width: compact ? 30 : 36, height: compact ? 30 : 36,
            borderRadius: '50%',
            background: v ? t.accentColor : (t.dark ? 'rgba(255,255,255,0.06)' : 'rgba(40,30,20,0.05)'),
            border: 'none', cursor: 'pointer',
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            color: v ? '#fff' : t.fgDim,
            boxShadow: v ? t.accentGlow : 'none',
            transition: 'all 0.2s ease',
          }}>{Icon.send('currentColor')}</button>
        </div>
      </div>
    </div>
  );
}

function iconBtnStyle(t) {
  return {
    width: 28, height: 28, borderRadius: 8,
    background: 'transparent', border: 'none', cursor: 'pointer',
    display: 'flex', alignItems: 'center', justifyContent: 'center',
    color: t.fgMuted, transition: 'all 0.2s ease',
  };
}

// ─── Chat pane ────────────────────────────────────────────────────────────
function ChatPane({ compact = false, convo = SAMPLE_CONVO }) {
  const t = useTheme();
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', overflow: 'hidden' }}>
      {/* header */}
      <div style={{
        padding: compact ? '14px 16px 10px' : `${22 * t.space}px ${28 * t.space}px ${14 * t.space}px`,
        display: 'flex', alignItems: 'center', gap: 10,
      }}>
        <div style={{
          width: 8, height: 8, borderRadius: '50%',
          background: t.accentColor, boxShadow: t.accentGlow,
        }} />
        <div style={{
          fontFamily: '"Fraunces", "Iowan Old Style", Georgia, serif',
          fontSize: compact ? 16 : 19, fontWeight: 500, letterSpacing: -0.3,
          color: t.fg,
        }}>On silence</div>
        <div style={{ flex: 1 }} />
        <div style={{
          fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
          fontSize: 10, letterSpacing: 0.6, color: t.fgDim, textTransform: 'uppercase',
        }}>draft · 3</div>
      </div>
      {/* messages */}
      <div style={{
        flex: 1, overflowY: 'auto',
        padding: compact ? '8px 16px 4px' : `${8 * t.space}px ${28 * t.space}px ${4 * t.space}px`,
      }}>
        {convo.map((m, i) => <Message key={i} m={m} compact={compact} />)}
      </div>
      <Composer compact={compact} />
    </div>
  );
}

// ─── Document pane ────────────────────────────────────────────────────────
function DocumentPane({ compact = false }) {
  const t = useTheme();
  return (
    <div style={{
      flex: 1, minWidth: 0, height: '100%', display: 'flex', flexDirection: 'column',
      background: t.surface,
      borderRadius: t.r.xl, margin: compact ? 10 : 14,
      boxShadow: t.paneBorder, overflow: 'hidden',
      position: 'relative',
    }}>
      {/* glow accent at top-right corner */}
      <div aria-hidden style={{
        position: 'absolute', top: -40, right: -40,
        width: 180, height: 180, borderRadius: '50%',
        background: t.accentColor, opacity: t.dark ? 0.09 : 0.05,
        filter: 'blur(60px)', pointerEvents: 'none',
      }} />
      <div style={{
        padding: compact ? '12px 16px' : `${16 * t.space}px ${26 * t.space}px`,
        display: 'flex', alignItems: 'center', gap: 10,
        borderBottom: `0.5px solid ${t.line}`,
      }}>
        <div style={{
          fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
          fontSize: 10, color: t.fgMuted, letterSpacing: 0.8, textTransform: 'uppercase',
        }}>Canvas</div>
        <div style={{ flex: 1 }} />
        {['A', 'B', 'C'].map((l, i) => (
          <div key={l} style={{
            width: 22, height: 22, borderRadius: 6,
            background: i === 0 ? t.accentSoft : 'transparent',
            border: `1px solid ${i === 0 ? 'transparent' : t.line}`,
            color: i === 0 ? t.accentColor : t.fgDim,
            fontSize: 10, fontFamily: 'ui-monospace, monospace', fontWeight: 600,
            display: 'flex', alignItems: 'center', justifyContent: 'center',
            cursor: 'pointer',
          }}>{l}</div>
        ))}
      </div>
      <div style={{
        flex: 1, overflowY: 'auto',
        padding: compact ? '18px 22px' : `${30 * t.space}px ${44 * t.space}px ${40 * t.space}px`,
      }}>
        <div style={{
          fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
          fontSize: 10, letterSpacing: 0.8, color: t.fgDim, textTransform: 'uppercase',
          marginBottom: compact ? 10 : 18,
        }}>Opening — option A</div>
        <h1 style={{
          fontFamily: '"Fraunces", "Iowan Old Style", Georgia, serif',
          fontSize: compact ? 26 : 40,
          fontWeight: 400, letterSpacing: -0.8, lineHeight: 1.08,
          margin: 0, color: t.fg,
          textWrap: 'pretty',
        }}>
          <span style={{ fontStyle: 'italic', color: t.accentColor }}>Silence</span> is not an empty room.
        </h1>
        <p style={{
          marginTop: compact ? 14 : 24,
          fontFamily: '"Inter Tight", "Inter", system-ui, sans-serif',
          fontSize: compact ? 14 : 17, lineHeight: 1.65,
          color: t.fg, letterSpacing: -0.1,
          textWrap: 'pretty',
        }}>
          It's a room we've stopped seeing because nothing is moving. Step into one — the good kind, the kind that happens in a stairwell at 6 a.m. or between two friends who've run out of small things to say — and the room begins to fill. Your own breathing, for one. The shape of a thought you've been putting off. The low electric hum of the building.
        </p>
        <p style={{
          marginTop: compact ? 12 : 18,
          fontFamily: '"Inter Tight", "Inter", system-ui, sans-serif',
          fontSize: compact ? 14 : 17, lineHeight: 1.65,
          color: t.fgMuted, letterSpacing: -0.1,
        }}>
          We treat silence as the absence of signal. It is closer to the opposite: the moment you stop generating enough of your own noise to hear what was there all along.
        </p>

        {/* AI inline suggestion */}
        {!compact && (
          <div style={{
            marginTop: 32,
            padding: '14px 18px',
            borderRadius: t.r.md,
            background: t.dark ? 'rgba(255,255,255,0.025)' : 'rgba(40,30,20,0.025)',
            boxShadow: `inset 0 1px 0 ${t.dark ? 'rgba(255,255,255,0.04)' : 'rgba(255,255,255,0.9)'}`,
            display: 'flex', gap: 14, alignItems: 'flex-start',
          }}>
            <div style={{
              width: 6, height: 6, borderRadius: '50%', marginTop: 7,
              background: t.accentColor, boxShadow: t.accentGlow, flexShrink: 0,
            }} />
            <div>
              <div style={{
                fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
                fontSize: 10, letterSpacing: 0.8, color: t.fgMuted,
                textTransform: 'uppercase', marginBottom: 4,
              }}>inline suggestion</div>
              <div style={{
                fontSize: 13, lineHeight: 1.55, color: t.fg,
              }}>
                Consider pulling the stairwell image earlier — maybe into the first line. It's the most alive thing on the page.
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

// ─── Sidebar rail ─────────────────────────────────────────────────────────
function SideRail({ compact = false }) {
  const t = useTheme();
  const threads = [
    { t: 'On silence', active: true, meta: '3 drafts' },
    { t: 'Book notes — Berger', active: false, meta: '12 clips' },
    { t: 'Lab memo, Q2', active: false, meta: 'today' },
    { t: 'Letter to R.', active: false, meta: '2 drafts' },
  ];
  return (
    <div style={{
      width: compact ? 180 : 240, flexShrink: 0,
      padding: compact ? '14px 12px' : `${20 * t.space}px ${18 * t.space}px`,
      display: 'flex', flexDirection: 'column', gap: compact ? 14 : 22,
      borderRight: `0.5px solid ${t.line}`,
    }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
        <div style={{
          width: 22, height: 22, borderRadius: 7,
          background: `linear-gradient(135deg, ${t.accentColor}, oklch(0.5 0.14 290))`,
          boxShadow: t.accentGlow,
        }} />
        <div style={{
          fontFamily: '"Fraunces", Georgia, serif',
          fontSize: 15, letterSpacing: -0.2, color: t.fg, fontWeight: 500,
        }}>Quire</div>
      </div>

      <button style={{
        display: 'flex', alignItems: 'center', gap: 8,
        padding: '8px 12px', borderRadius: t.r.sm,
        background: 'transparent',
        border: `1px solid ${t.line}`,
        color: t.fgMuted, cursor: 'pointer',
        fontFamily: 'inherit', fontSize: 12,
        justifyContent: 'space-between',
      }}>
        <span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          {Icon.plus(t.fgMuted)} New thread
        </span>
        <span style={{
          fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
          fontSize: 10, color: t.fgDim,
        }}>⌘N</span>
      </button>

      <div>
        <div style={{
          fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
          fontSize: 10, letterSpacing: 0.8, color: t.fgDim,
          textTransform: 'uppercase', padding: '0 4px 8px',
        }}>Threads</div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
          {threads.map((th, i) => (
            <div key={i} style={{
              padding: '8px 10px', borderRadius: t.r.sm,
              background: th.active
                ? (t.dark ? 'rgba(255,255,255,0.04)' : 'rgba(40,30,20,0.035)')
                : 'transparent',
              boxShadow: th.active
                ? `inset 0 1px 0 ${t.dark ? 'rgba(255,255,255,0.05)' : 'rgba(255,255,255,0.8)'}`
                : 'none',
              cursor: 'pointer', display: 'flex', alignItems: 'center', gap: 8,
            }}>
              <div style={{
                width: 4, height: 4, borderRadius: '50%',
                background: th.active ? t.accentColor : t.fgDim,
                boxShadow: th.active ? t.accentGlow : 'none',
                flexShrink: 0,
              }} />
              <div style={{ flex: 1, minWidth: 0 }}>
                <div style={{
                  fontSize: 13, color: t.fg, letterSpacing: -0.1,
                  whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis',
                }}>{th.t}</div>
                <div style={{
                  fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
                  fontSize: 10, color: t.fgDim, marginTop: 1,
                }}>{th.meta}</div>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// ─── Workspace (desktop) ──────────────────────────────────────────────────
function AppWorkspace({ compact = false, showRail = true, showDoc = true, split = 0.42 }) {
  const t = useTheme();
  const [drag, setDrag] = React.useState(split);
  const ref = React.useRef(null);
  const onDown = (e) => {
    e.preventDefault();
    const rect = ref.current.getBoundingClientRect();
    const move = (ev) => {
      const x = (ev.clientX - rect.left) / rect.width;
      setDrag(Math.max(0.25, Math.min(0.65, x)));
    };
    const up = () => {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  };

  return (
    <div ref={ref} style={{
      width: '100%', height: '100%', display: 'flex',
      background: t.bg, color: t.fg, overflow: 'hidden',
      fontFamily: '"Inter Tight", "Inter", system-ui, sans-serif',
      transition: 'background 0.4s ease, color 0.3s ease',
    }}>
      {showRail && <SideRail compact={compact} />}
      <div style={{
        display: 'flex', flex: 1, minWidth: 0,
        flexBasis: `${drag * 100}%`,
        maxWidth: showDoc ? `${drag * 100}%` : '100%',
      }}>
        <div style={{ flex: 1, minWidth: 0, display: 'flex', flexDirection: 'column' }}>
          <ChatPane compact={compact} />
        </div>
      </div>
      {showDoc && (
        <>
          {/* drag handle */}
          <div onMouseDown={onDown} style={{
            width: 1, height: '100%',
            background: t.line,
            cursor: 'col-resize', flexShrink: 0,
            position: 'relative',
          }}>
            <div style={{
              position: 'absolute', top: '50%', left: -3, transform: 'translateY(-50%)',
              width: 7, height: 40, borderRadius: 4,
            }} />
          </div>
          <div style={{ flex: 1, minWidth: 0, display: 'flex' }}>
            <DocumentPane compact={compact} />
          </div>
        </>
      )}
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
      background: 'oklch(0.18 0.012 275 / 0.85)',
      backdropFilter: 'blur(20px) saturate(140%)',
      WebkitBackdropFilter: 'blur(20px) saturate(140%)',
      boxShadow: 'inset 0 1px 0 rgba(255,255,255,0.08), 0 20px 60px rgba(0,0,0,0.4)',
      color: '#f0f0f2',
      padding: 20,
      fontFamily: '"Inter Tight", "Inter", system-ui, sans-serif',
    }}>
      <div style={{
        display: 'flex', alignItems: 'center', gap: 8, marginBottom: 20,
      }}>
        <div style={{ width: 6, height: 6, borderRadius: '50%', background: accentSwatch[config.accent] }} />
        <div style={{ fontSize: 13, fontWeight: 500, letterSpacing: -0.1 }}>Tweaks</div>
        <div style={{ flex: 1 }} />
        <div style={{
          fontFamily: 'ui-monospace, "JetBrains Mono", monospace',
          fontSize: 10, color: 'rgba(255,255,255,0.4)',
        }}>live</div>
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
        <input type="range" min={0} max={1.5} step={0.05}
          value={config.radius} onChange={e => setConfig({ ...config, radius: parseFloat(e.target.value) })}
          style={sliderStyle} />
      </div>

      <div style={group}>
        <div style={label}><span>Glow</span><span style={{ color: 'rgba(255,255,255,0.3)' }}>{config.glow.toFixed(2)}</span></div>
        <input type="range" min={0} max={1} step={0.05}
          value={config.glow} onChange={e => setConfig({ ...config, glow: parseFloat(e.target.value) })}
          style={sliderStyle} />
      </div>

      <div style={group}>
        <div style={label}><span>Bg tint</span><span style={{ color: 'rgba(255,255,255,0.3)' }}>{config.bgTint.toFixed(2)}</span></div>
        <input type="range" min={0} max={1} step={0.05}
          value={config.bgTint} onChange={e => setConfig({ ...config, bgTint: parseFloat(e.target.value) })}
          style={sliderStyle} />
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

const sliderStyle = {
  width: '100%', accentColor: '#c96442', height: 4,
};

Object.assign(window, { AppWorkspace, ThemeProvider, useTheme, TweaksPanel });
