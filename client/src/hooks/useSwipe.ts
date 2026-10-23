import { type RefObject, useEffect, useRef } from "react";

/** A horizontal swipe direction. */
export type SwipeDirection = "left" | "right";

/** Swipe recognition thresholds. */
export interface SwipeOptions {
  /** Minimum horizontal travel in px (default 60). */
  minDistance?: number;
  /** Maximum |dy| / |dx| — rejects vertical scrolls (default 0.6). */
  maxSlope?: number;
  /** Maximum gesture duration in ms (default 700). */
  maxDurationMs?: number;
}

/** A completed pointer gesture. */
export interface Gesture {
  /** Horizontal travel (px, positive = right). */
  dx: number;
  /** Vertical travel (px). */
  dy: number;
  /** Duration (ms). */
  dt: number;
}

/** Classifies a gesture as a left/right swipe, or `null`. */
export function classifySwipe(
  gesture: Gesture,
  options: SwipeOptions = {},
): SwipeDirection | null {
  const { minDistance = 60, maxSlope = 0.6, maxDurationMs = 700 } = options;
  const ax = Math.abs(gesture.dx);
  if (ax < minDistance || gesture.dt > maxDurationMs || gesture.dt < 0) {
    return null;
  }
  if (Math.abs(gesture.dy) / ax > maxSlope) return null;
  return gesture.dx < 0 ? "left" : "right";
}

/** Swipe callbacks. */
export interface SwipeHandlers extends SwipeOptions {
  /** Finger moved right-to-left (typically "next"). */
  onSwipeLeft?: () => void;
  /** Finger moved left-to-right (typically "previous"). */
  onSwipeRight?: () => void;
}

/**
 * Recognizes horizontal touch/pen swipes on `ref`'s element. Give the element
 * `touch-action: pan-y` (class `swipe-area`) so vertical scrolling still works.
 */
export function useSwipe<T extends HTMLElement>(
  ref: RefObject<T | null>,
  handlers: SwipeHandlers,
): void {
  const latest = useRef(handlers);
  latest.current = handlers;

  useEffect(() => {
    const element = ref.current;
    if (!element) return;
    let start: { x: number; y: number; t: number; id: number } | null = null;

    const onDown = (event: PointerEvent): void => {
      if (event.pointerType === "mouse" || !event.isPrimary) return;
      start = {
        x: event.clientX,
        y: event.clientY,
        t: event.timeStamp,
        id: event.pointerId,
      };
    };
    const onUp = (event: PointerEvent): void => {
      if (!start || event.pointerId !== start.id) return;
      const direction = classifySwipe(
        {
          dx: event.clientX - start.x,
          dy: event.clientY - start.y,
          dt: event.timeStamp - start.t,
        },
        latest.current,
      );
      start = null;
      if (direction === "left") latest.current.onSwipeLeft?.();
      if (direction === "right") latest.current.onSwipeRight?.();
    };
    const onCancel = (): void => {
      start = null;
    };

    element.addEventListener("pointerdown", onDown);
    element.addEventListener("pointerup", onUp);
    element.addEventListener("pointercancel", onCancel);
    return () => {
      element.removeEventListener("pointerdown", onDown);
      element.removeEventListener("pointerup", onUp);
      element.removeEventListener("pointercancel", onCancel);
    };
  }, [ref]);
}
