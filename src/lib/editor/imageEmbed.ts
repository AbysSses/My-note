/**
 * Image embed support — paste, drop, thumbnail preview.
 *
 * Two responsibilities bundled together because they share state (the
 * attachment blob-URL cache):
 *
 * 1. Paste / drop handler: when the user pastes an image from the clipboard
 *    or drops image files into the editor, save them via `attachment_save`
 *    and insert a `![alt](rel_path)` at the cursor.
 *
 * 2. Thumbnail preview: for any line whose *entire* content is
 *    `![alt](attachments/…)`, append a block-level widget below that line
 *    showing the image. The source markdown stays visible and editable —
 *    we never replace the line, only add a block below.
 *
 * Design_V2 §6.12 for the full rationale.
 */

import { Decoration, type DecorationSet, EditorView, WidgetType } from '@codemirror/view';
import { type EditorState, StateField, type Transaction } from '@codemirror/state';

import {
  attachmentReadBytes,
  attachmentReadExternalBytes,
  attachmentSave
} from '$lib/ipc/attachment';

// ---------------------------------------------------------------------------
// Blob URL cache — shared between paste insertion and widget rendering.
//
// Keyed by rel_path. On eviction we revoke the URL to free memory. We size
// it liberally because the lifetime of each URL is short (only live while
// the editor shows that image); a typical note has <20 images.

const BLOB_CACHE = new Map<string, string>();

async function getBlobUrl(pathLike: string): Promise<string> {
  const cached = BLOB_CACHE.get(pathLike);
  if (cached) return cached;
  // Three shapes we accept:
  //   attachments/...        → vault-scoped IPC
  //   /abs/path/foo.png      → external IPC
  //   file:///abs/path/...   → strip scheme → external IPC
  const normalized = pathLike.replace(/\\/g, '/');
  let bytes: Uint8Array;
  if (normalized.startsWith('attachments/')) {
    bytes = await attachmentReadBytes(normalized);
  } else if (normalized.startsWith('file://')) {
    bytes = await attachmentReadExternalBytes(decodeFileUri(normalized));
  } else if (normalized.startsWith('/')) {
    bytes = await attachmentReadExternalBytes(normalized);
  } else {
    throw new Error(`[imageEmbed] unsupported image path: ${pathLike}`);
  }
  const mime = mimeFromPath(normalized);
  // Blob wants an `ArrayBuffer`, while our IPC layer exposes `Uint8Array`.
  // `Uint8Array.from` also normalizes away `SharedArrayBuffer`-typed views in
  // DOM libdefs, which keeps `pnpm check` happy.
  const blobBytes = Uint8Array.from(bytes).buffer;
  const url = URL.createObjectURL(new Blob([blobBytes], { type: mime }));
  BLOB_CACHE.set(pathLike, url);
  return url;
}

/** `file:///Users/hcyang/%E5%9B%BE.png` → `/Users/hcyang/图.png`. */
function decodeFileUri(uri: string): string {
  // Strip `file://` (with or without extra slash). macOS / Linux file URIs look
  // like `file:///Users/…` — three slashes. We want what follows the host.
  const noScheme = uri.replace(/^file:\/\//i, '');
  try {
    return decodeURIComponent(noScheme);
  } catch {
    return noScheme;
  }
}

/** Caller should invoke this when a note is closed/switched. */
export function revokeAllAttachmentBlobs(): void {
  for (const url of BLOB_CACHE.values()) {
    try {
      URL.revokeObjectURL(url);
    } catch {
      /* ignore */
    }
  }
  BLOB_CACHE.clear();
}

function mimeFromPath(p: string): string {
  const m = p.toLowerCase().match(/\.([a-z0-9]+)$/);
  const ext = m?.[1] ?? '';
  switch (ext) {
    case 'png':
      return 'image/png';
    case 'jpg':
    case 'jpeg':
      return 'image/jpeg';
    case 'gif':
      return 'image/gif';
    case 'webp':
      return 'image/webp';
    case 'svg':
      return 'image/svg+xml';
    case 'bmp':
      return 'image/bmp';
    case 'avif':
      return 'image/avif';
    default:
      return 'application/octet-stream';
  }
}

// ---------------------------------------------------------------------------
// Line-regex: matches lines whose *entire* trimmed content is
// `![alt](<path>)`. Leading/trailing whitespace allowed. Accepted paths:
//   - vault-relative under `attachments/`
//   - POSIX absolute (`/Users/...`) — user typed it by hand
//   - Windows absolute (`C:\Users\...`, `D:/photos/x.png`) — Finder-equivalent
//     drag-drop on Windows or a user hand-typing a drive-letter path
//   - `file://` URI — e.g. pasted from WeChat / Finder
// Remote URLs (`http(s)://`) are intentionally skipped so no surprise network
// fetches happen inside the editor.
//
// The Windows branch accepts **both** forward- and back-slash separators.
// `scanEmbedLines` / `getBlobUrl` normalise `\` → `/` before any downstream
// use, so the rest of the pipeline stays POSIX-flavoured regardless of what
// the user typed.

const EMBED_LINE_RE =
  /^\s*!\[([^\]]*)\]\((attachments\/[^)]+|file:\/\/[^)]+|[A-Za-z]:[\\/][^)]+|\/[^)]+)\)\s*$/;

