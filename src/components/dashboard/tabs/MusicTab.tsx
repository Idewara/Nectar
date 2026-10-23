import { useCallback, useEffect, useRef, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { invoke } from '@tauri-apps/api/core';
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

interface MusicTabProps {
  mediaInfo: DashboardMediaInfo;
  albumArtUrl: string | null;
  albumArtKey: number;
  isPlaying: boolean;
  onTogglePlayPause: () => void;
}

const MARQUEE_SPEED = 30;
const MARQUEE_MIN_DURATION = 5;

function DashMarquee({ title }: { title: string }) {
  const textRef = useRef<HTMLSpanElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [isOverflowing, setIsOverflowing] = useState(false);
  const [scrollDistance, setScrollDistance] = useState(0);
  const [scrollDuration, setScrollDuration] = useState(8);

  useEffect(() => {
    const check = () => {
      if (textRef.current && containerRef.current) {
        const textWidth = textRef.current.scrollWidth;
        const containerWidth = containerRef.current.clientWidth;
        const overflow = textWidth > containerWidth;
        setIsOverflowing(overflow);
        if (overflow) {
          const distance = textWidth - containerWidth;
          setScrollDistance(distance);
          setScrollDuration(Math.max(distance / MARQUEE_SPEED, MARQUEE_MIN_DURATION));
        }
      }
    };
    check();
    const observer = new ResizeObserver(check);
    if (containerRef.current) observer.observe(containerRef.current);
    return () => observer.disconnect();
  }, [title]);

  return (
    <div ref={containerRef} className="dash-marquee-wrap">
      <span
        ref={textRef}
        className={`dash-music-title ${isOverflowing ? 'dash-marquee' : ''}`}
        style={{
          '--scroll-distance': isOverflowing ? `-${scrollDistance}px` : undefined,
          '--scroll-duration': `${scrollDuration}s`,
        } as React.CSSProperties}
      >
        {title}
      </span>
    </div>
  );
}

function formatTime(ms: number): string {
  if (ms <= 0) return '0:00';
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}:${seconds.toString().padStart(2, '0')}`;
}

function formatRemaining(elapsed: number, duration: number): string {
  const remaining = Math.max(0, duration - elapsed);
  const totalSeconds = Math.floor(remaining / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `-${minutes}:${seconds.toString().padStart(2, '0')}`;
}

export function MusicTab({ mediaInfo, albumArtUrl, albumArtKey, isPlaying, onTogglePlayPause }: MusicTabProps) {
  const positionMs = mediaInfo.position_ms ?? 0;
  const durationMs = mediaInfo.duration_ms ?? 0;
  const seekEnabled = mediaInfo.seek_enabled ?? false;
  const positionUpdatedAt = mediaInfo.position_updated_at ?? 0;

  // Last backend position + when it arrived; displayPosition extrapolates between updates.
  const backendPosRef = useRef(positionMs);
  const backendTimeRef = useRef(positionUpdatedAt || Date.now());
  const isSeekingRef = useRef(false);

  useEffect(() => {
    if (!isSeekingRef.current) {
      backendPosRef.current = positionMs;
      backendTimeRef.current = positionUpdatedAt || Date.now();
    }
  }, [positionMs, positionUpdatedAt]);

  const [displayPosition, setDisplayPosition] = useState(() => {
    const elapsed = Date.now() - (positionUpdatedAt || Date.now());
    return Math.min(positionMs + (isPlaying ? elapsed : 0), durationMs || Infinity);
  });

  useEffect(() => {
    const tick = () => {
      if (isSeekingRef.current) return;
      const elapsed = isPlaying ? Date.now() - backendTimeRef.current : 0;
      const pos = Math.min(backendPosRef.current + elapsed, durationMs || Infinity);
      setDisplayPosition(pos);
    };
    tick();
    const interval = setInterval(tick, 250);
    return () => clearInterval(interval);
  }, [isPlaying, durationMs]);

  const handleSeekStart = useCallback(() => {
    isSeekingRef.current = true;
  }, []);

  const handleSeekChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    setDisplayPosition(Number(e.target.value));
  }, []);

  const handleSeekEnd = useCallback(() => {
    const seekPos = displayPosition;
    isSeekingRef.current = false;
    backendPosRef.current = seekPos;
    backendTimeRef.current = Date.now();
    invoke('media_seek', { positionMs: seekPos }).catch(() => {});
  }, [displayPosition]);

  const progressPercent = durationMs > 0 ? (displayPosition / durationMs) * 100 : 0;

  if (!mediaInfo.has_media) {
    return (
      <div className="dash-music-empty">
        <MusicNoteIcon size={28} />
        <span>Nothing playing</span>
      </div>
    );
  }

  return (
    <div className="dash-music">
      <div className="dash-music-art">
        <AnimatePresence mode="wait" initial={false}>
          {albumArtUrl ? (
            <motion.img
              key={`art-${albumArtKey}`}
              src={albumArtUrl}
              alt="Art"
              draggable={false}
              initial={{ rotateY: 90, opacity: 0 }}
              animate={{ rotateY: 0, opacity: 1 }}
              exit={{ rotateY: -90, opacity: 0 }}
              transition={{ duration: 0.3, ease: "easeInOut" }}
            />
          ) : (
            <motion.div
              key="placeholder"
              className="dash-music-art-placeholder"
              initial={{ rotateY: 90, opacity: 0 }}
              animate={{ rotateY: 0, opacity: 1 }}
              exit={{ rotateY: -90, opacity: 0 }}
              transition={{ duration: 0.3, ease: "easeInOut" }}
            >
              <MusicNoteIcon size={40} />
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      <div className="dash-music-main">
        <div className="dash-music-meta">
          <DashMarquee title={mediaInfo.title} />
          <span className="dash-music-artist">{mediaInfo.artist}</span>
        </div>

        {durationMs > 0 && (
          <div className="dash-music-progress-row">
            <span className="dash-music-time">{formatTime(displayPosition)}</span>
            <div className="dash-music-bar">
              <div className="dash-music-bar-fill" style={{ width: `${progressPercent}%` }} />
              {seekEnabled && (
                <input
                  type="range"
                  className="dash-music-seek"
                  min={0}
                  max={durationMs}
                  step={100}
                  value={displayPosition}
                  onMouseDown={handleSeekStart}
                  onTouchStart={handleSeekStart}
                  onChange={handleSeekChange}
                  onMouseUp={handleSeekEnd}
                  onTouchEnd={handleSeekEnd}
                  onClick={(e) => e.stopPropagation()}
                />
              )}
            </div>
            <span className="dash-music-time">{formatRemaining(displayPosition, durationMs)}</span>
          </div>
        )}

        <div className="dash-music-controls">
          <motion.button
            className="dash-music-btn"
            title="Previous"
            onClick={(e) => { e.stopPropagation(); invoke('media_previous').catch(() => {}); }}
            whileTap={{ scale: 0.9 }}
          >
            <SkipBackIcon size={26} />
          </motion.button>
          <motion.button
            className="dash-music-play"
            title={isPlaying ? 'Pause' : 'Play'}
            onClick={(e) => { e.stopPropagation(); onTogglePlayPause(); }}
            whileHover={{ scale: 1.06 }}
            whileTap={{ scale: 0.94 }}
          >
            <AnimatePresence mode="wait" initial={false}>
              <motion.div
                key={isPlaying ? 'pause' : 'play'}
                initial={{ opacity: 0, scale: 0.7 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.7 }}
                transition={{ duration: 0.12 }}
                style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}
              >
                {isPlaying ? <PauseIcon size={22} /> : <PlayIcon size={22} />}
              </motion.div>
            </AnimatePresence>
          </motion.button>
          <motion.button
            className="dash-music-btn"
            title="Next"
            onClick={(e) => { e.stopPropagation(); invoke('media_next').catch(() => {}); }}
            whileTap={{ scale: 0.9 }}
          >
            <SkipForwardIcon size={26} />
          </motion.button>
        </div>
      </div>
    </div>
  );
}
