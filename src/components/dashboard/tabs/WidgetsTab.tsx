import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { currentMonitor } from '@tauri-apps/api/window';
import {
  Wifi, Bluetooth, Coffee, SunMoon, Mic, MicOff, Volume2, VolumeX, Camera, Lock, AppWindow,
} from 'lucide-react';
import { useWeather } from '../../../hooks/useWeather';

interface WidgetsTabProps {
  dndActive: boolean;
  onToggleDnd: () => void;
}

// ── Weather ──

function WeatherCard() {
  const { temperature, weatherCondition, weatherIcon: WeatherIcon, cityName, tempUnit } = useWeather(true);
  return (
    <div className="dash-card dash-weather">
      <WeatherIcon size={26} strokeWidth={1.8} className="dash-weather-icon" />
      <div className="dash-weather-temp">
        {temperature !== null ? `${temperature}°${tempUnit === 'fahrenheit' ? 'F' : 'C'}` : '—'}
      </div>
      <div className="dash-weather-meta">
        <span className="dash-weather-condition">{weatherCondition || 'Loading…'}</span>
        {cityName && <span className="dash-weather-city">{cityName}</span>}
      </div>
    </div>
  );
}

// ── Quick toggles ──

function TogglesCard({ dndActive, onToggleDnd }: WidgetsTabProps) {
  const [wifiOn, setWifiOn] = useState(true);
  const [btOn, setBtOn] = useState(true);
  const [lightTheme, setLightTheme] = useState(() => localStorage.getItem('nectar-theme-mode') === 'light');
  const [micMuted, setMicMuted] = useState(false);
  const [volMuted, setVolMuted] = useState(false);

  useEffect(() => {
    invoke<{ enabled: boolean; connected: boolean }>('get_wifi_status')
      .then((s) => setWifiOn(s.enabled))
      .catch(() => {});
    invoke<boolean>('get_bluetooth_state').then(setBtOn).catch(() => {});
    invoke<{ volume: number; is_muted: boolean }>('get_volume_state')
      .then((s) => setVolMuted(s.is_muted))
      .catch(() => {});
  }, []);

  const toggleWifi = () => {
    const next = !wifiOn;
    setWifiOn(next);
    invoke('set_wifi_state', { enabled: next }).catch(() => setWifiOn(!next));
  };

  const toggleBluetooth = () => {
    const next = !btOn;
    setBtOn(next);
    invoke('set_bluetooth_state', { enabled: next }).catch(() => setBtOn(!next));
  };

  const toggleTheme = () => {
    const next = lightTheme ? 'dark' : 'light';
    setLightTheme(!lightTheme);
    // localStorage first — the theme listener applies from there on settings-changed.
    localStorage.setItem('nectar-theme-mode', next);
    invoke('save_setting', { key: 'nectar-theme-mode', value: next }).catch(() => {});
  };

  const toggleMic = () => {
    const next = !micMuted;
    setMicMuted(next);
    invoke('set_mic_mute', { muted: next }).catch(() => setMicMuted(!next));
  };

  const toggleVolumeMute = () => {
    setVolMuted(!volMuted);
    invoke('toggle_volume_mute').catch(() => setVolMuted(volMuted));
  };

  const screenshot = async () => {
    try {
      const mon = await currentMonitor();
      const region = mon
        ? { x: mon.position.x, y: mon.position.y, width: mon.size.width, height: mon.size.height }
        : null;
      await invoke('take_screenshot', { region });
    } catch (e) {
      console.error(e);
    }
  };

  const snip = async () => {
    try {
      const mon = await currentMonitor();
      if (!mon) return;
      await invoke('start_snip', {
        region: { x: mon.position.x, y: mon.position.y, width: mon.size.width, height: mon.size.height },
      });
    } catch (e) {
      console.error(e);
    }
  };

  const toggles: { icon: typeof Wifi; label: string; active: boolean; onClick: () => void; onContextMenu?: () => void }[] = [
    { icon: Wifi, label: 'Wi-Fi', active: wifiOn, onClick: toggleWifi },
    { icon: Bluetooth, label: 'Bluetooth', active: btOn, onClick: toggleBluetooth },
    { icon: Coffee, label: 'Do Not Disturb', active: dndActive, onClick: onToggleDnd },
    { icon: SunMoon, label: lightTheme ? 'Light theme' : 'Dark theme', active: lightTheme, onClick: toggleTheme },
    { icon: micMuted ? MicOff : Mic, label: micMuted ? 'Mic muted' : 'Mic on', active: !micMuted, onClick: toggleMic },
    { icon: volMuted ? VolumeX : Volume2, label: volMuted ? 'Sound muted' : 'Sound on', active: !volMuted, onClick: toggleVolumeMute },
    { icon: Camera, label: 'Screenshot (klik) · Pilih area (klik kanan)', active: false, onClick: () => { void screenshot(); }, onContextMenu: () => { void snip(); } },
    { icon: Lock, label: 'Lock', active: false, onClick: () => { invoke('lock_workstation').catch(() => {}); } },
  ];

  return (
    <div className="dash-card dash-toggles">
      {toggles.map(({ icon: Icon, label, active, onClick, onContextMenu }) => (
        <button
          key={label}
          className={`dash-toggle ${active ? 'active' : ''}`}
          title={label}
          onClick={(e) => { e.stopPropagation(); onClick(); }}
          onContextMenu={onContextMenu ? (e) => { e.preventDefault(); e.stopPropagation(); onContextMenu(); } : undefined}
        >
          <Icon size={16} strokeWidth={2.2} />
        </button>
      ))}
    </div>
  );
}

