/**
 * Minimal template engine for MyNotes.
 *
 * Supported placeholders (all inside `{{...}}`):
 *   {{now}}            → "YYYY-MM-DD HH:mm"
 *   {{date}}           → "YYYY-MM-DD"
 *   {{time}}           → "HH:mm"
 *   {{year}}           → "YYYY"
 *   {{week}}           → ISO week string, e.g. "2026-W16"
 *   {{date:FORMAT}}    → formatted date with tokens below
 *   {{<name>}}         → looked up in the context object; unknown keys are kept verbatim
 *
 * Date format tokens (longest-first matching):
 *   YYYY  4-digit year
 *   MM    zero-padded month (01..12)
 *   DD    zero-padded day (01..31)
 *   HH    zero-padded 24h hour
 *   mm    zero-padded minute
 *   ss    zero-padded second
 *   ddd   short weekday in zh-CN ("周一".."周日")
 */

const WEEKDAYS_ZH = ['周日', '周一', '周二', '周三', '周四', '周五', '周六'];

function pad2(n: number): string {
  return String(n).padStart(2, '0');
}

export function formatDate(d: Date, fmt: string): string {
  // Tokenized replace — non-token text is kept as-is.
  return fmt.replace(/YYYY|MM|DD|HH|mm|ss|ddd/g, (token) => {
    switch (token) {
      case 'YYYY':
        return String(d.getFullYear());
      case 'MM':
        return pad2(d.getMonth() + 1);
      case 'DD':
        return pad2(d.getDate());
      case 'HH':
        return pad2(d.getHours());
      case 'mm':
        return pad2(d.getMinutes());
      case 'ss':
        return pad2(d.getSeconds());
      case 'ddd':
        return WEEKDAYS_ZH[d.getDay()];
      default:
        return token;
    }
  });
}

/** ISO-8601 week number + owning year. */
export function isoWeek(d: Date): { year: number; week: number } {
  const date = new Date(Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()));
  const dayNum = date.getUTCDay() || 7; // Sunday=0 -> 7
  date.setUTCDate(date.getUTCDate() + 4 - dayNum); // jump to this ISO week's Thursday
  const yearStart = new Date(Date.UTC(date.getUTCFullYear(), 0, 1));
  const weekNum = Math.ceil(((date.getTime() - yearStart.getTime()) / 86_400_000 + 1) / 7);
  return { year: date.getUTCFullYear(), week: weekNum };
}

export function isoWeekString(d: Date): string {
  const { year, week } = isoWeek(d);
  return `${year}-W${pad2(week)}`;
}

export type RenderContext = Record<string, string | undefined>;

/** Render a template string with the given context + base date. */
export function render(tpl: string, ctx: RenderContext, date: Date = new Date()): string {
  return tpl.replace(/\{\{(.+?)\}\}/g, (match, body: string) => {
    const key = body.trim();
    if (key.startsWith('date:')) return formatDate(date, key.slice('date:'.length));
    switch (key) {
      case 'now':
        return formatDate(date, 'YYYY-MM-DD HH:mm');
      case 'date':
        return formatDate(date, 'YYYY-MM-DD');
      case 'time':
        return formatDate(date, 'HH:mm');
      case 'year':
        return formatDate(date, 'YYYY');
      case 'week':
        return isoWeekString(date);
    }
    const val = ctx[key];
    return val === undefined ? match : val;
  });
}
