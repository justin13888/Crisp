import { beforeEach, describe, expect, test } from "bun:test";
import {
  applyUpdate,
  type ContainerLike,
  isUpdateAvailable,
  onUpdateAvailable,
  type RegistrationLike,
  registerServiceWorker,
  resetServiceWorkerState,
  type WorkerLike,
} from "./swRegistration";

class FakeWorker extends EventTarget implements WorkerLike {
  state = "installing";
  messages: unknown[] = [];
  postMessage(message: unknown): void {
    this.messages.push(message);
  }
  become(state: string): void {
    this.state = state;
    this.dispatchEvent(new Event("statechange"));
  }
}

class FakeRegistration extends EventTarget implements RegistrationLike {
  installing: FakeWorker | null = null;
  waiting: FakeWorker | null = null;
  updates = 0;
  async update(): Promise<void> {
    this.updates++;
  }
  find(worker: FakeWorker): void {
    this.installing = worker;
    this.dispatchEvent(new Event("updatefound"));
  }
}

class FakeContainer extends EventTarget implements ContainerLike {
  controller: unknown;
  registered: string[] = [];
  constructor(
    public registration = new FakeRegistration(),
    controlled = true,
    private fail = false,
  ) {
    super();
    this.controller = controlled ? {} : null;
  }
  async register(url: string): Promise<RegistrationLike> {
    if (this.fail) throw new Error("SecurityError");
    this.registered.push(url);
    return this.registration;
  }
}

function fakePage() {
  const visible: (() => void)[] = [];
  let reloads = 0;
  return {
    page: {
      reload: () => {
        reloads++;
      },
      onVisible: (fn: () => void) => {
        visible.push(fn);
      },
    },
    show: () => {
      for (const fn of visible) fn();
    },
    reloads: () => reloads,
  };
}

beforeEach(resetServiceWorkerState);

describe("registerServiceWorker", () => {
  test("returns null when unsupported or registration fails", async () => {
    expect(await registerServiceWorker(undefined, fakePage().page)).toBeNull();
    const failing = new FakeContainer(new FakeRegistration(), true, true);
    expect(await registerServiceWorker(failing, fakePage().page)).toBeNull();
  });

  test("registers /sw.js and checks for updates when shown", async () => {
    const container = new FakeContainer();
    const p = fakePage();
    await registerServiceWorker(container, p.page);
    expect(container.registered).toEqual(["/sw.js"]);
    p.show();
    expect(container.registration.updates).toBe(1);
  });

  test("signals an update once a new worker has installed", async () => {
    const container = new FakeContainer();
    await registerServiceWorker(container, fakePage().page);
    let notified = 0;
    onUpdateAvailable(() => {
      notified++;
    });
    const worker = new FakeWorker();
    container.registration.find(worker);
    expect(isUpdateAvailable()).toBe(false);
    worker.become("installed");
    expect(isUpdateAvailable()).toBe(true);
    expect(notified).toBe(1);
  });

  test("a first install (no controller) is not an update", async () => {
    const container = new FakeContainer(new FakeRegistration(), false);
    await registerServiceWorker(container, fakePage().page);
    const worker = new FakeWorker();
    container.registration.find(worker);
    worker.become("installed");
    expect(isUpdateAvailable()).toBe(false);
  });

  test("picks up a worker already waiting at load", async () => {
    const registration = new FakeRegistration();
    registration.waiting = new FakeWorker();
    await registerServiceWorker(
      new FakeContainer(registration),
      fakePage().page,
    );
    expect(isUpdateAvailable()).toBe(true);
  });

  test("applyUpdate activates the waiting worker, then reloads", async () => {
    const registration = new FakeRegistration();
    const worker = new FakeWorker();
    registration.waiting = worker;
    const container = new FakeContainer(registration);
    const p = fakePage();
    await registerServiceWorker(container, p.page);

    // A controller change not triggered by us does not reload.
    container.dispatchEvent(new Event("controllerchange"));
    expect(p.reloads()).toBe(0);

    applyUpdate();
    expect(worker.messages).toEqual([{ type: "SKIP_WAITING" }]);
    container.dispatchEvent(new Event("controllerchange"));
    expect(p.reloads()).toBe(1);
  });

  test("applyUpdate is a no-op with nothing waiting", () => {
    expect(() => applyUpdate()).not.toThrow();
  });
});
