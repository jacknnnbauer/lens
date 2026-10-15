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

export const DownloadIcon = ({ size = 18 }) => (
  <svg {...base(size)}>
    <path d="M12 4v11" />
    <path d="M7 10l5 5 5-5" />
    <path d="M5 19h14" />
  </svg>
);

/** 侧栏开关 */
export const SidebarIcon = ({ size = 16 }) => (
  <svg {...base(size)}>
    <rect x="3.5" y="4.5" width="17" height="15" rx="2" />
    <path d="M9.5 4.5v15" />
  </svg>
);

/** 右侧栏开关（与 SidebarIcon 左右对称） */
export const ContextPanelIcon = ({ size = 16 }) => (
  <svg {...base(size)}>
    <rect x="3.5" y="4.5" width="17" height="15" rx="2" />
    <path d="M14.5 4.5v15" />
  </svg>
);

/** 沉浸模式：四角向外 */
export const ImmersiveIcon = ({ size = 18 }) => (
  <svg {...base(size)}>
    <path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" />
  </svg>
);

/** 窗口按钮：线宽 1，与系统标题栏一致 */
const thin = (size: number): SVGProps<SVGSVGElement> => ({ ...base(size), viewBox: "0 0 10 10", strokeWidth: 1, strokeLinecap: "butt", strokeLinejoin: "miter" });

export const MinimizeIcon = () => (
  <svg {...thin(10)}>
    <path d="M0 5.5h10" />
  </svg>
);

export const MaximizeIcon = ({ restore = false }) =>
  restore ? (
    <svg {...thin(10)}>
      <rect x="0.5" y="2.5" width="7" height="7" />
      <path d="M2.5 2.5V0.5h7v7h-2" />
    </svg>
  ) : (
    <svg {...thin(10)}>
      <rect x="0.5" y="0.5" width="9" height="9" />
    </svg>
  );

export const WindowCloseIcon = () => (
  <svg {...thin(10)}>
    <path d="M0 0l10 10M10 0L0 10" />
  </svg>
);

/** 拖动把手：两列三行的点 */
export const GripIcon = ({ size = 16 }) => (
  <svg width={size} height={size} viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
    <circle cx="5.5" cy="3.5" r="1.4" />
    <circle cx="10.5" cy="3.5" r="1.4" />
    <circle cx="5.5" cy="8" r="1.4" />
    <circle cx="10.5" cy="8" r="1.4" />
    <circle cx="5.5" cy="12.5" r="1.4" />
    <circle cx="10.5" cy="12.5" r="1.4" />
  </svg>
);