// ── Pinned apps ──

interface ShelfApp {
  name: string;
  path: string;
  args?: string;
}

function AppsCard() {
  const [apps, setApps] = useState<ShelfApp[]>([]);
  const [icons, setIcons] = useState<Record<string, string>>({});

  useEffect(() => {
    invoke<ShelfApp[]>('load_pinned_apps')
      .then((list) => {
        const first = list.slice(0, 6);
        setApps(first);
        first.forEach((a) => {
          invoke<string | null>('get_app_icon', { path: a.path, name: a.name, hwnd: null })
            .then((icon) => {
              if (icon) setIcons((prev) => ({ ...prev, [a.path]: icon }));
            })
            .catch(() => {});
        });
      })
      .catch(() => {});
  }, []);

  return (
    <div className="dash-card dash-apps">
      {apps.length === 0 && <span className="dash-apps-empty">No pinned apps</span>}
      {apps.map((app) => (
        <button
          key={app.path}
          className="dash-app"
          title={app.name}
          onClick={(e) => {
            e.stopPropagation();
            invoke('open_app', { appName: app.path, args: app.args ?? null }).catch(() => {});
          }}
        >
          {icons[app.path] ? (
            <img src={icons[app.path]} alt="" draggable={false} />
          ) : (
            <AppWindow size={20} strokeWidth={1.8} />
          )}
          <span className="dash-app-name">{app.name}</span>
        </button>
      ))}
    </div>
  );
}

// ── Day progress ──

function parseHm(v: string, fallback: number): number {
  const m = /^(\d{1,2}):(\d{2})$/.exec(v.trim());
  if (!m) return fallback;
  const h = Math.min(23, parseInt(m[1], 10));
  const min = Math.min(59, parseInt(m[2], 10));
  return h * 60 + min;
}

function DayCard() {
  const [now, setNow] = useState(() => new Date());
  const [start, setStart] = useState('06:00');
  const [end, setEnd] = useState('22:00');

  useEffect(() => {
    invoke<Record<string, unknown>>('load_settings')
      .then((s) => {
        if (s['nectar-day-start']) setStart(String(s['nectar-day-start']));
        if (s['nectar-day-end']) setEnd(String(s['nectar-day-end']));
      })
      .catch(() => {});
  }, []);

  useEffect(() => {
    const t = setInterval(() => setNow(new Date()), 30000);
    return () => clearInterval(t);
  }, []);

  const startMin = parseHm(start, 6 * 60);
  const endMin = parseHm(end, 22 * 60);
  const nowMin = now.getHours() * 60 + now.getMinutes();

  let progress: number;
  let remaining: number;
  let label: string;
  if (nowMin >= startMin && nowMin < endMin) {
    progress = (nowMin - startMin) / Math.max(1, endMin - startMin);
    remaining = endMin - nowMin;
    label = 'until bedtime';
  } else {
    const nightLen = 1440 - Math.max(1, endMin - startMin);
    const elapsedNight = (nowMin - endMin + 1440) % 1440;
    progress = elapsedNight / Math.max(1, nightLen);
    remaining = (startMin - nowMin + 1440) % 1440;
    label = 'until morning';
  }

  const hours = Math.floor(remaining / 60);
  const mins = remaining % 60;

  return (
    <div className="dash-card dash-day">
      <div className="dash-day-pct">{Math.round(progress * 100)}%</div>
      <div className="dash-day-remaining">
        {hours}h {mins.toString().padStart(2, '0')}m {label}
      </div>
      <div className="dash-day-bar">
        <div className="dash-day-bar-fill" style={{ width: `${Math.min(100, Math.max(0, progress * 100))}%` }} />
      </div>
    </div>
  );
}

export function WidgetsTab({ dndActive, onToggleDnd }: WidgetsTabProps) {
  return (
    <div className="dash-widgets">
      <WeatherCard />
      <TogglesCard dndActive={dndActive} onToggleDnd={onToggleDnd} />
      <AppsCard />
      <DayCard />
    </div>
  );
}
