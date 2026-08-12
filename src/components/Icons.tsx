import type { SVGProps } from "react";
import type { MediaKind } from "../types";

const base = (props: SVGProps<SVGSVGElement>) => ({
  width: 16,
  height: 16,
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
  ...props,
});

export const IconCheck = (props: SVGProps<SVGSVGElement>) => (
  <svg {...base(props)}>
    <path d="M20 6 9 17l-5-5" />
  </svg>
);

export const IconClose = (props: SVGProps<SVGSVGElement>) => (
  <svg {...base(props)}>
    <path d="M18 6 6 18M6 6l12 12" />
  </svg>
);

export const IconFolder = (props: SVGProps<SVGSVGElement>) => (
  <svg {...base(props)}>
    <path d="M4 20a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h5l2 3h7a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2z" />
  </svg>
);

export const IconAlert = (props: SVGProps<SVGSVGElement>) => (
  <svg {...base(props)}>
    <circle cx="12" cy="12" r="9" />
    <path d="M12 8v5M12 16.5v.01" />
  </svg>
);

const IconVideo = (props: SVGProps<SVGSVGElement>) => (
  <svg {...base(props)}>
    <rect x="2" y="5" width="14" height="14" rx="2" />
    <path d="m16 12 6-4v8z" />
  </svg>
);

const IconAudio = (props: SVGProps<SVGSVGElement>) => (
  <svg {...base(props)}>
    <path d="M9 18V6l10-2v12" />
    <circle cx="6" cy="18" r="3" />
    <circle cx="16" cy="16" r="3" />
  </svg>
);

const IconImage = (props: SVGProps<SVGSVGElement>) => (
  <svg {...base(props)}>
    <rect x="3" y="4" width="18" height="16" rx="2" />
    <circle cx="8.5" cy="9.5" r="1.5" />
    <path d="m21 16-5-5-9 9" />
  </svg>
);

const IconDocument = (props: SVGProps<SVGSVGElement>) => (
  <svg {...base(props)}>
    <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
    <path d="M14 3v5h5M9 13h6M9 17h4" />
  </svg>
);

const IconModel = (props: SVGProps<SVGSVGElement>) => (
  <svg {...base(props)}>
    <path d="m12 2 9 5v10l-9 5-9-5V7z" />
    <path d="m3 7 9 5 9-5M12 12v10" />
  </svg>
);

const IconFile = (props: SVGProps<SVGSVGElement>) => (
  <svg {...base(props)}>
    <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
    <path d="M14 3v5h5" />
  </svg>
);

export function KindIcon({ kind, ...props }: { kind: MediaKind } & SVGProps<SVGSVGElement>) {
  switch (kind) {
    case "video":
      return <IconVideo {...props} />;
    case "audio":
      return <IconAudio {...props} />;
    case "image":
      return <IconImage {...props} />;
    case "document":
      return <IconDocument {...props} />;
    case "model":
      return <IconModel {...props} />;
    default:
      return <IconFile {...props} />;
  }
}