/**
 * Normalise a captured path so downstream consumers (`getBlobUrl`, blob-URL
 * cache keys) see a single canonical form. For POSIX / vault-relative /
 * file:// shapes this is a no-op; for Windows drive-letter paths it just
 * swaps backslashes to forward slashes (`C:\Users\x.png` → `C:/Users/x.png`).
 * Exported for unit tests.
 */
export function normalizeAbsPath(raw: string): string {
  // Windows drive-letter path — replace `\` with `/` so the blob cache and
  // external IPC see one canonical spelling regardless of how the user
  // typed it. We deliberately don't touch `file://` URIs (the `%XX` percent
  // encoding must stay byte-accurate) or paths under `attachments/` (never
  // contain `\` on any platform in a well-formed vault).
  if (/^[A-Za-z]:[\\/]/.test(raw)) {
    return raw.replace(/\\/g, '/');
  }
  return raw;
}

/**
 * Given a doc, return [line-end-position, rel_path, alt] tuples for every
 * standalone attachment image.
 */
function scanEmbedLines(
  state: EditorState
): Array<{ lineEnd: number; relPath: string; alt: string }> {
  const out: Array<{ lineEnd: number; relPath: string; alt: string }> = [];
  const doc = state.doc;
  for (let i = 1; i <= doc.lines; i++) {
    const line = doc.line(i);
    const m = EMBED_LINE_RE.exec(line.text);
    if (m) {
      out.push({
        lineEnd: line.to,
        relPath: normalizeAbsPath(m[2]),
        alt: m[1]
      });
    }
  }
  return out;
}

// ---------------------------------------------------------------------------
// Widget

class ImagePreviewWidget extends WidgetType {
  constructor(
    readonly relPath: string,
    readonly alt: string
  ) {
    super();
  }

  toDOM(view: EditorView): HTMLElement {
    const wrap = document.createElement('div');
    wrap.className = 'cm-md-image-preview';

    const img = document.createElement('img');
    img.alt = this.alt;
    img.decoding = 'async';
    img.loading = 'lazy';
    img.setAttribute('data-attachment', this.relPath);
    // Lightweight placeholder until the blob URL resolves.
    img.setAttribute(
      'style',
      [
        'display: block',
        'max-width: 520px',
        'max-height: 360px',
        'width: auto',
        'height: auto',
        'object-fit: contain',
        'border-radius: 6px',
        'box-shadow: 0 2px 12px rgba(0,0,0,0.06)',
        'margin: 8px 0',
        'background: var(--color-bg-subtle, #f3f1ec)'
      ].join(';')
    );
    wrap.appendChild(img);

    // Async src resolution. If the attachment is missing or unreadable we
    // swap in an error chip instead of leaving a broken <img>.
    getBlobUrl(this.relPath)
      .then((url) => {
        img.src = url;
        img.addEventListener(
          'load',
          () => {
            // The widget's height changes after the bitmap loads. Ask CM to
            // remeasure so click→position mapping below the image stays
            // aligned with the rendered layout.
            view.requestMeasure();
          },
          { once: true }
        );
      })
      .catch((err) => {
        console.warn('[imageEmbed] read failed', this.relPath, err);
        const errBox = document.createElement('div');
        errBox.className = 'cm-md-image-error';
        errBox.textContent = `⚠ 无法加载图片: ${this.relPath}`;
        wrap.replaceChild(errBox, img);
        view.requestMeasure();
      });

    return wrap;
  }

  eq(other: ImagePreviewWidget): boolean {
    return other.relPath === this.relPath && other.alt === this.alt;
  }

  ignoreEvent(): boolean {
    return true;
  }
}

// ---------------------------------------------------------------------------
// StateField — block widgets MUST come from a StateField, not a ViewPlugin
// (CM6 constraint: plugins can only emit span-level decorations).

function buildEmbedDecorations(state: EditorState): DecorationSet {
  const embeds = scanEmbedLines(state);
  if (embeds.length === 0) return Decoration.none;
  return Decoration.set(
    embeds.map(({ lineEnd, relPath, alt }) =>
      Decoration.widget({
        widget: new ImagePreviewWidget(relPath, alt),
        block: true,
        side: 1 // place after the line
      }).range(lineEnd)
    ),
    true
  );
}

