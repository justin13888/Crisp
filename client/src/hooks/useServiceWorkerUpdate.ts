import { useSyncExternalStore } from "react";
import {
  applyUpdate,
  isUpdateAvailable,
  onUpdateAvailable,
} from "../lib/swRegistration";

/** Whether a new app version is waiting, and how to switch to it. */
export function useServiceWorkerUpdate(): {
  updateAvailable: boolean;
  applyUpdate: () => void;
} {
  const updateAvailable = useSyncExternalStore(
    onUpdateAvailable,
    isUpdateAvailable,
    () => false,
  );
  return { updateAvailable, applyUpdate };
}
