// windows-frame.jsx — Minimal Windows 11 window chrome (Mica-ish)
// Exports: WinWindow

function WinWindow({ width = 900, height = 600, title = 'App', dark = true, children }) {
  const bg = dark ? '#1f1b24' : '#f3f1f7';
  const bar = dark ? 'rgba(255,255,255,0.04)' : 'rgba(0,0,0,0.03)';
  const text = dark ? 'rgba(255,255,255,0.88)' : 'rgba(0,0,0,0.82)';
  const dim = dark ? 'rgba(255,255,255,0.55)' : 'rgba(0,0,0,0.55)';
  const line = dark ? 'rgba(255,255,255,0.06)' : 'rgba(0,0,0,0.06)';

  const ctrlIcon = (path) => (
    <div style={{
      width: 46, height: 32, display: 'flex', alignItems: 'center', justifyContent: 'center',
    }}>
      <svg width="10" height="10" viewBox="0 0 10 10" fill="none">{path}</svg>
    </div>
  );

  return (
    <div style={{
      width, height, borderRadius: 8, overflow: 'hidden',
      background: bg,
      boxShadow: '0 0 0 1px rgba(0,0,0,0.4), 0 30px 80px rgba(0,0,0,0.5)',
      display: 'flex', flexDirection: 'column',
      fontFamily: '"Segoe UI Variable", "Segoe UI", system-ui, sans-serif',
    }}>
      {/* titlebar */}
      <div style={{
        height: 32, display: 'flex', alignItems: 'center',
        background: bar, borderBottom: `0.5px solid ${line}`,
        flexShrink: 0,
      }}>
        <div style={{
          flex: 1, padding: '0 12px',
          fontSize: 12, color: dim, letterSpacing: 0.1,
        }}>{title}</div>
        <div style={{ display: 'flex', color: text }}>
          {ctrlIcon(<path d="M1 5h8" stroke="currentColor" strokeWidth="1"/>)}
          {ctrlIcon(<rect x="1.5" y="1.5" width="7" height="7" stroke="currentColor" strokeWidth="1" fill="none"/>)}
          {ctrlIcon(<path d="M1.5 1.5l7 7M8.5 1.5l-7 7" stroke="currentColor" strokeWidth="1"/>)}
        </div>
      </div>
      <div style={{ flex: 1, overflow: 'hidden', display: 'flex' }}>{children}</div>
    </div>
  );
}

Object.assign(window, { WinWindow });