export const imageEmbedField = StateField.define<DecorationSet>({
  create(state) {
    return buildEmbedDecorations(state);
  },
  update(value, tr: Transaction) {
    // Recompute only when doc changed — selection moves don't affect widgets.
    if (tr.docChanged) {
      return buildEmbedDecorations(tr.state);
    }
    return value;
  },
  provide: (f) => EditorView.decorations.from(f)
});

// ---------------------------------------------------------------------------
// Paste + drop DOM event handlers.
//
// We accept any MIME that starts with `image/` (PNG from clipboard is almost
// always image/png; drag from Finder yields the real filetype). We DO NOT
// handle non-image drops in Phase 2 Task 3 — dropping a .md/.pdf should be
// ignored, not silently filed into attachments.

async function saveImageFile(file: File): Promise<{ relPath: string; altFromName: string }> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  const ext = extFromFile(file);
  const original = file.name && !file.name.startsWith('image.') ? file.name : null;
  const relPath = await attachmentSave(bytes, original, ext);
  const altFromName = stripExt(file.name || '') || '';
  return { relPath, altFromName };
}

/**
 * Fallback path for clipboards / drops that carry a file reference as text
 * instead of a DataTransfer File. Reads the bytes via IPC, writes them into
 * `attachments/` via `attachment_save`, and returns the same shape
 * `saveImageFile` does so the insert path can be uniform.
 *
 * Accepts either an absolute POSIX path or a `file://` URI. Returns null for
 * anything else (non-image extension, network URL, relative path) so the
 * caller can skip silently.
 */
async function saveImageByPath(
  pathOrUri: string
): Promise<{ relPath: string; altFromName: string } | null> {
  const trimmed = pathOrUri.trim();
  if (!trimmed) return null;
  let absPath: string;
  if (/^file:\/\//i.test(trimmed)) {
    absPath = decodeFileUri(trimmed);
  } else if (trimmed.startsWith('/')) {
    absPath = trimmed;
  } else {
    return null;
  }
  const extMatch = absPath.toLowerCase().match(/\.([a-z0-9]+)$/);
  const ext = extMatch?.[1];
  if (!ext || !IMAGE_EXTS.has(ext)) return null;

  try {
    const bytes = await attachmentReadExternalBytes(absPath);
    const name = absPath.split('/').pop() || 'image';
    const original = name && !name.startsWith('image.') ? name : null;
    const relPath = await attachmentSave(bytes, original, ext);
    const altFromName = stripExt(name) || '';
    return { relPath, altFromName };
  } catch (err) {
    console.warn('[imageEmbed] saveImageByPath failed', absPath, err);
    return null;
  }
}

const IMAGE_EXTS = new Set([
  'png',
  'jpg',
  'jpeg',
  'gif',
  'webp',
  'svg',
  'bmp',
  'avif',
  'heic',
  'heif'
]);

/** Parse a `text/uri-list` clipboard / drag payload. Skips comments per RFC
 *  2483; returns entries in order. */
function parseUriList(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l) => l && !l.startsWith('#'));
}

function extFromFile(file: File): string {
  // Trust file.name first (drag-drop case); fall back to MIME (clipboard case).
  const m = file.name?.toLowerCase().match(/\.([a-z0-9]+)$/);
  if (m) return m[1];
  const t = file.type.toLowerCase();
  if (t.startsWith('image/')) return t.slice('image/'.length).replace('jpeg', 'jpg');
  return 'bin';
}

function stripExt(name: string): string {
  const i = name.lastIndexOf('.');
  return i > 0 ? name.slice(0, i) : name;
}

function escapeMdAlt(s: string): string {
  return s.replace(/[\[\]\\]/g, '\\$&');
}

/** Insert `![alt](rel)` at the current selection. Splits across multiple
 *  files with newlines between. */
function insertEmbedMarkdown(
  view: EditorView,
  entries: Array<{ relPath: string; altFromName: string }>
): void {
  if (entries.length === 0) return;
  const md = entries.map((e) => `![${escapeMdAlt(e.altFromName)}](${e.relPath})`).join('\n');
  // `changeByRange` returns a full transaction spec ({changes, range}) per
  // range — spread it into `update` rather than nesting it under `changes`.
  const tr = view.state.update(
    view.state.changeByRange((range) => ({
      changes: { from: range.from, to: range.to, insert: md + '\n' },
      range: range
    })),
    { scrollIntoView: true }
  );
  view.dispatch(tr);
}

