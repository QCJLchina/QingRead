import type { SVGProps } from "react";

/** 统一的线性图标：24 网格、1.6 线宽、currentColor，不引入图标依赖。 */
const PATHS: Record<string, string[]> = {
  book: [
    "M3 5.5A2.5 2.5 0 0 1 5.5 3H19v18H5.5A2.5 2.5 0 0 1 3 18.5v-13Z",
    "M8 3v18",
  ],
  "panel-left": ["M4 4h16v16H4z", "M10 4v16"],
  search: ["M11 19a8 8 0 1 1 0-16 8 8 0 0 1 0 16Z", "m20 20-3.6-3.6"],
  type: ["M5 6h14", "M12 6v13", "M9 19h6"],
  scan: ["M4 8V5a1 1 0 0 1 1-1h3", "M16 4h3a1 1 0 0 1 1 1v3", "M20 16v3a1 1 0 0 1-1 1h-3", "M8 20H5a1 1 0 0 1-1-1v-3"],
  leaf: ["M20 4c0 8-5 12-10 12H6c0-7 4-11 10-11h4Z", "M6 20c0-4 2-7 5-9"],
  "eye-off": ["M3 3l18 18", "M10.6 10.7a3 3 0 0 0 4.2 4.2", "M6.5 6.6C4.3 8 2.8 10 2 12c1.8 4 5.6 6.5 10 6.5 1.7 0 3.2-.4 4.6-1", "M9.8 5.6A11 11 0 0 1 12 5.5c4.4 0 8.2 2.5 10 6.5-.6 1.4-1.5 2.6-2.6 3.6"],
  eye: ["M2 12c1.8-4 5.6-6.5 10-6.5S20.2 8 22 12c-1.8 4-5.6 6.5-10 6.5S3.8 16 2 12Z", "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z"],
  "arrow-left": ["M19 12H5", "m12 5-7 7 7 7"],
  "chevron-left": ["m15 5-7 7 7 7"],
  "chevron-right": ["m9 5 7 7-7 7"],
  "chevron-down": ["m5 9 7 7 7-7"],
  pin: ["M12 17v5", "M8 3h8l-1 6 3 3v1H6v-1l3-3-1-6Z"],
  close: ["M5 5l14 14", "M19 5 5 19"],
  minus: ["M5 12h14"],
  plus: ["M12 5v14", "M5 12h14"],
  grid: ["M4 4h7v7H4z", "M13 4h7v7h-7z", "M4 13h7v7H4z", "M13 13h7v7h-7z"],
  list: ["M4 6h16", "M4 12h16", "M4 18h16"],
  column: ["M12 4v16", "M6 4h12a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2Z"],
  refresh: ["M20 11a8 8 0 1 0-2 6", "M20 5v6h-6"],
  settings: ["M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z", "M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-2.9 1.2 2 2 0 1 1-4 0 1.7 1.7 0 0 0-2.9-1.2l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1A1.7 1.7 0 0 0 3 15a2 2 0 1 1 0-4 1.7 1.7 0 0 0 1.4-2.9l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1A1.7 1.7 0 0 0 10 4.1a2 2 0 1 1 4 0 1.7 1.7 0 0 0 2.9 1.2l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1A1.7 1.7 0 0 0 21 11a2 2 0 1 1 0 4 1.7 1.7 0 0 0-1.6 1Z"],
  undo: ["M9 14 4 9l5-5", "M4 9h10a6 6 0 0 1 0 12h-3"],
};

export interface IconProps extends Omit<SVGProps<SVGSVGElement>, "name"> {
  name: keyof typeof PATHS | string;
  size?: number;
}

export default function Icon({ name, size = 17, ...rest }: IconProps) {
  const paths = PATHS[name] ?? [];
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.6}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...rest}
    >
      {paths.map((d) => (
        <path key={d} d={d} />
      ))}
    </svg>
  );
}
