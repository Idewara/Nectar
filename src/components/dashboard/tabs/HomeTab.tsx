import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { emit } from '@tauri-apps/api/event';
import { Plus, Webcam } from 'lucide-react';
import type { CaldavState } from '../../hooks/useCaldav';
import { PlayIcon, PauseIcon, SkipBackIcon, SkipForwardIcon, MusicNoteIcon } from '../../icons';

// Structurally compatible with the MediaInfo interface in App.tsx.
export interface DashboardMediaInfo {
  title: string;
  artist: string;
  is_playing: boolean;
  has_media: boolean;
  artwork?: string[];
  position_ms?: number;
  duration_ms?: number;
  seek_enabled?: boolean;
  position_updated_at?: number;
}

interface HomeTabProps {
  mediaInfo: DashboardMediaInfo;
  albumArtUrl: string | null;
  albumArtKey: number;
  isPlaying: boolean;
  onTogglePlayPause: () => void;
  caldav: CaldavState;
  hour12: boolean;
}

// ── Music card ──

function formatClock(ms: number): string {
  if (ms <= 0) return '0:00';
  const totalSeconds = Math.floor(ms / 1000);
  return `${Math.floor(totalSeconds / 60)}:${(totalSeconds % 60).toString().padStart(2, '0')}`;
}

function MusicCard({
  mediaInfo,
  albumArtUrl,
  albumArtKey,
  isPlaying,
  onTogglePlayPause,
}: Omit<HomeTabProps, 'caldav' | 'hour12'>) {
  const positionMs = mediaInfo.position_ms ?? 0;
  const durationMs = mediaInfo.duration_ms ?? 0;
  const positionUpdatedAt = mediaInfo.position_updated_at ?? 0;

  // Last backend position + arrival time; displayPosition extrapolates between updates.
  const backendPosRef = useRef(positionMs);
  const backendTimeRef = useRef(positionUpdatedAt || Date.now());

  useEffect(() => {
    backendPosRef.current = positionMs;
    backendTimeRef.current = positionUpdatedAt || Date.now();
  }, [positionMs, positionUpdatedAt]);

  const [displayPosition, setDisplayPosition] = useState(positionMs);

  useEffect(() => {
    if (!mediaInfo.has_media || !isPlaying || durationMs <= 0) return;
    const tick = () => {
      const elapsed = Date.now() - backendTimeRef.current;
      setDisplayPosition(Math.min(backendPosRef.current + elapsed, durationMs));
    };
    tick();
    const interval = setInterval(tick, 250);
    return () => clearInterval(interval);
  }, [mediaInfo.has_media, isPlaying, durationMs]);

  const hasMedia = mediaInfo.has_media;
  const progressPercent = durationMs > 0 ? (displayPosition / durationMs) * 100 : 0;

  const skip = useCallback((cmd: 'media_previous' | 'media_next') => {
    invoke(cmd).catch(() => {});
  }, []);

  return (
    <div className="home-card home-music">
      <div className="home-music-top">
        <div className="home-music-art">
          {hasMedia && albumArtUrl ? (
            <img key={albumArtKey} src={albumArtUrl} alt="" draggable={false} />
          ) : (
            <MusicNoteIcon size={24} />
          )}
        </div>
        <div className="home-music-meta">
          <div className="home-music-title">{hasMedia ? mediaInfo.title : 'Nothing playing'}</div>
          <div className="home-music-sub">
            {hasMedia ? mediaInfo.artist : 'Play something in Spotify, YouTube…'}
          </div>
        </div>
      </div>

      <div className="home-music-progress">
        <div className="home-music-track">
          <div className="home-music-fill" style={{ width: `${Math.min(100, Math.max(0, progressPercent))}%` }} />
        </div>
        <div className="home-music-times">
          <span>{formatClock(hasMedia ? displayPosition : 0)}</span>
          <span>{formatClock(hasMedia ? durationMs : 0)}</span>
        </div>
      </div>

      <div className="home-music-controls">
        <button className="home-music-btn" title="Previous" onClick={(e) => { e.stopPropagation(); skip('media_previous'); }}>
          <SkipBackIcon size={16} />
        </button>
        <button className="home-music-btn home-music-play" title={isPlaying ? 'Pause' : 'Play'} onClick={(e) => { e.stopPropagation(); onTogglePlayPause(); }}>
          {isPlaying ? <PauseIcon size={18} /> : <PlayIcon size={18} />}
        </button>
        <button className="home-music-btn" title="Next" onClick={(e) => { e.stopPropagation(); skip('media_next'); }}>
          <SkipForwardIcon size={16} />
        </button>
      </div>
    </div>
  );
}