export const attachmentPasteDrop = EditorView.domEventHandlers({
  paste(e, view) {
    const cd = e.clipboardData;
    if (!cd) return false;
    // Gather image Files (Chromium exposes them on `files`, Safari / some
    // cases on `items`). Both are DataTransferItemList-ish.
    const files: File[] = [];
    for (const f of Array.from(cd.files)) {
      if (f.type.startsWith('image/')) files.push(f);
    }
    if (files.length === 0) {
      for (const item of Array.from(cd.items)) {
        if (item.kind === 'file' && item.type.startsWith('image/')) {
          const f = item.getAsFile();
          if (f) files.push(f);
        }
      }
    }

    // Fallback: some sources (WeChat desktop, Finder via some code paths) put
    // only a file reference on the clipboard, as `text/uri-list` or plain
    // text — no `image/*` MIME at all. We resolve those through IPC.
    const pathCandidates: string[] = [];
    if (files.length === 0) {
      const uriList = cd.getData('text/uri-list');
      if (uriList) pathCandidates.push(...parseUriList(uriList));
      const plain = cd.getData('text/plain');
      if (plain && pathCandidates.length === 0) {
        // Trim to the first line — multi-line plain-text isn't a path list.
        const first = plain.split(/\r?\n/)[0].trim();
        if (first) pathCandidates.push(first);
      }
    }

    if (files.length === 0 && pathCandidates.length === 0) {
      return false; // let CM handle text paste
    }

    // If we only have text candidates that aren't image paths, bail out so
    // the user's plain-text paste still lands. We check synchronously to
    // avoid swallowing innocuous text pastes.
    if (files.length === 0 && !pathCandidates.some(looksLikeImagePath)) {
      return false;
    }

    e.preventDefault();
    (async () => {
      const entries: Array<{ relPath: string; altFromName: string }> = [];
      for (const f of files) {
        try {
          entries.push(await saveImageFile(f));
        } catch (err) {
          console.warn('[imageEmbed] paste save failed', err);
        }
      }
      for (const p of pathCandidates) {
        const res = await saveImageByPath(p);
        if (res) entries.push(res);
      }
      insertEmbedMarkdown(view, entries);
    })();
    return true;
  },

  dragover(e, _view) {
    // Must preventDefault to opt into being a drop target; otherwise the
    // browser treats the drop as navigation.
    if (e.dataTransfer && Array.from(e.dataTransfer.types).includes('Files')) {
      e.preventDefault();
      return true;
    }
    return false;
  },

  drop(e, view) {
    const dt = e.dataTransfer;
    if (!dt) return false;
    const files = Array.from(dt.files).filter((f) => f.type.startsWith('image/'));

    // Same fallback shape as paste — Finder drags on macOS sometimes deliver
    // `text/uri-list` alongside the Files list, and in some WKWebView corners
    // only as text.
    const pathCandidates: string[] = [];
    if (files.length === 0) {
      const uriList = dt.getData('text/uri-list');
      if (uriList) pathCandidates.push(...parseUriList(uriList));
      const plain = dt.getData('text/plain');
      if (plain && pathCandidates.length === 0) {
        const first = plain.split(/\r?\n/)[0].trim();
        if (first) pathCandidates.push(first);
      }
    }

    if (files.length === 0 && pathCandidates.length === 0) return false;
    if (files.length === 0 && !pathCandidates.some(looksLikeImagePath)) return false;

    e.preventDefault();
    (async () => {
      const entries: Array<{ relPath: string; altFromName: string }> = [];
      for (const f of files) {
        try {
          entries.push(await saveImageFile(f));
        } catch (err) {
          console.warn('[imageEmbed] drop save failed', err);
        }
      }
      for (const p of pathCandidates) {
        const res = await saveImageByPath(p);
        if (res) entries.push(res);
      }
      insertEmbedMarkdown(view, entries);
    })();
    return true;
  }
});

function looksLikeImagePath(s: string): boolean {
  const t = s.trim();
  if (!t) return false;
  const target = /^file:\/\//i.test(t) || t.startsWith('/') ? t.toLowerCase() : '';
  if (!target) return false;
  const m = target.match(/\.([a-z0-9]+)(?:$|\?)/);
  return !!m && IMAGE_EXTS.has(m[1]);
}

// ---------------------------------------------------------------------------
// Theme fragment — small, co-located so the extension is drop-in.

export const imageEmbedTheme = EditorView.theme({
  '.cm-md-image-preview': {
    padding: '2px 0'
  },
  '.cm-md-image-error': {
    display: 'inline-block',
    margin: '8px 0',
    padding: '8px 12px',
    borderRadius: '6px',
    border: '1px dashed var(--color-border, #e0ddd4)',
    color: 'var(--color-fg-muted, #888)',
    fontSize: '12px',
    fontFamily: 'var(--font-mono)'
  }
});
