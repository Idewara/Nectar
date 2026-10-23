import { NotchCalendar } from '../NotchCalendar';
import type { CaldavState } from '../../hooks/useCaldav';

interface CalendarTabProps {
  caldav: CaldavState;
  hour12: boolean;
}

export function CalendarTab({ caldav, hour12 }: CalendarTabProps) {
  return (
    <div className="dashboard-calendar-tab">
      <NotchCalendar caldav={caldav} hour12={hour12} />
    </div>
  );
}
