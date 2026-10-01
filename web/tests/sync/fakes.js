// Shared test fakes for the sync suite — manual clock + mock sockets.
// Nothing fires without advance(); no sockets exist until the factory makes
// one; tests drive every event by hand.

/** Manual clock + timer wheel. @returns {object} */
export function fakeClock() {
  let currentTime = 0;
  let seq = 0;
  const pending = new Map();
  return {
    now: () => currentTime,
    timers: {
      setTimeout(fn, ms) {
        const id = ++seq;
        pending.set(id, { fn, at: currentTime + ms });
        return id;
      },
      clearTimeout(id) {
        pending.delete(id);
      },
    },
    advance(ms) {
      const target = currentTime + ms;
      for (;;) {
        const due = [...pending.entries()]
          .filter(([, p]) => p.at <= target)
          .sort((a, b) => a[1].at - b[1].at)[0];
        if (!due) break;
        const [id, p] = due;
        pending.delete(id);
        currentTime = Math.max(currentTime, p.at);
        p.fn();
      }
      currentTime = target;
    },
  };
}

/** Records every socket the connection opens; tests drive their events. */
export function mockSockets() {
  const sockets = [];
  const factory = (url) => {
    const s = {
      url,
      sent: [],
      closed: false,
      onopen: null,
      onmessage: null,
      onclose: null,
      onerror: null,
      send(data) {
        s.sent.push(data);
      },
      close() {
        s.closed = true;
        s.onclose?.();
      },
      open() {
        s.onopen?.();
      },
      message(data) {
        s.onmessage?.({ data });
      },
      error() {
        s.onerror?.({ type: 'error' });
      },
    };
    sockets.push(s);
    return s;
  };
  return { sockets, factory };
}

/** In-memory {getItem,setItem} — the queue/storage interface minus the DOM. */
export function fakeStorage() {
  const map = new Map();
  return {
    getItem: (k) => (map.has(k) ? map.get(k) : null),
    setItem: (k, v) => map.set(k, v),
  };
}
