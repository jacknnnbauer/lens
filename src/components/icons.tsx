// 线条图标（stroke = currentColor），尺寸与设计稿一致。全部 aria-hidden，由外层按钮提供 aria-label。

import type { SVGProps } from "react";

const base = (size: number): SVGProps<SVGSVGElement> => ({
  width: size,
  height: size,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.8,
  strokeLinecap: "round",
  strokeLinejoin: "round",
  "aria-hidden": true,
});

export const SearchIcon = ({ size = 18 }) => (
  <svg {...base(size)}>
    <circle cx="11" cy="11" r="7" />
    <path d="M16 16l4 4" />
  </svg>
);

export const LibraryIcon = ({ size = 18 }) => (
  <svg {...base(size)}>
    <path d="M4 5.5A1.5 1.5 0 015.5 4H10v16H5.5A1.5 1.5 0 014 18.5z" />
    <path d="M14 4h4.5A1.5 1.5 0 0120 5.5v13a1.5 1.5 0 01-1.5 1.5H14z" />
  </svg>
);

export const GearIcon = ({ size = 18 }) => (
  <svg {...base(size)}>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1L7 17M17 7l2.1-2.1" />
  </svg>
);

export const ChevronIcon = ({ size = 14, open = false }) => (
  <svg {...base(size)} style={{ transform: open ? "rotate(180deg)" : undefined, transition: "transform 120ms" }}>
    <path d="M6 9l6 6 6-6" />
  </svg>
);

export const PlusIcon = ({ size = 16 }) => (
  <svg {...base(size)}>
    <path d="M12 5v14M5 12h14" />
  </svg>
);

export const CloseIcon = ({ size = 16 }) => (
  <svg {...base(size)}>
    <path d="M6 6l12 12M18 6L6 18" />
  </svg>
);

export const ClockIcon = ({ size = 18 }) => (
  <svg {...base(size)}>
    <circle cx="12" cy="12" r="8" />
    <path d="M12 8v4l3 2" />
  </svg>
);

export const StarIcon = ({ size = 18, filled = false }) => (
  <svg {...base(size)} fill={filled ? "currentColor" : "none"}>
    <path d="M12 3.5l2.6 5.3 5.9.9-4.25 4.1 1 5.8L12 16.9l-5.25 2.7 1-5.8L3.5 9.7l5.9-.9z" />
  </svg>
);

export const BookmarkIcon = ({ size = 18 }) => (
  <svg {...base(size)}>
    <path d="M6 4h12v16l-6-4-6 4z" />
  </svg>
);