// ── Mini calendar card ──

const WEEKDAY_LETTERS = ['M', 'T', 'W', 'T', 'F', 'S', 'S'];
const DAY_MS = 86400000;

function CalendarCard({ caldav, hour12 }: { caldav: CaldavState; hour12: boolean }) {
  const today = new Date();
  const mondayOffset = (today.getDay() + 6) % 7; // week starts Monday
  const monday = new Date(today.getFullYear(), today.getMonth(), today.getDate() - mondayOffset);
  const weekDates = Array.from(
    { length: 7 },
    (_, i) => new Date(monday.getFullYear(), monday.getMonth(), monday.getDate() + i),
  );

  const dayStartMs = new Date(today.getFullYear(), today.getMonth(), today.getDate()).getTime();
  const todayEvents = caldav.events
    .filter((e) => e.start >= dayStartMs && e.start < dayStartMs + DAY_MS)
    .sort((a, b) => a.start - b.start)
    .slice(0, 2);

  const eventTime = (ms: number, allDay: boolean) =>
    allDay ? 'All day' : new Date(ms).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit', hour12 });

  const connect = async () => {
    await invoke('open_settings_window').catch(() => {});
    await emit('settings-navigate', 'calendar-sync').catch(() => {});
  };

  return (
    <div className="home-card home-cal">
      <div className="home-cal-head">
        <div className="home-cal-title">
          {today.toLocaleDateString([], { month: 'long' })} <span>{today.getFullYear()}</span>
        </div>
        <button
          className="home-cal-add"
          title="Open calendar"
          onClick={(e) => { e.stopPropagation(); invoke('open_calendar_window').catch(() => {}); }}
        >
          <Plus size={13} strokeWidth={2.5} />
        </button>
      </div>

      <div className="home-cal-week">
        <div className="home-cal-dow">
          {WEEKDAY_LETTERS.map((l, i) => (
            <span key={i}>{l}</span>
          ))}
        </div>
        <div className="home-cal-dates">
          {weekDates.map((d, i) => (
            <span
              key={i}
              className={d.toDateString() === today.toDateString() ? 'home-cal-today' : ''}
            >
              {d.getDate()}
            </span>
          ))}
        </div>
      </div>

      <div className="home-cal-foot">
        {todayEvents.length > 0 ? (
          todayEvents.map((e) => (
            <div key={e.uid} className="home-cal-event">
              <span className="home-cal-event-time">{eventTime(e.start, e.allDay)}</span>
              <span className="home-cal-event-title">{e.title}</span>
            </div>
          ))
        ) : caldav.connected ? (
          <span className="home-cal-empty">No events today.</span>
        ) : (
          <>
            <span className="home-cal-empty">No calendars yet.</span>
            <button className="home-cal-link" onClick={(e) => { e.stopPropagation(); void connect(); }}>
              Connect Google / Outlook calendar
            </button>
          </>
        )}
      </div>
    </div>
  );
}

// ── Mirror card (placeholder) ──

function MirrorCard() {
  return (
    <div className="home-card home-mirror" title="Coming soon">
      <Webcam size={28} strokeWidth={1.8} />
      <span>Mirror</span>
    </div>
  );
}

export function HomeTab(props: HomeTabProps) {
  const { mediaInfo, albumArtUrl, albumArtKey, isPlaying, onTogglePlayPause, caldav, hour12 } = props;
  return (
    <div className="dash-home">
      <MusicCard
        mediaInfo={mediaInfo}
        albumArtUrl={albumArtUrl}
        albumArtKey={albumArtKey}
        isPlaying={isPlaying}
        onTogglePlayPause={onTogglePlayPause}
      />
      <CalendarCard caldav={caldav} hour12={hour12} />
      <MirrorCard />
    </div>
  );
}
