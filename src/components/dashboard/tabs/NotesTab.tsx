import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Plus, Trash2 } from 'lucide-react';

interface Note {
  id: string;
  text: string;
  updated_at: number;
}

function noteTitle(text: string): string {
  const firstLine = text.split('\n')[0].trim();
  return firstLine || 'Untitled';
}

export function NotesTab() {
  const [notes, setNotes] = useState<Note[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [draft, setDraft] = useState('');
  const saveTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const notesRef = useRef<Note[]>([]);
  notesRef.current = notes;

  useEffect(() => {
    invoke<Note[]>('notes_list')
      .then((list) => {
        setNotes(list);
        if (list.length > 0) {
          setSelectedId(list[0].id);
          setDraft(list[0].text);
        }
      })
      .catch(() => {});
    return () => {
      if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    };
  }, []);

  const selectNote = (n: Note) => {
    setSelectedId(n.id);
    setDraft(n.text);
  };

  const createNote = () => {
    const note: Note = { id: String(Date.now()), text: '', updated_at: Date.now() };
    setNotes((prev) => [note, ...prev]);
    setSelectedId(note.id);
    setDraft('');
    invoke('notes_save', { note }).catch(() => {});
  };

  const onDraftChange = (text: string) => {
    setDraft(text);
    if (!selectedId) return;
    if (saveTimerRef.current) clearTimeout(saveTimerRef.current);
    const id = selectedId;
    saveTimerRef.current = setTimeout(() => {
      const updated: Note = { id, text, updated_at: Date.now() };
      setNotes((prev) => {
        const next = prev.map((n) => (n.id === id ? updated : n));
        next.sort((a, b) => b.updated_at - a.updated_at);
        return next;
      });
      invoke('notes_save', { note: updated }).catch(() => {});
    }, 500);
  };

  const deleteNote = () => {
    if (!selectedId) return;
    const id = selectedId;
    const next = notesRef.current.filter((n) => n.id !== id);
    setNotes(next);
    const first = next[0];
    setSelectedId(first ? first.id : null);
    setDraft(first ? first.text : '');
    invoke('notes_delete', { id }).catch(() => {});
  };

  const selected = notes.find((n) => n.id === selectedId) ?? null;

  return (
    <div className="dash-notes">
      <div className="dash-notes-list">
        <button className="dash-notes-new" onClick={(e) => { e.stopPropagation(); createNote(); }}>
          <Plus size={14} strokeWidth={2.2} />
          <span>New note</span>
        </button>
        {notes.length === 0 ? (
          <span className="dash-notes-empty">No notes yet</span>
        ) : (
          notes.map((n) => (
            <button
              key={n.id}
              className={`dash-notes-item ${n.id === selectedId ? 'active' : ''}`}
              onClick={(e) => { e.stopPropagation(); selectNote(n); }}
            >
              {noteTitle(n.text)}
            </button>
          ))
        )}
      </div>
      <div className="dash-notes-editor">
        {selected ? (
          <>
            <textarea
              value={draft}
              placeholder="Start typing… The first line becomes the title."
              onChange={(e) => onDraftChange(e.target.value)}
              onPointerDown={(e) => e.stopPropagation()}
            />
            <button className="dash-notes-delete" title="Delete note" onClick={(e) => { e.stopPropagation(); deleteNote(); }}>
              <Trash2 size={14} strokeWidth={2} />
            </button>
          </>
        ) : (
          <div className="dash-notes-no-selection">Select or create a note</div>
        )}
      </div>
    </div>
  );
}
