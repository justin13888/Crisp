import { useEffect, useRef } from "react";

/** A parsed key combination such as `shift+n` or `mod+k`. */
export interface Hotkey {
  /** The key, lower-cased (`arrowright`, `n`, ` `). */
  key: string;
  /** Requires Shift. */
  shift: boolean;
  /** Requires Alt/Option. */
  alt: boolean;
  /** Requires Ctrl. */
  ctrl: boolean;
  /** Requires Meta/Cmd. */
  meta: boolean;
}

/** The keyboard-event fields matching needs. */
export interface KeyEventLike {
  /** `KeyboardEvent.key`. */
  key: string;
  /** Modifier states. */
  shiftKey: boolean;
  /** Alt state. */
  altKey: boolean;
  /** Ctrl state. */
  ctrlKey: boolean;
  /** Meta state. */
  metaKey: boolean;
}

const ALIASES: Record<string, string> = {
  space: " ",
  esc: "escape",
  left: "arrowleft",
  right: "arrowright",
  up: "arrowup",
  down: "arrowdown",
  plus: "+",
};

/**
 * Parses a combo string: modifiers (`shift`, `alt`, `ctrl`, `meta`, `mod` =
 * Cmd on Apple platforms else Ctrl) joined by `+` with a final key.
 */
export function parseHotkey(combo: string, isApple = false): Hotkey {
  const parts = combo.toLowerCase().split("+");
  const raw = parts.pop() ?? "";
  const key = ALIASES[raw] ?? raw;
  const mods = new Set(parts);
  const mod = mods.has("mod");
  return {
    key,
    shift: mods.has("shift"),
    alt: mods.has("alt"),
    ctrl: mods.has("ctrl") || (mod && !isApple),
    meta: mods.has("meta") || (mod && isApple),
  };
}

/** Whether an event matches a hotkey exactly (no extra modifiers). */
export function matchesHotkey(event: KeyEventLike, hotkey: Hotkey): boolean {
  return (
    event.key.toLowerCase() === hotkey.key &&
    event.shiftKey === hotkey.shift &&
    event.altKey === hotkey.alt &&
    event.ctrlKey === hotkey.ctrl &&
    event.metaKey === hotkey.meta
  );
}

/** Whether keystrokes on `target` belong to a text field. */
export function isEditableTarget(target: EventTarget | null): boolean {
  if (typeof HTMLElement === "undefined" || !(target instanceof HTMLElement)) {
    return false;
  }
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  if (tag === "TEXTAREA" || tag === "SELECT") return true;
  if (tag !== "INPUT") return false;
  const type = target.getAttribute("type") ?? "text";
  return !["button", "checkbox", "radio", "range", "submit", "reset"].includes(
    type,
  );
}

/** Hotkey options. */
export interface HotkeyOptions {
  /** Disable all bindings (default enabled). */
  enabled?: boolean;
  /** Also fire while typing in text fields (default false). */
  allowInInputs?: boolean;
}

/**
 * Binds window-level hotkeys, e.g.
 * `useHotkeys({ ArrowRight: next, ArrowLeft: prev, space: toggle })`.
 * Matching bindings call `preventDefault()`.
 */
export function useHotkeys(
  bindings: Record<string, (event: KeyboardEvent) => void>,
  options: HotkeyOptions = {},
): void {
  const { enabled = true, allowInInputs = false } = options;
  const latest = useRef(bindings);
  latest.current = bindings;

  useEffect(() => {
    if (!enabled) return;
    const isApple = /Mac|iPhone|iPad/.test(navigator.platform);
    const onKey = (event: KeyboardEvent): void => {
      if (event.defaultPrevented || event.isComposing) return;
      if (!allowInInputs && isEditableTarget(event.target)) return;
      for (const [combo, handler] of Object.entries(latest.current)) {
        if (matchesHotkey(event, parseHotkey(combo, isApple))) {
          event.preventDefault();
          handler(event);
          return;
        }
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [enabled, allowInInputs]);
}
