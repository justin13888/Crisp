import { afterAll, beforeAll, describe, expect, test } from "bun:test";
import { GlobalRegistrator } from "@happy-dom/global-registrator";
import type { Alarm, AlarmInfo } from "../lib/alarm";
import type { Timer } from "../lib/timers";

// React DOM decides whether a DOM exists when it is first imported, so the
// DOM is registered before the dynamic imports below.
GlobalRegistrator.register({ url: "http://localhost/" });
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

const React = await import("react");
const { act } = React;
const { createRoot } = await import("react-dom/client");
const { MemoryRouter } = await import("react-router-dom");
const { TimersProvider, useTimers } = await import("../hooks/useTimers");
const { TimerDock } = await import("./TimerDock");

const MIN = 60_000;

/** A fake alarm that records calls. */
function fakeAlarm() {
  const fired: AlarmInfo[] = [];
  const stopped: (string | undefined)[] = [];
  let primed = 0;
  const active = new Set<string>();
  const alarm: Alarm = {
    prime: () => {
      primed++;
    },
    fire: (info) => {
      fired.push(info);
      active.add(info.id);
    },
    stop: (id) => {
      stopped.push(id);
      if (id === undefined) active.clear();
      else active.delete(id);
    },
    ringing: () => [...active],
  };
  return { alarm, fired, stopped, primed: () => primed };
}

let now = 1_700_000_000_000;
const clock = { now: () => now };

/** Exposes the timers API to the test. */
let api: ReturnType<typeof useTimers> | null = null;
function Probe() {
  api = useTimers();
  return null;
}

function timersApi(): ReturnType<typeof useTimers> {
  if (!api) throw new Error("not mounted");
  return api;
}

async function advance(ms: number): Promise<void> {
  now += ms;
  // Let the provider's tick timeout fire and effects settle.
  await act(async () => {
    window.dispatchEvent(new Event("focus"));
  });
}

function click(label: string): Promise<void> {
  const button = [...document.querySelectorAll("button")].find(
    (b) => b.textContent === label || b.getAttribute("aria-label") === label,
  );
  if (!button) throw new Error(`no button ${label}`);
  return act(async () => {
    button.click();
  });
}

function rows(): string[] {
  return [...document.querySelectorAll("li.timer")].map(
    (li) => li.getAttribute("data-timer-id") ?? "",
  );
}

describe("TimersProvider + TimerDock", () => {
  const fake = fakeAlarm();
  const saved: (readonly Timer[])[] = [];
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);

  beforeAll(async () => {
    await act(async () => {
      root.render(
        <MemoryRouter>
          <TimersProvider
            clock={clock}
            alarm={fake.alarm}
            persistence={{
              load: async () => [],
              save: async (timers) => {
                saved.push(timers);
              },
            }}
          >
            <Probe />
            <TimerDock collapseAt={3} />
          </TimersProvider>
        </MemoryRouter>,
      );
    });
  });

  afterAll(async () => {
    await act(async () => root.unmount());
    await GlobalRegistrator.unregister();
  });

  test("renders nothing but the live region with no timers", () => {
    expect(document.querySelector(".timer-dock")).toBeNull();
    expect(document.querySelector('[aria-live="assertive"]')).not.toBeNull();
  });

  test("starting a timer primes the alarm and shows a countdown", async () => {
    await act(async () => {
      timersApi().start({
        id: "pasta",
        label: "Pasta",
        context: { title: "Carbonara", step: 2, href: "/recipe/abc" },
        durationMs: 9 * MIN,
      });
    });
    expect(fake.primed()).toBe(1);
    expect(rows()).toEqual(["pasta"]);
    expect(document.querySelector('[role="timer"]')?.textContent).toBe("09:00");
    const link = document.querySelector(".timer__context a");
    expect(link?.getAttribute("href")).toBe("/recipe/abc");
    expect(link?.textContent).toBe("Carbonara · step 2");
    expect(saved.at(-1)?.map((t) => t.id)).toEqual(["pasta"]);
  });

  test("ticks from the clock and pauses / resumes", async () => {
    await advance(2 * MIN + 500);
    expect(document.querySelector('[role="timer"]')?.textContent).toBe("07:00");
    await click("Pause");
    await advance(30 * MIN);
    expect(document.querySelector('[role="timer"]')?.textContent).toBe("07:00");
    expect(document.querySelector(".timer__status")?.textContent).toBe(
      "Paused",
    );
    await click("Resume");
    await click("Add 1 minute to Pasta");
    expect(document.querySelector('[role="timer"]')?.textContent).toBe("08:00");
  });

  test("a finished timer rings, announces, and flashes until acknowledged", async () => {
    await advance(8 * MIN + 2_000);
    expect(fake.fired.map((f) => f.id)).toEqual(["pasta"]);
    expect(fake.fired[0]?.title).toBe("Pasta is done");
    expect(fake.fired[0]?.body).toBe("Carbonara · step 2");
    expect(document.querySelector('[aria-live="assertive"]')?.textContent).toBe(
      "Pasta timer is done. Carbonara · step 2.",
    );
    const row = document.querySelector("li.timer");
    expect(row?.className).toContain("timer--ringing");
    expect(document.querySelector(".timer__status")?.textContent).toBe(
      "Done just now",
    );

    await advance(3 * MIN);
    expect(fake.fired).toHaveLength(1); // rings once, not per tick
    expect(document.querySelector(".timer__status")?.textContent).toBe(
      "Done 3 min ago",
    );

    await click("Acknowledge");
    expect(fake.stopped).toContain("pasta");
    expect(document.querySelector("li.timer")?.className).not.toContain(
      "timer--ringing",
    );
  });

  test("snoozing a finished timer silences it and restarts the count", async () => {
    await act(async () => {
      timersApi().restart("pasta");
    });
    await advance(10 * MIN);
    expect(fake.fired).toHaveLength(2);
    await click("Add 1 minute to Pasta");
    expect(fake.alarm.ringing()).toEqual([]);
    expect(document.querySelector('[role="timer"]')?.textContent).toBe("01:00");
  });

  test("sorts by remaining time and collapses into a pill when many", async () => {
    await act(async () => {
      const t = timersApi();
      t.start({
        id: "sauce",
        label: "Sauce",
        context: { title: "Carbonara" },
        durationMs: 20 * MIN,
      });
      t.start({
        id: "eggs",
        label: "Eggs",
        context: { title: "Carbonara" },
        durationMs: 30_000,
      });
    });
    // Three timers: folded into a pill showing the most urgent one.
    expect(rows()).toEqual([]);
    const pill = document.querySelector(".timer-pill");
    expect(pill?.textContent).toContain("3 timers");
    expect(pill?.textContent).toContain("Eggs");
    await click(pill?.textContent ?? "");
    expect(rows()).toEqual(["eggs", "pasta", "sauce"]);
    await click("Collapse");
    expect(rows()).toEqual([]);

    // A ringing timer forces the dock open.
    await advance(31_000);
    expect(rows()).toEqual(["eggs", "pasta", "sauce"]);
  });

  test("dismiss removes a timer and stops its alarm", async () => {
    await click("Dismiss Eggs timer");
    expect(rows()).not.toContain("eggs");
    expect(fake.alarm.ringing()).not.toContain("eggs");
    expect(
      saved
        .at(-1)
        ?.map((t) => t.id)
        .sort(),
    ).toEqual(["pasta", "sauce"]);
  });
});
