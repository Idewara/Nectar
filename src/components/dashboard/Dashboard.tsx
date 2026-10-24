import { motion, AnimatePresence } from 'framer-motion';
import { Home, LayoutGrid, FolderDown, StickyNote, Coffee, Settings, X } from 'lucide-react';
import type { CaldavState } from '../../hooks/useCaldav';
import { HomeTab, type DashboardMediaInfo } from './tabs/HomeTab';
import { WidgetsTab } from './tabs/WidgetsTab';
import { FilesTab } from './tabs/FilesTab';
import { NotesTab } from './tabs/NotesTab';
import './Dashboard.css';

export type DashboardTab = 'home' | 'widgets' | 'files' | 'notes';

const TABS: { id: DashboardTab; icon: typeof Home; label: string }[] = [
  { id: 'home', icon: Home, label: 'Home' },
  { id: 'widgets', icon: LayoutGrid, label: 'Widgets' },
  { id: 'files', icon: FolderDown, label: 'Files' },
  { id: 'notes', icon: StickyNote, label: 'Notes' },
];

interface DashboardProps {
  activeTab: DashboardTab;
  onTabChange: (tab: DashboardTab) => void;
  onClose: () => void;
  time: string;
  dndActive: boolean;
  onToggleDnd: () => void;
  onOpenSettings: () => void;
  batteryLevel: number;
  isCharging: boolean;
  hasBattery: boolean;
  mediaInfo: DashboardMediaInfo;
  albumArtUrl: string | null;
  albumArtKey: number;
  isPlaying: boolean;
  onTogglePlayPause: () => void;
  caldav: CaldavState;
  hour12: boolean;
}

export function Dashboard({
  activeTab,
  onTabChange,
  onClose,
  time,
  dndActive,
  onToggleDnd,
  onOpenSettings,
  batteryLevel,
  isCharging,
  hasBattery,
  mediaInfo,
  albumArtUrl,
  albumArtKey,
  isPlaying,
  onTogglePlayPause,
  caldav,
  hour12,
}: DashboardProps) {
  return (
    <div className="dashboard">
      <div className="dashboard-top-strip">
        <div className="dashboard-tabs">
          {TABS.map(({ id, icon: Icon, label }) => (
            <button
              key={id}
              className={`dashboard-tab ${activeTab === id ? 'active' : ''}`}
              title={label}
              onClick={(e) => { e.stopPropagation(); onTabChange(id); }}
            >
              <Icon size={16} strokeWidth={2.2} />
            </button>
          ))}
        </div>
        <div className="dashboard-right-cluster">
          <span className="dashboard-clock">{time}</span>
          <button
            className={`dashboard-strip-btn ${dndActive ? 'active' : ''}`}
            title="Do Not Disturb"
            onClick={(e) => { e.stopPropagation(); onToggleDnd(); }}
          >
            <Coffee size={15} strokeWidth={2.2} />
          </button>
          <button
            className="dashboard-strip-btn"
            title="Settings"
            onClick={(e) => { e.stopPropagation(); onOpenSettings(); }}
          >
            <Settings size={15} strokeWidth={2.2} />
          </button>
          {hasBattery && (
            <span className="dashboard-battery">
              {batteryLevel}%{isCharging ? ' ⚡' : ''}
            </span>
          )}
        </div>
      </div>

      <div className="dashboard-content">
        <button
          className="dashboard-close"
          title="Close"
          onClick={(e) => { e.stopPropagation(); onClose(); }}
        >
          <X size={14} strokeWidth={2.5} />
        </button>
        <AnimatePresence mode="wait">
          <motion.div
            key={activeTab}
            className="dashboard-tab-pane"
            initial={{ opacity: 0, scale: 0.99 }}
            animate={{ opacity: 1, scale: 1 }}
            exit={{ opacity: 0, scale: 0.99, transition: { duration: 0.08 } }}
            transition={{ type: 'spring', stiffness: 500, damping: 32 }}
          >
            {activeTab === 'home' && (
              <HomeTab
                mediaInfo={mediaInfo}
                albumArtUrl={albumArtUrl}
                albumArtKey={albumArtKey}
                isPlaying={isPlaying}
                onTogglePlayPause={onTogglePlayPause}
                caldav={caldav}
                hour12={hour12}
              />
            )}
            {activeTab === 'widgets' && <WidgetsTab dndActive={dndActive} onToggleDnd={onToggleDnd} />}
            {activeTab === 'files' && <FilesTab />}
            {activeTab === 'notes' && <NotesTab />}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  );
}
