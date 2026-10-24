import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { open } from '@tauri-apps/plugin-dialog';
import { startDrag } from '@crabnebula/tauri-plugin-drag';
import { FileText, Image as ImageIcon, File, Plus, Share2, Trash2, X } from 'lucide-react';

const SHELF_KEY = 'nectar-shelf-files';

// The drag plugin wants an image for the cursor preview — a tiny generated canvas PNG.
let cachedDragIcon: string | null = null;
function dragIcon(): string {
  if (cachedDragIcon) return cachedDragIcon;
  const c = document.createElement('canvas');
  c.width = 48;
  c.height = 48;
  const ctx = c.getContext('2d');
  if (ctx) {
    ctx.fillStyle = 'rgba(25, 25, 27, 0.92)';
    ctx.beginPath();
    ctx.roundRect(2, 2, 44, 44, 10);
    ctx.fill();
    ctx.strokeStyle = 'rgba(255, 255, 255, 0.75)';
    ctx.lineWidth = 2.5;
    ctx.beginPath();
    ctx.moveTo(17, 13);
    ctx.lineTo(27, 13);
    ctx.lineTo(33, 19);
    ctx.lineTo(33, 35);
    ctx.lineTo(17, 35);
    ctx.closePath();
    ctx.moveTo(27, 13);
    ctx.lineTo(27, 19);
    ctx.lineTo(33, 19);
    ctx.stroke();
  }
  cachedDragIcon = c.toDataURL('image/png');
  return cachedDragIcon;
}

function baseName(p: string): string {
  return p.split(/[\\/]/).pop() || p;
}

function iconFor(p: string) {
  const ext = (p.split('.').pop() || '').toLowerCase();
  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'ico'].includes(ext)) return ImageIcon;
  if (['txt', 'md', 'log', 'json', 'doc', 'docx', 'rtf'].includes(ext)) return FileText;
  return File;
}

export function FilesTab() {
  const [paths, setPaths] = useState<string[]>(() => {
    try {
      const parsed = JSON.parse(localStorage.getItem(SHELF_KEY) || '[]');
      return Array.isArray(parsed) ? parsed.filter((p) => typeof p === 'string') : [];
    } catch {
      return [];
    }
  });
  const [dropActive, setDropActive] = useState(false);
  const skipFirstPersist = useRef(true);

  // settings.json is the cross-window source of truth; localStorage is the fast cache.
  useEffect(() => {
    invoke<Record<string, unknown>>('load_settings')
      .then((s) => {
        const raw = s[SHELF_KEY];
        if (typeof raw === 'string') {
          try {
            const parsed = JSON.parse(raw);
            if (Array.isArray(parsed)) setPaths(parsed.filter((p) => typeof p === 'string'));
          } catch { /* keep localStorage value */ }
        }
      })
      .catch(() => {});
  }, []);

  // Persist on every change (skips the initial localStorage-seeded render).
  useEffect(() => {
    if (skipFirstPersist.current) {
      skipFirstPersist.current = false;
      return;
    }
    const json = JSON.stringify(paths);
    localStorage.setItem(SHELF_KEY, json);
    invoke('save_setting', { key: SHELF_KEY, value: json }).catch(() => {});
  }, [paths]);

  // Drop-in: OS file drops land on the webview.
  useEffect(() => {
    const unlisten = getCurrentWebviewWindow().onDragDropEvent((event) => {
      const payload = event.payload;
      if (payload.type === 'enter' || payload.type === 'over') {
        setDropActive(true);
      } else if (payload.type === 'leave') {
        setDropActive(false);
      } else if (payload.type === 'drop') {
        setDropActive(false);
        setPaths((prev) => {
          const next = [...prev];
          payload.paths.forEach((p) => {
            if (!next.includes(p)) next.push(p);
          });
          return next;
        });
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const addViaDialog = async () => {
    try {
      const picked = await open({ multiple: true });
      if (picked && picked.length > 0) {
        setPaths((prev) => {
          const next = [...prev];
          picked.forEach((p) => {
            if (!next.includes(p)) next.push(p);
          });
          return next;
        });
      }
    } catch (e) {
      console.error(e);
    }
  };

  const shareAll = () => {
    navigator.clipboard.writeText(paths.join('\n')).catch(() => {});
  };

  const removeOne = (p: string) => {
    setPaths((prev) => prev.filter((x) => x !== p));
  };

  return (
    <div className={`dash-files ${dropActive ? 'drop-active' : ''}`}>
      {paths.length === 0 ? (
        <div className="dash-files-empty">
          <span className="dash-files-empty-title">Drop files here</span>
          <span className="dash-files-empty-sub">Keep files handy, then drag them out anywhere</span>
        </div>
      ) : (
        <div className="dash-files-list">
          {paths.map((p) => {
            const Icon = iconFor(p);
            return (
              <div
                key={p}
                className="dash-file-chip"
                title={`${p} — drag out to share`}
                onPointerDown={(e) => {
                  if (e.button !== 0) return;
                  startDrag({ item: [p], icon: dragIcon() }).catch(() => {});
                }}
              >
                <Icon size={14} strokeWidth={2} className="dash-file-icon" />
                <span className="dash-file-name">{baseName(p)}</span>
                <button
                  className="dash-file-remove"
                  title="Remove"
                  onPointerDown={(e) => e.stopPropagation()}
                  onClick={(e) => {
                    e.stopPropagation();
                    removeOne(p);
                  }}
                >
                  <X size={11} strokeWidth={2.5} />
                </button>
              </div>
            );
          })}
        </div>
      )}
      <div className="dash-files-actions">
        <button className="dash-files-btn" title="Add files" onClick={(e) => { e.stopPropagation(); void addViaDialog(); }}>
          <Plus size={14} strokeWidth={2.2} />
        </button>
        <button
          className="dash-files-btn"
          title="Copy all paths"
          disabled={paths.length === 0}
          onClick={(e) => { e.stopPropagation(); shareAll(); }}
        >
          <Share2 size={14} strokeWidth={2.2} />
        </button>
        <button
          className="dash-files-btn"
          title="Clear shelf"
          disabled={paths.length === 0}
          onClick={(e) => { e.stopPropagation(); setPaths([]); }}
        >
          <Trash2 size={14} strokeWidth={2.2} />
        </button>
      </div>
    </div>
  );
}
