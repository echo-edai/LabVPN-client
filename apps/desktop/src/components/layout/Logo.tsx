/** LabVPN 标志：盾牌 + 分流箭头（与应用图标同构）。 */
export function Logo({ size = 34, className }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 64 64"
      fill="none"
      className={className}
      aria-label="LabVPN"
    >
      <defs>
        <linearGradient id="labvpn-bg" x1="4" y1="2" x2="60" y2="62" gradientUnits="userSpaceOnUse">
          <stop stopColor="#0b1f3f" />
          <stop offset="0.62" stopColor="#1a4a9e" />
          <stop offset="1" stopColor="#2563eb" />
        </linearGradient>
        <linearGradient id="labvpn-line" x1="22" y1="16" x2="44" y2="46" gradientUnits="userSpaceOnUse">
          <stop stopColor="#eef2f9" />
          <stop offset="1" stopColor="#93c5fd" />
        </linearGradient>
      </defs>
      <rect x="1" y="1" width="62" height="62" rx="15" fill="url(#labvpn-bg)" />
      <path
        d="M20.5 15.5H43.5V32.5L32 49L20.5 32.5V15.5Z"
        stroke="white"
        strokeOpacity="0.92"
        strokeWidth="2.4"
        strokeLinejoin="round"
      />
      <path
        d="M32 23V29.5M32 29.5L25 37M32 29.5L39 37"
        stroke="url(#labvpn-line)"
        strokeWidth="2.2"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
      <circle cx="32" cy="23" r="2.2" fill="white" />
      <circle cx="25" cy="37.5" r="2.2" fill="white" />
      <circle cx="39" cy="37.5" r="2.2" fill="white" />
      <circle cx="32" cy="29.5" r="2.6" fill="#60a5fa" />
    </svg>
  );
}
