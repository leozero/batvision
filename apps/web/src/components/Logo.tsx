import { cx } from "./ui";

/** Bat signal: a bat cut out of an amber disc. */
export function LogoMark({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 32 32" aria-hidden="true" className={cx("size-7", className)}>
      <defs>
        <radialGradient id="bv-glow" cx="50%" cy="40%" r="60%">
          <stop offset="0%" stopColor="#ffd77a" />
          <stop offset="100%" stopColor="#f4b740" />
        </radialGradient>
      </defs>
      <circle cx="16" cy="16" r="15" fill="url(#bv-glow)" />
      <path
        fill="#0b0b0f"
        d="M16 11.2c.6-.9 1.1-1.7 1.3-2.5.5 1 .6 2 .4 3 1.6-.5 3.6-.4 5.6.4-1.6.5-2.4 1.5-2.3 2.8 1.7-.6 3.3-.5 4.7.3-2.3.6-3.6 2-3.9 4-1.9-1.3-3.7-1.4-5.3-.3-.3.4-.4 1-.5 1.6-.1-.6-.2-1.2-.5-1.6-1.6-1.1-3.4-1-5.3.3-.3-2-1.6-3.4-3.9-4 1.4-.8 3-.9 4.7-.3.1-1.3-.7-2.3-2.3-2.8 2-.8 4-.9 5.6-.4-.2-1-.1-2 .4-3 .2.8.7 1.6 1.3 2.5z"
      />
    </svg>
  );
}

export function Logo({ className }: { className?: string }) {
  return (
    <span
      className={cx("inline-flex items-center gap-2 text-lg font-bold tracking-tight", className)}
    >
      <LogoMark />
      batvision
    </span>
  );
}
