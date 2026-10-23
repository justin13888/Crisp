/** The icons used by the recipe and cook screens (24px stroke icons). */
const PATHS = {
  timer: "M10 2h4M12 14l3-3M12 22a8 8 0 1 0 0-16 8 8 0 0 0 0 16z",
  flame:
    "M12 22c4 0 7-3 7-7 0-3-2-6-4-8 0 3-2 4-3 4 0-3-1-6-4-9-1 4-5 7-5 12 0 5 4 8 9 8z",
  thermo: "M14 14.8V4a2 2 0 1 0-4 0v10.8a4 4 0 1 0 4 0zM12 9v8",
  close: "M18 6 6 18M6 6l12 12",
  list: "M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01",
  sun: "M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10zM12 1v2M12 21v2M4.2 4.2l1.4 1.4M18.4 18.4l1.4 1.4M1 12h2M21 12h2M4.2 19.8l1.4-1.4M18.4 5.6l1.4-1.4",
  moon: "M21 12.8A9 9 0 1 1 11.2 3 7 7 0 0 0 21 12.8z",
  left: "M15 18l-6-6 6-6",
  right: "M9 18l6-6-6-6",
  check: "M20 6 9 17l-5-5",
  restart: "M3 12a9 9 0 1 0 3-6.7L3 8M3 3v5h5",
  clock: "M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zM12 6v6l4 2",
  users:
    "M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM23 21v-2a4 4 0 0 0-3-3.9M16 3.1a4 4 0 0 1 0 7.8",
  shield: "M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z",
} as const;

/** An icon name. */
export type IconName = keyof typeof PATHS;

/** A decorative stroke icon sized to the surrounding text. */
export function Icon({
  name,
  className,
}: {
  name: IconName;
  className?: string;
}) {
  return (
    <svg
      className={`icon-svg${className ? ` ${className}` : ""}`}
      viewBox="0 0 24 24"
      width="1em"
      height="1em"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
