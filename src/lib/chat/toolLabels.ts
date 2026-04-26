export function toolTraceLabel(name: string, argsRaw: string): string {
  const args = safeParse(argsRaw);
  const relPath = readString(args, ['rel_path', 'target_rel_path']);
  const tag = readString(args, ['tag']);

  switch (name) {
    case 'read_note':
      return relPath ? `AI 正在读取 ${relPath}…` : 'AI 正在读取笔记…';
    case 'search_by_tag':
      return tag ? `AI 正在搜索 #${tag}…` : 'AI 正在按标签搜索…';
    case 'search_fulltext':
      return 'AI 正在全文搜索…';
    case 'list_tags':
      return 'AI 正在列出标签…';
    case 'get_related_notes':
      return relPath ? `AI 正在寻找 ${relPath} 的相关笔记…` : 'AI 正在寻找相关笔记…';
    case 'propose_summary':
      return relPath ? `AI 正在起草 ${relPath} 的摘要…` : 'AI 正在起草摘要…';
    case 'propose_tag_update':
      return relPath ? `AI 正在起草 ${relPath} 的标签更新…` : 'AI 正在起草标签更新…';
    case 'propose_moc':
      return tag ? `AI 正在起草 #${tag} 的 MOC…` : 'AI 正在起草 MOC…';
    case 'propose_note_edit':
      return relPath ? `AI 正在修改 ${relPath}…` : 'AI 正在起草修改…';
    case 'delete_note':
      return relPath ? `AI 正在准备删除 ${relPath}…` : 'AI 正在准备删除提案…';
    case 'rename_note':
      return relPath ? `AI 正在准备重命名 ${relPath}…` : 'AI 正在准备重命名提案…';
    default:
      return `AI 正在调用 ${name}…`;
  }
}

function safeParse(raw: string): Record<string, unknown> | null {
  try {
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed === 'object' && parsed !== null && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>;
    }
  } catch {
    /* noop */
  }
  return null;
}

function readString(
  value: Record<string, unknown> | null,
  keys: string[]
): string | null {
  if (!value) return null;
  for (const key of keys) {
    const candidate = value[key];
    if (typeof candidate === 'string' && candidate.trim()) {
      return candidate.trim();
    }
  }
  return null;
}
