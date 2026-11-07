import { useEffect, useState } from "react";

const TICK_MS = 250;

/**
 * Wall-clock time for the recording timer, ticking only while the pill is on screen: the
 * overlay page is never suspended (so it wakes instantly), so an idle ticker would cost power.
 */
export function useClock(active: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!active) return;
    setNow(Date.now());
    const id = setInterval(() => setNow(Date.now()), TICK_MS);
    return () => clearInterval(id);
  }, [active]);
  return now;
}
